use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::SkillDependencies;
use praxis_app_gateway_protocol::SkillErrorInfo;
use praxis_app_gateway_protocol::SkillInterface;
use praxis_app_gateway_protocol::SkillMetadata;
use praxis_app_gateway_protocol::SkillToolDependency;
use praxis_app_gateway_protocol::SkillsConfigWriteParams;
use praxis_app_gateway_protocol::SkillsConfigWriteResponse;
use praxis_app_gateway_protocol::SkillsInstallParams;
use praxis_app_gateway_protocol::SkillsInstallResponse;
use praxis_app_gateway_protocol::SkillsListEntry;
use praxis_app_gateway_protocol::SkillsListParams;
use praxis_app_gateway_protocol::SkillsListResponse;
use praxis_app_gateway_protocol::SkillsUninstallParams;
use praxis_app_gateway_protocol::SkillsUninstallResponse;
use praxis_core::config::edit::ConfigEdit;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::config_loader::load_config_layers_state;
use praxis_features::Feature;
use praxis_protocol::protocol::SkillScope as CoreSkillScope;
use praxis_utils_absolute_path::AbsolutePathBuf;
use tracing::warn;
use uuid::Uuid;

use super::PraxisMessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn skills_list(
        &self,
        request_id: ConnectionRequestId,
        params: SkillsListParams,
    ) {
        let SkillsListParams {
            cwds,
            force_reload,
            per_cwd_extra_user_roots,
        } = params;
        let cwds = if cwds.is_empty() {
            vec![self.config.cwd.to_path_buf()]
        } else {
            cwds
        };
        let cwd_set: HashSet<PathBuf> = cwds.iter().cloned().collect();

        let mut extra_roots_by_cwd: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for entry in per_cwd_extra_user_roots.unwrap_or_default() {
            if !cwd_set.contains(&entry.cwd) {
                warn!(
                    cwd = %entry.cwd.display(),
                    "ignoring per-cwd extra roots for cwd not present in skills/list cwds"
                );
                continue;
            }

            let mut valid_extra_roots = Vec::new();
            for root in entry.extra_user_roots {
                if !root.is_absolute() {
                    self.send_invalid_request_error(
                        request_id,
                        format!(
                            "skills/list perCwdExtraUserRoots extraUserRoots paths must be absolute: {}",
                            root.display()
                        ),
                    )
                    .await;
                    return;
                }
                valid_extra_roots.push(root);
            }
            extra_roots_by_cwd
                .entry(entry.cwd)
                .or_default()
                .extend(valid_extra_roots);
        }

        let config = match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(config) => config,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let skills_manager = self.thread_manager.skills_manager();
        let plugins_manager = self.thread_manager.plugins_manager();
        let cli_overrides = self.current_cli_overrides();
        let mut data = Vec::new();
        for cwd in cwds {
            let extra_roots = extra_roots_by_cwd
                .get(&cwd)
                .map_or(&[][..], std::vec::Vec::as_slice);
            let cwd_abs = match AbsolutePathBuf::try_from(cwd.as_path()) {
                Ok(path) => path,
                Err(err) => {
                    let error_path = cwd.clone();
                    data.push(SkillsListEntry {
                        cwd,
                        skills: Vec::new(),
                        errors: errors_to_info(&[praxis_core::skills::SkillError {
                            path: error_path,
                            message: err.to_string(),
                        }]),
                    });
                    continue;
                }
            };
            let config_layer_stack = match load_config_layers_state(
                &self.config.praxis_home,
                Some(cwd_abs),
                &cli_overrides,
                LoaderOverrides::default(),
                CloudConfigBundleLoader::default(),
            )
            .await
            {
                Ok(config_layer_stack) => config_layer_stack,
                Err(err) => {
                    let error_path = cwd.clone();
                    data.push(SkillsListEntry {
                        cwd,
                        skills: Vec::new(),
                        errors: errors_to_info(&[praxis_core::skills::SkillError {
                            path: error_path,
                            message: err.to_string(),
                        }]),
                    });
                    continue;
                }
            };
            let effective_skill_roots = plugins_manager.effective_skill_roots_for_layer_stack(
                &config_layer_stack,
                config.features.enabled(Feature::Plugins),
            );
            let skills_input = praxis_core::skills::SkillsLoadInput::new(
                cwd.clone(),
                effective_skill_roots,
                config_layer_stack,
                config.bundled_skills_enabled(),
            );
            let outcome = skills_manager
                .skills_for_cwd_with_extra_user_roots(&skills_input, force_reload, extra_roots)
                .await;
            let errors = errors_to_info(&outcome.errors);
            let skills = skills_to_info(
                &outcome.skills,
                &outcome.disabled_paths,
                &self.config.praxis_home,
            );
            data.push(SkillsListEntry {
                cwd,
                skills,
                errors,
            });
        }
        self.outgoing
            .send_response(request_id, SkillsListResponse { data })
            .await;
    }

    pub(super) async fn skills_config_write(
        &self,
        request_id: ConnectionRequestId,
        params: SkillsConfigWriteParams,
    ) {
        let SkillsConfigWriteParams {
            path,
            name,
            enabled,
        } = params;
        let edit = match (path, name) {
            (Some(path), None) => ConfigEdit::SetSkillConfig {
                path: path.into_path_buf(),
                enabled,
            },
            (None, Some(name)) if !name.trim().is_empty() => {
                ConfigEdit::SetSkillConfigByName { name, enabled }
            }
            _ => {
                let error = JSONRPCErrorError {
                    code: INVALID_PARAMS_ERROR_CODE,
                    message: "skills/config/write requires exactly one of path or name".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let edits = vec![edit];
        let result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits(edits)
            .apply()
            .await;

        match result {
            Ok(()) => {
                self.thread_manager.plugins_manager().clear_cache();
                self.thread_manager.skills_manager().clear_cache();
                self.outgoing
                    .send_response(
                        request_id,
                        SkillsConfigWriteResponse {
                            effective_enabled: enabled,
                        },
                    )
                    .await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to update skill settings: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    pub(super) async fn skills_install(
        &self,
        request_id: ConnectionRequestId,
        params: SkillsInstallParams,
    ) {
        let source_skill_path = params.source_skill_path.into_path_buf();
        let skills_root = self.config.praxis_home.join("skills");
        let result = tokio::task::spawn_blocking(move || {
            install_user_skill(&source_skill_path, &skills_root)
        })
        .await;
        let installed_skill_path = match result {
            Ok(Ok(path)) => path,
            Ok(Err(SkillMutationError::Invalid(message))) => {
                self.send_invalid_request_error(request_id, message).await;
                return;
            }
            Ok(Err(SkillMutationError::Io(message))) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
            Err(error) => {
                self.send_internal_error(request_id, format!("skill install task failed: {error}"))
                    .await;
                return;
            }
        };
        self.clear_skill_caches();
        let installed_skill_path = match AbsolutePathBuf::try_from(installed_skill_path) {
            Ok(path) => path,
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("installed skill path is not absolute: {error}"),
                )
                .await;
                return;
            }
        };
        self.outgoing
            .send_response(
                request_id,
                SkillsInstallResponse {
                    installed_skill_path,
                },
            )
            .await;
    }

    pub(super) async fn skills_uninstall(
        &self,
        request_id: ConnectionRequestId,
        params: SkillsUninstallParams,
    ) {
        let skill_path = params.skill_path.into_path_buf();
        let skills_root = self.config.praxis_home.join("skills");
        let validated = match validate_installed_user_skill(&skill_path, &skills_root) {
            Ok(validated) => validated,
            Err(SkillMutationError::Invalid(message)) => {
                self.send_invalid_request_error(request_id, message).await;
                return;
            }
            Err(SkillMutationError::Io(message)) => {
                self.send_internal_error(request_id, message).await;
                return;
            }
        };
        let result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits(vec![ConfigEdit::SetSkillConfig {
                path: validated.skill_path.clone(),
                enabled: true,
            }])
            .apply()
            .await;
        if let Err(error) = result {
            self.send_internal_error(
                request_id,
                format!("failed to clear skill settings before uninstall: {error}"),
            )
            .await;
            return;
        }
        let removed_skill_path = validated.skill_path.clone();
        let skill_dir = validated.skill_dir;
        let removed_dir = skill_dir.clone();
        match tokio::task::spawn_blocking(move || fs::remove_dir_all(&removed_dir)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.send_internal_error(
                    request_id,
                    format!("failed to remove {}: {error}", skill_dir.display()),
                )
                .await;
                return;
            }
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("skill uninstall task failed: {error}"),
                )
                .await;
                return;
            }
        }
        self.clear_skill_caches();
        let removed_skill_path = match AbsolutePathBuf::try_from(removed_skill_path) {
            Ok(path) => path,
            Err(error) => {
                self.send_internal_error(
                    request_id,
                    format!("removed skill path is not absolute: {error}"),
                )
                .await;
                return;
            }
        };
        self.outgoing
            .send_response(request_id, SkillsUninstallResponse { removed_skill_path })
            .await;
    }

    fn clear_skill_caches(&self) {
        self.thread_manager.plugins_manager().clear_cache();
        self.thread_manager.skills_manager().clear_cache();
    }
}

