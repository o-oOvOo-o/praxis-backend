use std::path::PathBuf;

use praxis_features::Feature;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ListSkillsResponseEvent;
use praxis_protocol::protocol::SkillsListEntry;
use praxis_utils_absolute_path::AbsolutePathBuf;

use crate::SkillError;
use crate::config_loader::CloudConfigBundleLoader;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::load_config_layers_state;

use super::Session;

pub(super) async fn list_skills(
    sess: &Session,
    sub_id: String,
    cwds: Vec<PathBuf>,
    force_reload: bool,
) {
    let cwds = if cwds.is_empty() {
        let state = sess.state.lock().await;
        vec![state.session_configuration.cwd.to_path_buf()]
    } else {
        cwds
    };

    let skills_manager = &sess.services.skills_manager;
    let plugins_manager = &sess.services.plugins_manager;
    let config = sess.get_config().await;
    let praxis_home = sess.praxis_home().await;
    let mut skills = Vec::new();
    let empty_cli_overrides: &[(String, toml::Value)] = &[];
    for cwd in cwds {
        let cwd_abs = match AbsolutePathBuf::try_from(cwd.as_path()) {
            Ok(path) => path,
            Err(err) => {
                let message = err.to_string();
                let cwd_for_entry = cwd.clone();
                skills.push(SkillsListEntry {
                    cwd: cwd_for_entry.clone(),
                    skills: Vec::new(),
                    errors: super::errors_to_info(&[SkillError {
                        path: cwd_for_entry,
                        message,
                    }]),
                });
                continue;
            }
        };
        let config_layer_stack = match load_config_layers_state(
            &praxis_home,
            Some(cwd_abs),
            empty_cli_overrides,
            LoaderOverrides::default(),
            CloudConfigBundleLoader::default(),
        )
        .await
        {
            Ok(config_layer_stack) => config_layer_stack,
            Err(err) => {
                let message = err.to_string();
                let cwd_for_entry = cwd.clone();
                skills.push(SkillsListEntry {
                    cwd: cwd_for_entry.clone(),
                    skills: Vec::new(),
                    errors: super::errors_to_info(&[SkillError {
                        path: cwd_for_entry,
                        message,
                    }]),
                });
                continue;
            }
        };
        let effective_skill_roots = plugins_manager.effective_skill_roots_for_layer_stack(
            &config_layer_stack,
            config.features.enabled(Feature::Plugins),
        );
        let skills_input = crate::SkillsLoadInput::new(
            cwd.clone(),
            effective_skill_roots,
            config_layer_stack,
            config.bundled_skills_enabled(),
        );
        let outcome = skills_manager
            .skills_for_cwd(&skills_input, force_reload)
            .await;
        let errors = super::errors_to_info(&outcome.errors);
        let skills_metadata = super::skills_to_info(&outcome.skills, &outcome.disabled_paths);
        skills.push(SkillsListEntry {
            cwd,
            skills: skills_metadata,
            errors,
        });
    }

    let event = Event {
        id: sub_id,
        msg: EventMsg::ListSkillsResponse(ListSkillsResponseEvent { skills }),
    };
    sess.send_event_raw(event).await;
}
