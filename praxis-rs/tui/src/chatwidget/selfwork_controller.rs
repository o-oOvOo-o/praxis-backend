use super::*;

impl ChatWidget {
    fn selfwork_can_start_now(&self) -> bool {
        self.selfwork_plan_path.is_some()
            && !self.selfwork_runtime.turn_in_flight()
            && self.is_session_configured()
            && !self.bottom_pane.is_task_running()
            && !self.has_queued_follow_up_messages()
            && self.pending_steers.is_empty()
            && self.bottom_pane.no_modal_or_popup_active()
            && self.bottom_pane.composer_is_empty()
            && self.initial_user_message.is_none()
    }

    pub(super) fn sync_work_panel_selfwork(&mut self) {
        self.work_panel.set_selfwork(
            self.selfwork_plan_path.clone(),
            self.selfwork_runtime.turn_in_flight(),
            self.selfwork_runtime.stall_count(),
            SELFWORK_STALL_LIMIT,
        );
    }

    fn persist_selfwork_plan_path(&self, plan_path: Option<PathBuf>) {
        if let Some(thread_id) = self.thread_id {
            self.app_event_tx.send(AppEvent::PersistSelfworkPlanPath {
                thread_id,
                plan_path,
            });
        }
    }

    pub(super) fn handle_selfwork_default_invocation(&mut self) {
        if self.selfwork_plan_path.is_some() {
            self.show_selfwork_status();
        } else {
            self.open_selfwork_plan_picker_or_prompt();
        }
    }

    pub(crate) fn start_selfwork_from_input(&mut self, raw_path: String) {
        match resolve_selfwork_plan_path(
            &raw_path,
            self.current_cwd.as_deref(),
            self.config.cwd.as_path(),
        ) {
            Ok(path) => self.activate_selfwork(path),
            Err(err) => self.add_error_message(err),
        }
    }