struct ValidatedInstalledSkill {
    skill_path: PathBuf,
    skill_dir: PathBuf,
}

enum SkillMutationError {
    Invalid(String),
    Io(String),
}

fn install_user_skill(
    source_skill_path: &Path,
    skills_root: &Path,
) -> Result<PathBuf, SkillMutationError> {
    let source_skill_path = source_skill_path.canonicalize().map_err(|error| {
        SkillMutationError::Invalid(format!(
            "skill source {} cannot be resolved: {error}",
            source_skill_path.display()
        ))
    })?;
    if source_skill_path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
        return Err(SkillMutationError::Invalid(format!(
            "skill source must point to SKILL.md: {}",
            source_skill_path.display()
        )));
    }
    let source_dir = source_skill_path.parent().ok_or_else(|| {
        SkillMutationError::Invalid("skill source has no containing directory".to_owned())
    })?;
    let install_dir_name = source_dir.file_name().ok_or_else(|| {
        SkillMutationError::Invalid("skill source directory has no name".to_owned())
    })?;
    if install_dir_name.to_string_lossy().starts_with('.') {
        return Err(SkillMutationError::Invalid(
            "hidden skill directories cannot be installed".to_owned(),
        ));
    }
    fs::create_dir_all(skills_root).map_err(|error| {
        SkillMutationError::Io(format!(
            "failed to create skill root {}: {error}",
            skills_root.display()
        ))
    })?;
    let destination = skills_root.join(install_dir_name);
    if destination.exists() {
        return Err(SkillMutationError::Invalid(format!(
            "skill is already installed at {}",
            destination.display()
        )));
    }
    let staging = skills_root.join(format!(".skill-install-{}", Uuid::now_v7()));
    fs::create_dir(&staging).map_err(|error| {
        SkillMutationError::Io(format!(
            "failed to create skill staging directory {}: {error}",
            staging.display()
        ))
    })?;
    let install_result = copy_skill_directory(source_dir, &staging).and_then(|()| {
        fs::rename(&staging, &destination).map_err(|error| {
            SkillMutationError::Io(format!(
                "failed to commit skill install to {}: {error}",
                destination.display()
            ))
        })
    });
    if install_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    install_result?;
    Ok(destination.join("SKILL.md"))
}

