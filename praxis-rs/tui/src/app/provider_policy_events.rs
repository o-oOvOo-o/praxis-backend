use super::*;

impl App {
    pub(super) fn is_provider_policy_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::OpenProviderLoginPrompt { .. }
                | AppEvent::ShowAnthropicLoginStatement
                | AppEvent::ApplyProviderSetup { .. }
                | AppEvent::ApplyModelSelection { .. }
                | AppEvent::PluginUninstallLoaded { .. }
                | AppEvent::PersistPersonalitySelection { .. }
                | AppEvent::PersistServiceTierSelection { .. }
                | AppEvent::PersistRealtimeAudioDeviceSelection { .. }
                | AppEvent::RestartRealtimeAudioDevice { .. }
                | AppEvent::UpdateAskForApprovalPolicy(_)
                | AppEvent::UpdateSandboxPolicy(_)
                | AppEvent::UpdateApprovalsReviewer(_)
                | AppEvent::UpdateFeatureFlags { .. }
                | AppEvent::SkipNextWorldWritableScan
                | AppEvent::UpdateFullAccessWarningAcknowledged(_)
                | AppEvent::UpdateWorldWritableWarningAcknowledged(_)
                | AppEvent::UpdateRateLimitSwitchPromptHidden(_)
                | AppEvent::UpdatePlanModeReasoningEffort(_)
                | AppEvent::PersistFullAccessWarningAcknowledged
                | AppEvent::PersistWorldWritableWarningAcknowledged
                | AppEvent::PersistRateLimitSwitchPromptHidden
                | AppEvent::PersistPlanModeReasoningEffort(_)
                | AppEvent::PersistModelMigrationPromptAcknowledged { .. }
        )
    }
    pub(super) async fn handle_provider_policy_event(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        event: AppEvent,
    ) -> Result<AppRunControl> {
        match event {
            AppEvent::OpenProviderLoginPrompt { provider } => {
                self.chat_widget.open_provider_login_prompt(provider);
            }
            AppEvent::ShowAnthropicLoginStatement => {
                self.chat_widget.show_anthropic_login_statement();
            }
            AppEvent::ApplyProviderSetup {
                model,
                provider_id,
                provider,
                effort,
            } => {
                let provider_label = provider.name.clone();
                self.apply_model_provider_selection(
                    model,
                    provider_id,
                    Some(provider),
                    effort,
                    ProviderConfigWriteMode::ForceUpsert,
                    Some(provider_label),
                )
                .await;
            }
            AppEvent::ApplyModelSelection {
                model,
                provider_id,
                provider,
                effort,
            } => {
                self.apply_model_provider_selection(
                    model,
                    provider_id,
                    provider,
                    effort,
                    ProviderConfigWriteMode::UpsertIfMissing,
                    None,
                )
                .await;
            }
            AppEvent::PluginUninstallLoaded {
                cwd,
                plugin_id: _plugin_id,
                plugin_display_name,
                result,
            } => {
                let uninstall_succeeded = result.is_ok();
                if uninstall_succeeded {
                    if let Err(err) = self.refresh_in_memory_config_from_disk().await {
                        tracing::warn!(
                            error = %err,
                            "failed to refresh config after plugin uninstall"
                        );
                    }
                    self.chat_widget.refresh_plugin_mentions();
                    self.chat_widget.submit_op(AppCommand::reload_user_config());
                }
                self.chat_widget.on_plugin_uninstall_loaded(
                    cwd.clone(),
                    plugin_display_name,
                    result,
                );
                if uninstall_succeeded
                    && self.chat_widget.config_ref().cwd.as_path() == cwd.as_path()
                {
                    self.fetch_plugins_list(app_gateway, cwd);
                }
            }
            AppEvent::PersistPersonalitySelection { personality } => {
                let profile = self.active_profile.as_deref();
                match ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_profile(profile)
                    .set_personality(Some(personality))
                    .apply()
                    .await
                {
                    Ok(()) => {
                        let label = Self::personality_label(personality);
                        let mut message = format!("Personality set to {label}");
                        if let Some(profile) = profile {
                            message.push_str(" for ");
                            message.push_str(profile);
                            message.push_str(" profile");
                        }
                        self.chat_widget.add_info_message(message, /*hint*/ None);
                    }
                    Err(err) => {
                        tracing::error!(
                            error = %err,
                            "failed to persist personality selection"
                        );
                        if let Some(profile) = profile {
                            self.chat_widget.add_error_message(format!(
                                "Failed to save personality for profile `{profile}`: {err}"
                            ));
                        } else {
                            self.chat_widget.add_error_message(format!(
                                "Failed to save default personality: {err}"
                            ));
                        }
                    }
                }
            }
            AppEvent::PersistServiceTierSelection { service_tier } => {
                self.refresh_status_line();
                let profile = self.active_profile.as_deref();
                match ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_profile(profile)
                    .set_service_tier(service_tier)
                    .apply()
                    .await
                {
                    Ok(()) => {
                        let status = if service_tier.is_some() { "on" } else { "off" };
                        let mut message = format!("Fast mode set to {status}");
                        if let Some(profile) = profile {
                            message.push_str(" for ");
                            message.push_str(profile);
                            message.push_str(" profile");
                        }
                        self.chat_widget.add_info_message(message, /*hint*/ None);
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "failed to persist fast mode selection");
                        if let Some(profile) = profile {
                            self.chat_widget.add_error_message(format!(
                                "Failed to save Fast mode for profile `{profile}`: {err}"
                            ));
                        } else {
                            self.chat_widget.add_error_message(format!(
                                "Failed to save default Fast mode: {err}"
                            ));
                        }
                    }
                }
            }
            AppEvent::PersistRealtimeAudioDeviceSelection { kind, name } => {
                let builder = match kind {
                    RealtimeAudioDeviceKind::Microphone => {
                        ConfigEditsBuilder::new(&self.config.praxis_home)
                            .set_realtime_microphone(name.as_deref())
                    }
                    RealtimeAudioDeviceKind::Speaker => {
                        ConfigEditsBuilder::new(&self.config.praxis_home)
                            .set_realtime_speaker(name.as_deref())
                    }
                };

                match builder.apply().await {
                    Ok(()) => {
                        match kind {
                            RealtimeAudioDeviceKind::Microphone => {
                                self.config.realtime_audio.microphone = name.clone();
                            }
                            RealtimeAudioDeviceKind::Speaker => {
                                self.config.realtime_audio.speaker = name.clone();
                            }
                        }
                        self.chat_widget
                            .set_realtime_audio_device(kind, name.clone());

                        if self.chat_widget.realtime_conversation_is_live() {
                            self.chat_widget.open_realtime_audio_restart_prompt(kind);
                        } else {
                            let selection = name.unwrap_or_else(|| "System default".to_string());
                            self.chat_widget.add_info_message(
                                format!("Realtime {} set to {selection}", kind.noun()),
                                /*hint*/ None,
                            );
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            error = %err,
                            "failed to persist realtime audio selection"
                        );
                        self.chat_widget.add_error_message(format!(
                            "Failed to save realtime {}: {err}",
                            kind.noun()
                        ));
                    }
                }
            }
            AppEvent::RestartRealtimeAudioDevice { kind } => {
                self.chat_widget.restart_realtime_audio_device(kind);
            }
            AppEvent::UpdateAskForApprovalPolicy(policy) => {
                let mut config = self.config.clone();
                if !self.try_set_approval_policy_on_config(
                    &mut config,
                    policy,
                    "Failed to set approval policy",
                    "failed to set approval policy on app config",
                ) {
                    return Ok(AppRunControl::Continue);
                }
                self.config = config;
                self.runtime_approval_policy_override =
                    Some(self.config.permissions.approval_policy.value());
                self.chat_widget
                    .set_approval_policy(self.config.permissions.approval_policy.value());
                self.sync_cached_thread_permissions_from_config().await;
                if matches!(
                    self.config.permissions.approval_policy.value(),
                    AskForApproval::Never
                ) {
                    self.chat_widget.auto_approve_runtime_approval_requests();
                }
            }
            AppEvent::UpdateSandboxPolicy(policy) => {
                #[cfg(target_os = "windows")]
                let policy_is_workspace_write_or_ro = matches!(
                    &policy,
                    praxis_protocol::protocol::SandboxPolicy::WorkspaceWrite { .. }
                        | praxis_protocol::protocol::SandboxPolicy::ReadOnly { .. }
                );
                let policy_for_chat = policy.clone();

                let mut config = self.config.clone();
                if !self.try_set_sandbox_policy_on_config(
                    &mut config,
                    policy,
                    "Failed to set sandbox policy",
                    "failed to set sandbox policy on app config",
                ) {
                    return Ok(AppRunControl::Continue);
                }
                self.config = config;
                if let Err(err) = self.chat_widget.set_sandbox_policy(policy_for_chat) {
                    tracing::warn!(%err, "failed to set sandbox policy on chat config");
                    self.chat_widget
                        .add_error_message(format!("Failed to set sandbox policy: {err}"));
                    return Ok(AppRunControl::Continue);
                }
                self.runtime_sandbox_policy_override =
                    Some(self.config.permissions.sandbox_policy.get().clone());
                self.sync_cached_thread_permissions_from_config().await;
                if matches!(
                    self.config.permissions.approval_policy.value(),
                    AskForApproval::Never
                ) {
                    self.chat_widget.auto_approve_runtime_approval_requests();
                }

                // If sandbox policy becomes workspace-write or read-only, run the Windows world-writable scan.
                #[cfg(target_os = "windows")]
                {
                    // One-shot suppression if the user just confirmed continue.
                    if self.windows_sandbox.skip_world_writable_scan_once {
                        self.windows_sandbox.skip_world_writable_scan_once = false;
                        return Ok(AppRunControl::Continue);
                    }

                    let should_check = WindowsSandboxLevel::from_config(&self.config)
                        != WindowsSandboxLevel::Disabled
                        && policy_is_workspace_write_or_ro
                        && !self.chat_widget.world_writable_warning_hidden();
                    if should_check {
                        let cwd = self.config.cwd.clone();
                        let env_map: std::collections::HashMap<String, String> =
                            std::env::vars().collect();
                        let tx = self.app_event_tx.clone();
                        let logs_base_dir = self.config.praxis_home.clone();
                        let sandbox_policy = self.config.permissions.sandbox_policy.get().clone();
                        Self::spawn_world_writable_scan(
                            cwd.to_path_buf(),
                            env_map,
                            logs_base_dir,
                            sandbox_policy,
                            tx,
                        );
                    }
                }
            }
            AppEvent::UpdateApprovalsReviewer(policy) => {
                self.config.approvals_reviewer = policy;
                self.chat_widget.set_approvals_reviewer(policy);
                self.sync_cached_thread_permissions_from_config().await;
                let profile = self.active_profile.as_deref();
                let segments = if let Some(profile) = profile {
                    vec![
                        "profiles".to_string(),
                        profile.to_string(),
                        "approvals_reviewer".to_string(),
                    ]
                } else {
                    vec!["approvals_reviewer".to_string()]
                };
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_profile(profile)
                    .with_edits([ConfigEdit::SetPath {
                        segments,
                        value: policy.to_string().into(),
                    }])
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist approvals reviewer update"
                    );
                    self.chat_widget
                        .add_error_message(format!("Failed to save approvals reviewer: {err}"));
                }
            }
            AppEvent::UpdateFeatureFlags { updates } => {
                self.update_feature_flags(updates).await;
            }
            AppEvent::SkipNextWorldWritableScan => {
                self.windows_sandbox.skip_world_writable_scan_once = true;
            }
            AppEvent::UpdateFullAccessWarningAcknowledged(ack) => {
                self.chat_widget.set_full_access_warning_acknowledged(ack);
            }
            AppEvent::UpdateWorldWritableWarningAcknowledged(ack) => {
                self.chat_widget
                    .set_world_writable_warning_acknowledged(ack);
            }
            AppEvent::UpdateRateLimitSwitchPromptHidden(hidden) => {
                self.chat_widget.set_rate_limit_switch_prompt_hidden(hidden);
            }
            AppEvent::UpdatePlanModeReasoningEffort(effort) => {
                self.config.plan_mode_reasoning_effort = effort;
                self.chat_widget.set_plan_mode_reasoning_effort(effort);
            }
            AppEvent::PersistFullAccessWarningAcknowledged => {
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .set_hide_full_access_warning(/*acknowledged*/ true)
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist full access warning acknowledgement"
                    );
                    self.chat_widget.add_error_message(format!(
                        "Failed to save full access confirmation preference: {err}"
                    ));
                }
            }
            AppEvent::PersistWorldWritableWarningAcknowledged => {
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .set_hide_world_writable_warning(/*acknowledged*/ true)
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist world-writable warning acknowledgement"
                    );
                    self.chat_widget.add_error_message(format!(
                        "Failed to save Agent mode warning preference: {err}"
                    ));
                }
            }
            AppEvent::PersistRateLimitSwitchPromptHidden => {
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .set_hide_rate_limit_model_nudge(/*acknowledged*/ true)
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist rate limit switch prompt preference"
                    );
                    self.chat_widget.add_error_message(format!(
                        "Failed to save rate limit reminder preference: {err}"
                    ));
                }
            }
            AppEvent::PersistPlanModeReasoningEffort(effort) => {
                let profile = self.active_profile.as_deref();
                let segments = if let Some(profile) = profile {
                    vec![
                        "profiles".to_string(),
                        profile.to_string(),
                        "plan_mode_reasoning_effort".to_string(),
                    ]
                } else {
                    vec!["plan_mode_reasoning_effort".to_string()]
                };
                let edit = if let Some(effort) = effort {
                    ConfigEdit::SetPath {
                        segments,
                        value: effort.to_string().into(),
                    }
                } else {
                    ConfigEdit::ClearPath { segments }
                };
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .with_edits([edit])
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist plan mode reasoning effort"
                    );
                    if let Some(profile) = profile {
                        self.chat_widget.add_error_message(format!(
                            "Failed to save Plan mode reasoning effort for profile `{profile}`: {err}"
                        ));
                    } else {
                        self.chat_widget.add_error_message(format!(
                            "Failed to save Plan mode reasoning effort: {err}"
                        ));
                    }
                }
            }
            AppEvent::PersistModelMigrationPromptAcknowledged {
                from_model,
                to_model,
            } => {
                if let Err(err) = ConfigEditsBuilder::new(&self.config.praxis_home)
                    .record_model_migration_seen(from_model.as_str(), to_model.as_str())
                    .apply()
                    .await
                {
                    tracing::error!(
                        error = %err,
                        "failed to persist model migration prompt acknowledgement"
                    );
                    self.chat_widget.add_error_message(format!(
                        "Failed to save model migration prompt preference: {err}"
                    ));
                }
            }
            _ => unreachable!("provider policy event pre-dispatch mismatch"),
        }
        Ok(AppRunControl::Continue)
    }
}
