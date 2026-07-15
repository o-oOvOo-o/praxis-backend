use super::*;

pub(super) struct PermissionsMenuModel {
    pub(super) items: Vec<SelectionItem>,
    show_elevate_sandbox_hint: bool,
}

impl ChatWidget {
    /// Open the permissions popup (alias for /permissions).
    pub(crate) fn open_approvals_popup(&mut self) {
        self.open_permissions_popup();
    }

    /// Open a popup to choose the permissions mode (approval policy + sandbox policy).
    pub(crate) fn open_permissions_popup(&mut self) {
        let model = self.permissions_menu_model();
        let footer_note = model.show_elevate_sandbox_hint.then(|| {
            vec![
                "The non-admin sandbox protects your files and prevents network access under most circumstances. However, it carries greater risk if prompt injected. To upgrade to the default sandbox, run ".dim(),
                "/setup-default-sandbox".cyan(),
                ".".dim(),
            ]
            .into()
        });

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Update Model Permissions".to_string()),
            footer_note,
            footer_hint: Some(standard_popup_hint_line()),
            items: model.items,
            header: Box::new(()),
            ..Default::default()
        });
    }

    pub(super) fn permissions_menu_model(&self) -> PermissionsMenuModel {
        let include_read_only = cfg!(target_os = "windows");
        let current_approval = self.config.permissions.approval_policy.value();
        let current_sandbox = self.config.permissions.sandbox_policy.get();
        let guardian_approval_enabled = self.config.features.enabled(Feature::GuardianApproval);
        let current_review_policy = self.config.approvals_reviewer;
        let mut items: Vec<SelectionItem> = Vec::new();
        let presets: Vec<ApprovalPreset> = builtin_approval_presets();

        #[cfg(target_os = "windows")]
        let windows_sandbox_level = WindowsSandboxLevel::from_config(&self.config);
        #[cfg(target_os = "windows")]
        let windows_degraded_sandbox_enabled =
            matches!(windows_sandbox_level, WindowsSandboxLevel::RestrictedToken);
        #[cfg(not(target_os = "windows"))]
        let windows_degraded_sandbox_enabled = false;

        let show_elevate_sandbox_hint = praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED
            && windows_degraded_sandbox_enabled
            && presets.iter().any(|preset| preset.id == "auto");

        let guardian_disabled_reason = |enabled: bool| {
            let mut next_features = self.config.features.get().clone();
            next_features.set_enabled(Feature::GuardianApproval, enabled);
            self.config
                .features
                .can_set(&next_features)
                .err()
                .map(|err| err.to_string())
        };

        for preset in presets.into_iter() {
            if !include_read_only && preset.id == "read-only" {
                continue;
            }
            let base_name = if preset.id == "auto" && windows_degraded_sandbox_enabled {
                "Default (non-admin sandbox)".to_string()
            } else {
                preset.label.to_string()
            };
            let base_description =
                Some(preset.description.replace(" (Identical to Agent mode)", ""));
            let approval_disabled_reason = match self
                .config
                .permissions
                .approval_policy
                .can_set(&preset.approval)
            {
                Ok(()) => None,
                Err(err) => Some(err.to_string()),
            };
            let default_disabled_reason = approval_disabled_reason
                .clone()
                .or_else(|| guardian_disabled_reason(false));
            let requires_confirmation = preset.id == "full-access"
                && !self
                    .config
                    .notices
                    .hide_full_access_warning
                    .unwrap_or(false);
            let default_actions: Vec<SelectionAction> = if requires_confirmation {
                let preset_clone = preset.clone();
                vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenFullAccessConfirmation {
                        preset: preset_clone.clone(),
                        return_to_permissions: !include_read_only,
                    });
                })]
            } else if preset.id == "auto" {
                #[cfg(target_os = "windows")]
                {
                    if WindowsSandboxLevel::from_config(&self.config)
                        == WindowsSandboxLevel::Disabled
                    {
                        let preset_clone = preset.clone();
                        if praxis_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED
                            && praxis_core::windows_sandbox::sandbox_setup_is_complete(
                                self.config.praxis_home.as_path(),
                            )
                        {
                            vec![Box::new(move |tx| {
                                tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                                    preset: preset_clone.clone(),
                                    mode: WindowsSandboxEnableMode::Elevated,
                                });
                            })]
                        } else {
                            vec![Box::new(move |tx| {
                                tx.send(AppEvent::OpenWindowsSandboxEnablePrompt {
                                    preset: preset_clone.clone(),
                                });
                            })]
                        }
                    } else if let Some((sample_paths, extra_count, failed_scan)) =
                        self.world_writable_warning_details()
                    {
                        let preset_clone = preset.clone();
                        vec![Box::new(move |tx| {
                            tx.send(AppEvent::OpenWorldWritableWarningConfirmation {
                                preset: Some(preset_clone.clone()),
                                sample_paths: sample_paths.clone(),
                                extra_count,
                                failed_scan,
                            });
                        })]
                    } else {
                        Self::approval_preset_actions(
                            preset.approval,
                            preset.sandbox.clone(),
                            base_name.clone(),
                            ApprovalsReviewer::User,
                        )
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Self::approval_preset_actions(
                        preset.approval,
                        preset.sandbox.clone(),
                        base_name.clone(),
                        ApprovalsReviewer::User,
                    )
                }
            } else {
                Self::approval_preset_actions(
                    preset.approval,
                    preset.sandbox.clone(),
                    base_name.clone(),
                    ApprovalsReviewer::User,
                )
            };
            if preset.id == "auto" {
                items.push(SelectionItem {
                    name: base_name.clone(),
                    description: base_description.clone(),
                    is_current: current_review_policy == ApprovalsReviewer::User
                        && Self::preset_matches_current(current_approval, current_sandbox, &preset),
                    actions: default_actions,
                    dismiss_on_select: true,
                    disabled_reason: default_disabled_reason,
                    ..Default::default()
                });

                if guardian_approval_enabled {
                    items.push(SelectionItem {
                        name: "Guardian Approvals".to_string(),
                        description: Some(
                            "Same workspace-write permissions as Default, but eligible `on-request` approvals are routed through the guardian reviewer subagent."
                                .to_string(),
                        ),
                        is_current: current_review_policy == ApprovalsReviewer::GuardianSubagent
                            && Self::preset_matches_current(
                                current_approval,
                                current_sandbox,
                                &preset,
                            ),
                        actions: Self::approval_preset_actions(
                            preset.approval,
                            preset.sandbox.clone(),
                            "Guardian Approvals".to_string(),
                            ApprovalsReviewer::GuardianSubagent,
                        ),
                        dismiss_on_select: true,
                        disabled_reason: approval_disabled_reason
                            .or_else(|| guardian_disabled_reason(true)),
                        ..Default::default()
                    });
                }
            } else {
                items.push(SelectionItem {
                    name: base_name,
                    description: base_description,
                    is_current: Self::preset_matches_current(
                        current_approval,
                        current_sandbox,
                        &preset,
                    ),
                    actions: default_actions,
                    dismiss_on_select: true,
                    disabled_reason: default_disabled_reason,
                    ..Default::default()
                });
            }
        }

        PermissionsMenuModel {
            items,
            show_elevate_sandbox_hint,
        }
    }

    pub(super) fn approval_preset_actions(
        approval: AskForApproval,
        sandbox: SandboxPolicy,
        label: String,
        approvals_reviewer: ApprovalsReviewer,
    ) -> Vec<SelectionAction> {
        vec![Box::new(move |tx| {
            let sandbox_clone = sandbox.clone();
            tx.send(AppEvent::AgentOp(
                AppCommand::override_turn_context(
                    /*cwd*/ None,
                    Some(approval),
                    Some(approvals_reviewer),
                    Some(sandbox_clone.clone()),
                    /*windows_sandbox_level*/ None,
                    /*model_provider*/ None,
                    /*model*/ None,
                    /*effort*/ None,
                    /*summary*/ None,
                    /*service_tier*/ None,
                    /*collaboration_mode*/ None,
                    /*personality*/ None,
                )
                .into_core(),
            ));
            tx.send(AppEvent::UpdateAskForApprovalPolicy(approval));
            tx.send(AppEvent::UpdateSandboxPolicy(sandbox_clone));
            tx.send(AppEvent::UpdateApprovalsReviewer(approvals_reviewer));
            tx.send(AppEvent::InsertHistoryCell(Box::new(
                history_cell::new_info_event(
                    format!("Permissions updated to {label}"),
                    /*hint*/ None,
                ),
            )));
        })]
    }

    pub(super) fn preset_matches_current(
        current_approval: AskForApproval,
        current_sandbox: &SandboxPolicy,
        preset: &ApprovalPreset,
    ) -> bool {
        praxis_utils_approval_presets::approval_preset_matches(
            current_approval,
            current_sandbox,
            preset,
        )
    }
}