fn copy_skill_directory(source: &Path, destination: &Path) -> Result<(), SkillMutationError> {
    let entries = fs::read_dir(source).map_err(|error| {
        SkillMutationError::Io(format!("failed to read {}: {error}", source.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            SkillMutationError::Io(format!("failed to read skill directory entry: {error}"))
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| {
            SkillMutationError::Io(format!(
                "failed to inspect {}: {error}",
                source_path.display()
            ))
        })?;
        if file_type.is_symlink() {
            return Err(SkillMutationError::Invalid(format!(
                "skill packages cannot contain symbolic links: {}",
                source_path.display()
            )));
        }
        if file_type.is_dir() {
            fs::create_dir(&destination_path).map_err(|error| {
                SkillMutationError::Io(format!(
                    "failed to create {}: {error}",
                    destination_path.display()
                ))
            })?;
            copy_skill_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                SkillMutationError::Io(format!(
                    "failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn validate_installed_user_skill(
    skill_path: &Path,
    skills_root: &Path,
) -> Result<ValidatedInstalledSkill, SkillMutationError> {
    let skills_root = skills_root.canonicalize().map_err(|error| {
        SkillMutationError::Io(format!(
            "failed to resolve skill root {}: {error}",
            skills_root.display()
        ))
    })?;
    let skill_path = skill_path.canonicalize().map_err(|error| {
        SkillMutationError::Invalid(format!(
            "installed skill {} cannot be resolved: {error}",
            skill_path.display()
        ))
    })?;
    if skill_path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
        return Err(SkillMutationError::Invalid(format!(
            "installed skill must point to SKILL.md: {}",
            skill_path.display()
        )));
    }
    let skill_dir = skill_path
        .parent()
        .ok_or_else(|| {
            SkillMutationError::Invalid("installed skill has no containing directory".to_owned())
        })?
        .to_path_buf();
    if skill_dir.parent() != Some(skills_root.as_path())
        || skill_dir
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return Err(SkillMutationError::Invalid(format!(
            "only direct user skills under {} can be uninstalled",
            skills_root.display()
        )));
    }
    Ok(ValidatedInstalledSkill {
        skill_path,
        skill_dir,
    })
}

fn skills_to_info(
    skills: &[praxis_core::skills::SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
    praxis_home: &Path,
) -> Vec<SkillMetadata> {
    skills
        .iter()
        .map(|skill| {
            let enabled = !disabled_paths.contains(&skill.path_to_skills_md);
            SkillMetadata {
                name: skill.name.clone(),
                description: skill.description.clone(),
                short_description: skill.short_description.clone(),
                interface: skill.interface.clone().map(|interface| SkillInterface {
                    display_name: interface.display_name,
                    short_description: interface.short_description,
                    icon_small: interface.icon_small,
                    icon_large: interface.icon_large,
                    brand_color: interface.brand_color,
                    default_prompt: interface.default_prompt,
                }),
                dependencies: skill
                    .dependencies
                    .clone()
                    .map(|dependencies| SkillDependencies {
                        tools: dependencies
                            .tools
                            .into_iter()
                            .map(|tool| SkillToolDependency {
                                r#type: tool.r#type,
                                value: tool.value,
                                description: tool.description,
                                transport: tool.transport,
                                command: tool.command,
                                url: tool.url,
                            })
                            .collect(),
                    }),
                path: skill.path_to_skills_md.clone(),
                scope: skill.scope.into(),
                enabled,
                can_uninstall: skill.scope == CoreSkillScope::User
                    && is_direct_user_skill(&skill.path_to_skills_md, praxis_home),
            }
        })
        .collect()
}

fn is_direct_user_skill(skill_path: &Path, praxis_home: &Path) -> bool {
    let Ok(skill_path) = skill_path.canonicalize() else {
        return false;
    };
    let Ok(skills_root) = praxis_home.join("skills").canonicalize() else {
        return false;
    };
    skill_path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        && skill_path
            .parent()
            .is_some_and(|skill_dir| skill_dir.parent() == Some(skills_root.as_path()))
        && !skill_path.parent().is_some_and(|skill_dir| {
            skill_dir
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
}

fn errors_to_info(errors: &[praxis_core::skills::SkillError]) -> Vec<SkillErrorInfo> {
    errors
        .iter()
        .map(|err| SkillErrorInfo {
            path: err.path.clone(),
            message: err.message.clone(),
        })
        .collect()
}
