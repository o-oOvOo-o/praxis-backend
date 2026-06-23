use super::*;

impl ChatWidget {
    pub(super) fn queue_user_message(&mut self, user_message: UserMessage) {
        if !self.is_session_configured() || self.bottom_pane.is_task_running() {
            self.queued_user_messages.push_back(user_message);
            self.refresh_pending_input_preview();
        } else {
            self.submit_user_message(user_message);
        }
    }

    pub(super) fn submit_user_message(&mut self, user_message: UserMessage) {
        if self.dispatch_slash_command_from_user_message(&user_message) {
            return;
        }
        if let Some(label) = self.read_only_thread_control_label() {
            self.restore_user_message_to_composer(user_message);
            self.add_info_message(
                format!("This thread is locked by {label}."),
                Some("Type /release-thread to take over before sending a message.".to_string()),
            );
            return;
        }
        if !self.is_session_configured() {
            tracing::warn!("cannot submit user message before session is configured; queueing");
            self.queued_user_messages.push_front(user_message);
            self.refresh_pending_input_preview();
            self.app_event_tx.send(AppEvent::NewSession);
            return;
        }
        let UserMessage {
            text,
            local_images,
            remote_image_urls,
            text_elements,
            mention_bindings,
        } = user_message;
        if text.is_empty() && local_images.is_empty() && remote_image_urls.is_empty() {
            return;
        }
        if (!local_images.is_empty() || !remote_image_urls.is_empty())
            && !self.current_model_supports_images()
        {
            self.restore_blocked_image_submission(
                text,
                text_elements,
                local_images,
                mention_bindings,
                remote_image_urls,
            );
            return;
        }

        let render_in_history = !self.agent_turn_running;
        let mut items: Vec<UserInput> = Vec::new();

        // Special-case: "!cmd" executes a local shell command instead of sending to the model.
        if let Some(stripped) = text.strip_prefix('!') {
            let cmd = stripped.trim();
            if cmd.is_empty() {
                self.app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
                    history_cell::new_info_event(
                        USER_SHELL_COMMAND_HELP_TITLE.to_string(),
                        Some(USER_SHELL_COMMAND_HELP_HINT.to_string()),
                    ),
                )));
                return;
            }
            self.submit_op(AppCommand::run_user_shell_command(cmd.to_string()));
            return;
        }

        for image_url in &remote_image_urls {
            items.push(UserInput::Image {
                image_url: image_url.clone(),
            });
        }

        for image in &local_images {
            items.push(UserInput::LocalImage {
                path: image.path.clone(),
            });
        }

        if !text.is_empty() {
            items.push(UserInput::Text {
                text: text.clone(),
                text_elements: text_elements.clone(),
            });
        }

        let mentions = collect_tool_mentions(&text, &HashMap::new());
        let bound_names: HashSet<String> = mention_bindings
            .iter()
            .map(|binding| binding.mention.clone())
            .collect();
        let mut skill_names_lower: HashSet<String> = HashSet::new();
        let mut selected_skill_paths: HashSet<PathBuf> = HashSet::new();
        let mut selected_plugin_ids: HashSet<String> = HashSet::new();

        if let Some(skills) = self.bottom_pane.skills() {
            skill_names_lower = skills
                .iter()
                .map(|skill| skill.name.to_ascii_lowercase())
                .collect();

            for binding in &mention_bindings {
                let path = binding
                    .path
                    .strip_prefix("skill://")
                    .unwrap_or(binding.path.as_str());
                let path = Path::new(path);
                if let Some(skill) = skills
                    .iter()
                    .find(|skill| skill.path_to_skills_md.as_path() == path)
                    && selected_skill_paths.insert(skill.path_to_skills_md.clone())
                {
                    items.push(UserInput::Skill {
                        name: skill.name.clone(),
                        path: skill.path_to_skills_md.clone(),
                    });
                }
            }

            let skill_mentions = find_skill_mentions_with_tool_mentions(&mentions, skills);
            for skill in skill_mentions {
                if bound_names.contains(skill.name.as_str())
                    || !selected_skill_paths.insert(skill.path_to_skills_md.clone())
                {
                    continue;
                }
                items.push(UserInput::Skill {
                    name: skill.name.clone(),
                    path: skill.path_to_skills_md.clone(),
                });
            }
        }

        if let Some(plugins) = self.plugins_for_mentions() {
            for binding in &mention_bindings {
                let Some(plugin_config_name) = binding
                    .path
                    .strip_prefix("plugin://")
                    .filter(|id| !id.is_empty())
                else {
                    continue;
                };
                if !selected_plugin_ids.insert(plugin_config_name.to_string()) {
                    continue;
                }
                if let Some(plugin) = plugins
                    .iter()
                    .find(|plugin| plugin.config_name == plugin_config_name)
                {
                    items.push(UserInput::Mention {
                        name: plugin.display_name.clone(),
                        path: binding.path.clone(),
                    });
                }
            }
        }

        let mut selected_app_ids: HashSet<String> = HashSet::new();
        if let Some(apps) = self.connectors_for_mentions() {
            for binding in &mention_bindings {
                let Some(app_id) = binding
                    .path
                    .strip_prefix("app://")
                    .filter(|id| !id.is_empty())
                else {
                    continue;
                };
                if !selected_app_ids.insert(app_id.to_string()) {
                    continue;
                }
                if let Some(app) = apps.iter().find(|app| app.id == app_id && app.is_enabled) {
                    items.push(UserInput::Mention {
                        name: app.name.clone(),
                        path: binding.path.clone(),
                    });
                }
            }

            let app_mentions = find_app_mentions(&mentions, apps, &skill_names_lower);
            for app in app_mentions {
                let slug = praxis_core::connectors::connector_mention_slug(&app);
                if bound_names.contains(&slug) || !selected_app_ids.insert(app.id.clone()) {
                    continue;
                }
                let app_id = app.id.as_str();
                items.push(UserInput::Mention {
                    name: app.name.clone(),
                    path: format!("app://{app_id}"),
                });
            }
        }

        let effective_mode = self.effective_collaboration_mode();
        if effective_mode.model().trim().is_empty() {
            self.add_error_message(
                "Thread model is unavailable. Wait for the thread to finish syncing or choose a model before sending input.".to_string(),
            );
            return;
        }
        let collaboration_mode = if self.collaboration_modes_enabled() {
            self.active_collaboration_mask
                .as_ref()
                .map(|_| effective_mode.clone())
        } else {
            None
        };
        let pending_steer = (!render_in_history).then(|| PendingSteer {
            user_message: UserMessage {
                text: text.clone(),
                local_images: local_images.clone(),
                remote_image_urls: remote_image_urls.clone(),
                text_elements: text_elements.clone(),
                mention_bindings: mention_bindings.clone(),
            },
            compare_key: Self::pending_steer_compare_key_from_items(&items),
        });
        let personality = self
            .config
            .personality
            .filter(|_| self.config.features.enabled(Feature::Personality))
            .filter(|_| self.current_model_supports_personality());
        let service_tier = self.config.service_tier.map(Some);
        let op = AppCommand::user_turn(
            items,
            self.config.cwd.to_path_buf(),
            self.config.permissions.approval_policy.value(),
            self.config.permissions.sandbox_policy.get().clone(),
            self.current_model_provider_id().to_string(),
            effective_mode.model().to_string(),
            effective_mode.reasoning_effort(),
            /*summary*/ None,
            service_tier,
            /*final_output_json_schema*/ None,
            collaboration_mode,
            personality,
        );

        if !self.submit_op(op) {
            return;
        }

        if render_in_history {
            self.on_task_started();
        }

        // Persist the text to cross-session message history. Mentions are
        // encoded into placeholder syntax so recall can reconstruct the
        // mention bindings in a future session.
        if !text.is_empty() {
            let encoded_mentions = mention_bindings
                .iter()
                .map(|binding| LinkedMention {
                    mention: binding.mention.clone(),
                    path: binding.path.clone(),
                })
                .collect::<Vec<_>>();
            let history_text = encode_history_mentions(&text, &encoded_mentions);
            self.submit_op(Op::AddToHistory { text: history_text });
        }

        if let Some(pending_steer) = pending_steer {
            self.pending_steers.push_back(pending_steer);
            self.saw_plan_item_this_turn = false;
            self.refresh_pending_input_preview();
        }

        // Show replayable user content in conversation history.
        if render_in_history && !text.is_empty() {
            let local_image_paths = local_images
                .into_iter()
                .map(|img| img.path)
                .collect::<Vec<_>>();
            self.last_rendered_user_message_event =
                Some(Self::rendered_user_message_event_from_parts(
                    text.clone(),
                    text_elements.clone(),
                    local_image_paths.clone(),
                    remote_image_urls.clone(),
                ));
            self.add_to_history(history_cell::new_user_prompt(
                text,
                text_elements,
                local_image_paths,
                remote_image_urls,
            ));
        } else if render_in_history && !remote_image_urls.is_empty() {
            self.last_rendered_user_message_event =
                Some(Self::rendered_user_message_event_from_parts(
                    String::new(),
                    Vec::new(),
                    Vec::new(),
                    remote_image_urls.clone(),
                ));
            self.add_to_history(history_cell::new_user_prompt(
                String::new(),
                Vec::new(),
                Vec::new(),
                remote_image_urls,
            ));
        }

        self.needs_final_message_separator = false;
    }

    /// Restore the blocked submission draft without losing mention resolution state.
    ///
    /// The blocked-image path intentionally keeps the draft in the composer so
    /// users can remove attachments and retry. We must restore
    /// mention bindings alongside visible text; restoring only `$name` tokens
    /// makes the draft look correct while degrading mention resolution to
    /// name-only heuristics on retry.
    fn restore_blocked_image_submission(
        &mut self,
        text: String,
        text_elements: Vec<TextElement>,
        local_images: Vec<LocalImageAttachment>,
        mention_bindings: Vec<MentionBinding>,
        remote_image_urls: Vec<String>,
    ) {
        // Preserve the user's composed payload so they can retry after changing models.
        let local_image_paths = local_images.iter().map(|img| img.path.clone()).collect();
        self.set_remote_image_urls(remote_image_urls);
        self.bottom_pane.set_composer_text_with_mention_bindings(
            text,
            text_elements,
            local_image_paths,
            mention_bindings,
        );
        self.add_to_history(history_cell::new_warning_event(
            self.image_inputs_not_supported_message(),
        ));
        self.request_redraw();
    }
}