    pub(crate) fn show_selfwork_plan_prompt(&mut self) {
        if !self.is_session_configured() {
            self.add_error_message(
                "Selfwork is unavailable until the thread session is ready.".to_string(),
            );
            return;
        }

        let root = selfwork_search_root(self.current_cwd.as_deref(), self.config.cwd.as_path());
        let context = format!(
            "Project root: {}",
            display_path_for(root.as_path(), self.config.cwd.as_path())
        );
        let tx = self.app_event_tx.clone();
        let view = CustomPromptView::new(
            "Choose Selfwork Plan".to_string(),
            "Type a markdown plan path and press Enter".to_string(),
            Some(context),
            Box::new(move |raw_path: String| {
                tx.send(AppEvent::StartSelfworkFromInput { raw_path });
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
        self.request_redraw();
    }

    pub(super) fn open_selfwork_plan_picker_or_prompt(&mut self) {
        if !self.is_session_configured() {
            self.add_error_message(
                "Selfwork is unavailable until the thread session is ready.".to_string(),
            );
            return;
        }

        let root = selfwork_search_root(self.current_cwd.as_deref(), self.config.cwd.as_path());
        let discovery = match discover_selfwork_plan_candidates(root) {
            Ok(discovery) => discovery,
            Err(err) => {
                self.add_error_message(err);
                return;
            }
        };

        if discovery.candidates.is_empty() {
            self.show_selfwork_plan_prompt();
            return;
        }

        let root_display = display_path_for(discovery.root.as_path(), self.config.cwd.as_path());
        let mut items: Vec<SelectionItem> = discovery
            .candidates
            .into_iter()
            .map(|candidate| {
                let plan_path = candidate.path.clone();
                SelectionItem {
                    name: candidate.display_path,
                    description: Some(candidate.description),
                    selected_description: Some(candidate.selected_description),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::ActivateSelfworkPlan {
                            plan_path: plan_path.clone(),
                        });
                    })],
                    dismiss_on_select: true,
                    search_value: Some(candidate.search_value),
                    ..Default::default()
                }
            })
            .collect();
        items.push(SelectionItem {
            name: "Type a path manually".to_string(),
            description: Some("Use a markdown file outside the scanned project tree.".to_string()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenSelfworkPlanPrompt);
            })],
            dismiss_on_select: true,
            search_value: Some("manual path prompt".to_string()),
            ..Default::default()
        });

        self.show_selection_view(SelectionViewParams {
            view_id: Some(SELFWORK_PICKER_VIEW_ID),
            title: Some("Choose Selfwork Plan".to_string()),
            subtitle: Some(format!("Pick a markdown plan under {root_display}.")),
            footer_note: discovery.truncated.then(|| {
                Line::from(format!(
                    "Showing the first {SELFWORK_PLAN_SCAN_LIMIT} markdown files. Use the manual-path option if the one you want is missing."
                ))
            }),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("Search markdown plans".to_string()),
            ..Default::default()
        });
    }

    pub(super) fn clear_selfwork_state(
        &mut self,
        persist: bool,
        message: Option<String>,
        hint: Option<String>,
    ) {
        self.selfwork_plan_path = None;
        self.selfwork_runtime.reset();
        self.sync_work_panel_selfwork();
        if persist {
            self.persist_selfwork_plan_path(None);
        }
        if let Some(message) = message {
            self.add_info_message(message, hint);
        }
    }

    pub(crate) fn activate_selfwork(&mut self, plan_path: PathBuf) {
        if !self.is_session_configured() {
            self.add_error_message(
                "Selfwork is unavailable until the thread session is ready.".to_string(),
            );
            return;
        }

        let inspection = match inspect_selfwork_plan(&plan_path) {
            Ok(inspection) => inspection,
            Err(err) => {
                self.add_error_message(err);
                return;
            }
        };

        if inspection.complete {
            self.clear_selfwork_state(
                /*persist*/ true,
                Some(format!(
                    "Plan already looks complete: {}",
                    inspection.path.display()
                )),
                None,
            );
            return;
        }

        self.selfwork_plan_path = Some(inspection.path.clone());
        self.selfwork_runtime.arm(&inspection);
        self.sync_work_panel_selfwork();
        self.persist_selfwork_plan_path(Some(inspection.path.clone()));

        let waiting_reason = if self.selfwork_can_start_now() {
            "Starting immediately while the thread is idle.".to_string()
        } else {
            "Armed. It will continue once the thread is idle, the composer is empty, and no popup is open.".to_string()
        };
        let checklist_hint = (inspection.checklist_total > 0).then(|| {
            format!(
                "Checklist progress: {}/{} unfinished item(s).",
                inspection.checklist_unchecked, inspection.checklist_total
            )
        });
        self.add_info_message(
            format!("Selfwork armed for {}.", inspection.path.display()),
            Some(match checklist_hint {
                Some(checklist_hint) => format!("{waiting_reason} {checklist_hint}"),
                None => waiting_reason,
            }),
        );
        self.maybe_start_selfwork_turn_now();
    }

    pub(super) fn show_selfwork_status(&mut self) {
        let Some(path) = self.selfwork_plan_path.clone() else {
            self.add_info_message(
                "Selfwork is off.".to_string(),
                Some(SELFWORK_USAGE.to_string()),
            );
            return;
        };

        let state = if self.selfwork_runtime.turn_in_flight() {
            "running"
        } else {
            "armed"
        };
        let hint = match inspect_selfwork_plan(&path) {
            Err(err) => Some(err),
            Ok(inspection) if inspection.complete => Some(
                "The plan already looks complete; the next idle check will stop selfwork."
                    .to_string(),
            ),
            Ok(inspection) if inspection.checklist_total > 0 => Some(format!(
                "Checklist progress: {}/{} unfinished item(s). Stall guard: {}/{} unchanged selfwork turn(s).",
                inspection.checklist_unchecked,
                inspection.checklist_total,
                self.selfwork_runtime.stall_count(),
                SELFWORK_STALL_LIMIT
            )),
            _ => Some(format!(
                "Stall guard: {}/{} unchanged selfwork turn(s).",
                self.selfwork_runtime.stall_count(), SELFWORK_STALL_LIMIT
            )),
        };
        self.add_info_message(format!("Selfwork is {state} for {}.", path.display()), hint);
    }

    pub(super) fn maybe_start_selfwork_turn_now(&mut self) -> bool {
        if !self.selfwork_can_start_now() {
            return false;
        }

        let Some(plan_path) = self.selfwork_plan_path.clone() else {
            return false;
        };

        let inspection = match inspect_selfwork_plan(&plan_path) {
            Ok(inspection) => inspection,
            Err(err) => {
                self.clear_selfwork_state(
                    /*persist*/ true,
                    Some(err),
                    Some(
                        "Selfwork stopped because the plan file is no longer available."
                            .to_string(),
                    ),
                );
                return false;
            }
        };

        if inspection.complete {
            self.clear_selfwork_state(
                /*persist*/ true,
                Some(format!(
                    "Selfwork stopped because the plan is complete: {}",
                    inspection.path.display()
                )),
                None,
            );
            return false;
        }

        self.selfwork_runtime.begin_turn(&inspection);
        self.sync_work_panel_selfwork();
        self.submit_user_message(selfwork_prompt(&inspection.path).into());
        true
    }

    pub(super) fn maybe_continue_selfwork_after_turn(&mut self, was_selfwork_turn: bool) {
        let Some(plan_path) = self.selfwork_plan_path.clone() else {
            return;
        };

        if was_selfwork_turn {
            let inspection = match inspect_selfwork_plan(&plan_path) {
                Ok(inspection) => inspection,
                Err(err) => {
                    self.clear_selfwork_state(
                        /*persist*/ true,
                        Some(err),
                        Some(
                            "Selfwork stopped because the plan file is no longer available."
                                .to_string(),
                        ),
                    );
                    return;
                }
            };

            if inspection.complete {
                self.clear_selfwork_state(
                    /*persist*/ true,
                    Some(format!(
                        "Selfwork finished: {} now looks complete.",
                        inspection.path.display()
                    )),
                    None,
                );
                return;
            }

            let advance = self.selfwork_runtime.observe_plan_after_turn(&inspection);
            self.sync_work_panel_selfwork();

            if matches!(advance, SelfworkPlanAdvance::Stalled { .. }) {
                self.clear_selfwork_state(
                    /*persist*/ true,
                    Some(format!(
                        "Selfwork stopped after {} unchanged turns for {}.",
                        SELFWORK_STALL_LIMIT,
                        inspection.path.display()
                    )),
                    Some(
                        "Edit the plan or restart /selfwork once there is a new next step."
                            .to_string(),
                    ),
                );
                return;
            }
        }

        self.maybe_start_selfwork_turn_now();
    }
}
