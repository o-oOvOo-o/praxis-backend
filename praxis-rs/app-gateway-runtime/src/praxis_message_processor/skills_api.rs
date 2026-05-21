use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::SkillDependencies;
use praxis_app_gateway_protocol::SkillErrorInfo;
use praxis_app_gateway_protocol::SkillInterface;
use praxis_app_gateway_protocol::SkillMetadata;
use praxis_app_gateway_protocol::SkillToolDependency;
use praxis_app_gateway_protocol::SkillsConfigWriteParams;
use praxis_app_gateway_protocol::SkillsConfigWriteResponse;
use praxis_app_gateway_protocol::SkillsListEntry;
use praxis_app_gateway_protocol::SkillsListParams;
use praxis_app_gateway_protocol::SkillsListResponse;
use praxis_core::config::edit::ConfigEdit;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::config_loader::CloudRequirementsLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::config_loader::load_config_layers_state;
use praxis_features::Feature;
use praxis_utils_absolute_path::AbsolutePathBuf;
use tracing::warn;

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
                CloudRequirementsLoader::default(),
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
            let skills = skills_to_info(&outcome.skills, &outcome.disabled_paths);
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
}

fn skills_to_info(
    skills: &[praxis_core::skills::SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
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
            }
        })
        .collect()
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
