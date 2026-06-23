use super::*;

impl ChatWidget {
    pub(crate) fn set_token_info(&mut self, info: Option<TokenUsageInfo>) {
        match info {
            Some(info) => self.apply_token_info(info),
            None => {
                self.bottom_pane
                    .set_context_window(/*percent*/ None, /*used_tokens*/ None);
                self.token_info = None;
                self.sync_status_budget_message();
                self.sync_work_panel_context();
            }
        }
    }

    pub(super) fn apply_turn_started_context_window(&mut self, model_context_window: Option<i64>) {
        let info = match self.token_info.take() {
            Some(mut info) => {
                if model_context_window.is_some() {
                    info.model_context_window = model_context_window;
                }
                info
            }
            None => {
                let Some(model_context_window) = model_context_window else {
                    return;
                };
                TokenUsageInfo {
                    total_token_usage: TokenUsage::default(),
                    last_token_usage: TokenUsage::default(),
                    model_context_window: Some(model_context_window),
                    model_auto_compact_token_limit: None,
                }
            }
        };

        self.apply_token_info(info);
    }

    pub(super) fn apply_token_info(&mut self, info: TokenUsageInfo) {
        let percent = self.context_remaining_percent(&info);
        let used_tokens = self.context_used_tokens(&info, percent.is_some());
        self.bottom_pane.set_context_window(percent, used_tokens);
        self.token_info = Some(info);
        self.sync_status_budget_message();
        self.sync_work_panel_context();
    }

    pub(super) fn context_remaining_percent(&self, info: &TokenUsageInfo) -> Option<i64> {
        info.model_context_window.map(|window| {
            info.last_token_usage
                .percent_of_context_window_remaining(window)
        })
    }

    pub(super) fn context_used_tokens(
        &self,
        info: &TokenUsageInfo,
        percent_known: bool,
    ) -> Option<i64> {
        if percent_known {
            return None;
        }

        Some(info.last_token_usage.tokens_in_context_window())
    }

    pub(super) fn status_budget_limit(&self, info: &TokenUsageInfo) -> Option<i64> {
        let auto_compact_limit = self
            .token_info
            .as_ref()
            .and_then(|info| info.model_auto_compact_token_limit)
            .or(self.config.model_auto_compact_token_limit)
            .filter(|limit| *limit > 0);
        let context_window = info
            .model_context_window
            .or(self.config.model_context_window)
            .filter(|limit| *limit > 0);

        match (auto_compact_limit, context_window) {
            (Some(auto_compact_limit), Some(context_window)) => {
                Some(auto_compact_limit.min(context_window))
            }
            (Some(auto_compact_limit), None) => Some(auto_compact_limit),
            (None, Some(context_window)) => Some(context_window),
            (None, None) => None,
        }
    }

    pub(super) fn status_context_used_tokens(info: &TokenUsageInfo) -> i64 {
        info.last_token_usage.tokens_in_context_window().max(0)
    }

    pub(super) fn status_budget_message(&self) -> Option<String> {
        let info = self.token_info.as_ref()?;
        let used_tokens = Self::status_context_used_tokens(info);
        let cache_message = self.status_cache_message();
        if used_tokens == 0 {
            return cache_message;
        }
        let Some(limit) = self.status_budget_limit(info) else {
            return Some(Self::append_status_cache_message(
                format!("Context: {} used", format_tokens_compact(used_tokens)),
                cache_message,
            ));
        };

        let used_fmt = format_tokens_compact(used_tokens);
        let limit_fmt = format_tokens_compact(limit);
        let context_message = if used_tokens >= limit {
            format!("Context: {used_fmt} used ({limit_fmt} compact)")
        } else {
            let used_percent = ((used_tokens as f64 / limit as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as i64;
            format!("Context: {used_fmt} / {limit_fmt} ({used_percent}%)")
        };
        Some(Self::append_status_cache_message(
            context_message,
            cache_message,
        ))
    }

    pub(super) fn status_cache_message(&self) -> Option<String> {
        let info = self.token_info.as_ref()?;
        let mut parts = Vec::new();
        if let Some(segment) = Self::cache_hit_segment("last", &info.last_token_usage) {
            parts.push(segment);
        }
        if let Some(segment) = Self::cache_hit_segment("total", &info.total_token_usage) {
            parts.push(segment);
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!("Cache: {}", parts.join(", ")))
        }
    }

    pub(super) fn status_line_cache_hit_message(&self) -> Option<String> {
        let info = self.token_info.as_ref()?;
        let mut parts = Vec::new();
        if let Some(segment) = Self::cache_hit_segment_short("L", &info.last_token_usage) {
            parts.push(segment);
        }
        if let Some(segment) = Self::cache_hit_segment_short("T", &info.total_token_usage) {
            parts.push(segment);
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!("cache {}", parts.join("/")))
        }
    }

    pub(super) fn cache_hit_segment(label: &str, usage: &TokenUsage) -> Option<String> {
        let input_tokens = usage.cache_reported_input();
        let percent = usage.cache_hit_percent()?;
        let cached_input = usage.cached_input().min(input_tokens);
        Some(format!(
            "{label} {percent}% ({}/{})",
            format_tokens_compact(cached_input),
            format_tokens_compact(input_tokens),
        ))
    }

    pub(super) fn cache_hit_segment_short(label: &str, usage: &TokenUsage) -> Option<String> {
        usage
            .cache_hit_percent()
            .map(|percent| format!("{label}{percent}%"))
    }

    pub(super) fn append_status_cache_message(
        context_message: String,
        cache_message: Option<String>,
    ) -> String {
        match cache_message {
            Some(cache_message) => format!("{context_message} · {cache_message}"),
            None => context_message,
        }
    }

    pub(super) fn sync_status_budget_message(&mut self) {
        self.turn_status_snapshot
            .set_budget_message(self.status_budget_message());
        self.refresh_rendered_status_state();
    }

    pub(super) fn restore_pre_review_token_info(&mut self) {
        if let Some(saved) = self.pre_review_token_info.take() {
            match saved {
                Some(info) => self.apply_token_info(info),
                None => {
                    self.bottom_pane
                        .set_context_window(/*percent*/ None, /*used_tokens*/ None);
                    self.token_info = None;
                    self.sync_work_panel_context();
                }
            }
        }
    }

    pub(crate) fn on_rate_limit_snapshot(&mut self, snapshot: Option<RateLimitSnapshot>) {
        if let Some(mut snapshot) = snapshot {
            let limit_id = snapshot
                .limit_id
                .clone()
                .unwrap_or_else(|| OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID.to_string());
            let limit_label = snapshot
                .limit_name
                .clone()
                .unwrap_or_else(|| limit_id.clone());
            if snapshot.credits.is_none() {
                snapshot.credits = self
                    .rate_limit_snapshots_by_limit_id
                    .get(&limit_id)
                    .and_then(|display| display.credits.as_ref())
                    .map(|credits| CreditsSnapshot {
                        has_credits: credits.has_credits,
                        unlimited: credits.unlimited,
                        balance: credits.balance.clone(),
                    });
            }

            self.plan_type = snapshot.plan_type.or(self.plan_type);

            let is_praxis_limit = is_openai_hosted_primary_rate_limit(&limit_id);
            let warnings = if is_praxis_limit {
                self.rate_limit_warnings.take_warnings(
                    snapshot
                        .secondary
                        .as_ref()
                        .map(|window| window.used_percent),
                    snapshot
                        .secondary
                        .as_ref()
                        .and_then(|window| window.window_minutes),
                    snapshot.primary.as_ref().map(|window| window.used_percent),
                    snapshot
                        .primary
                        .as_ref()
                        .and_then(|window| window.window_minutes),
                )
            } else {
                vec![]
            };

            let high_usage = is_praxis_limit
                && (snapshot
                    .secondary
                    .as_ref()
                    .map(|w| w.used_percent >= RATE_LIMIT_SWITCH_PROMPT_THRESHOLD)
                    .unwrap_or(false)
                    || snapshot
                        .primary
                        .as_ref()
                        .map(|w| w.used_percent >= RATE_LIMIT_SWITCH_PROMPT_THRESHOLD)
                        .unwrap_or(false));

            let has_workspace_credits = snapshot
                .credits
                .as_ref()
                .map(|credits| credits.has_credits)
                .unwrap_or(false);

            if high_usage
                && !has_workspace_credits
                && !self.rate_limit_switch_prompt_hidden()
                && self.current_model() != NUDGE_MODEL_SLUG
                && !matches!(
                    self.rate_limit_switch_prompt,
                    RateLimitSwitchPromptState::Shown
                )
            {
                self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Pending;
            }

            let display =
                rate_limit_snapshot_display_for_limit(&snapshot, limit_label, Local::now());
            self.rate_limit_snapshots_by_limit_id
                .insert(limit_id, display);

            if !warnings.is_empty() {
                for warning in warnings {
                    self.add_to_history(history_cell::new_warning_event(warning));
                }
                self.request_redraw();
            }
        } else {
            self.rate_limit_snapshots_by_limit_id.clear();
        }
        self.refresh_status_line();
    }

    pub(super) fn status_line_context_window_size(&self) -> Option<i64> {
        self.token_info
            .as_ref()
            .and_then(|info| info.model_context_window)
            .or(self.config.model_context_window)
    }

    pub(super) fn status_line_context_remaining_percent(&self) -> Option<i64> {
        let Some(context_window) = self.status_line_context_window_size() else {
            return Some(100);
        };
        let default_usage = TokenUsage::default();
        let usage = self
            .token_info
            .as_ref()
            .map(|info| &info.last_token_usage)
            .unwrap_or(&default_usage);
        Some(
            usage
                .percent_of_context_window_remaining(context_window)
                .clamp(0, 100),
        )
    }

    pub(super) fn status_line_context_used_percent(&self) -> Option<i64> {
        let remaining = self.status_line_context_remaining_percent().unwrap_or(100);
        Some((100 - remaining).clamp(0, 100))
    }

    pub(super) fn status_line_total_usage(&self) -> TokenUsage {
        self.token_info
            .as_ref()
            .map(|info| info.total_token_usage.clone())
            .unwrap_or_default()
    }

    pub(super) fn status_line_context_usage(&self) -> TokenUsage {
        self.token_info
            .as_ref()
            .map(|info| info.last_token_usage.clone())
            .unwrap_or_default()
    }

    pub(super) fn status_line_limit_display(
        &self,
        window: Option<&RateLimitWindowDisplay>,
        label: &str,
    ) -> Option<String> {
        let window = window?;
        let remaining = (100.0f64 - window.used_percent).clamp(0.0f64, 100.0f64);
        Some(format!("{label} {remaining:.0}%"))
    }

    pub(super) fn status_line_reasoning_effort_label(
        effort: Option<ReasoningEffortConfig>,
    ) -> &'static str {
        match effort {
            Some(ReasoningEffortConfig::Minimal) => "minimal",
            Some(ReasoningEffortConfig::Low) => "low",
            Some(ReasoningEffortConfig::Medium) => "medium",
            Some(ReasoningEffortConfig::High) => "high",
            Some(ReasoningEffortConfig::XHigh) => "xhigh",
            None | Some(ReasoningEffortConfig::None) => "default",
        }
    }

    pub(super) fn stop_rate_limit_poller(&mut self) {}

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn prefetch_rate_limits(&mut self) {
        self.stop_rate_limit_poller();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn should_prefetch_rate_limits(&self) -> bool {
        self.config.model_provider.requires_openai_auth && self.has_chatgpt_account
    }

    pub(super) fn lower_cost_preset(&self) -> Option<ModelPreset> {
        let models = self.model_catalog.try_list_models().ok()?;
        models
            .iter()
            .find(|preset| preset.show_in_picker && preset.model == NUDGE_MODEL_SLUG)
            .cloned()
    }

    pub(super) fn rate_limit_switch_prompt_hidden(&self) -> bool {
        self.config
            .notices
            .hide_rate_limit_model_nudge
            .unwrap_or(false)
    }

    pub(super) fn maybe_show_pending_rate_limit_prompt(&mut self) {
        if self.rate_limit_switch_prompt_hidden() {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
            return;
        }
        if !matches!(
            self.rate_limit_switch_prompt,
            RateLimitSwitchPromptState::Pending
        ) {
            return;
        }
        if let Some(preset) = self.lower_cost_preset() {
            self.open_rate_limit_switch_prompt(preset);
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Shown;
        } else {
            self.rate_limit_switch_prompt = RateLimitSwitchPromptState::Idle;
        }
    }

    pub(super) fn open_rate_limit_switch_prompt(&mut self, preset: ModelPreset) {
        let switch_model = preset.model;
        let switch_model_for_events = switch_model.clone();
        let switch_provider_id_for_events = self.current_model_provider_id().to_string();
        let default_effort: ReasoningEffortConfig = preset.default_reasoning_effort;

        let switch_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::AgentOp(
                AppCommand::override_turn_context(
                    /*cwd*/ None,
                    /*approval_policy*/ None,
                    /*approvals_reviewer*/ None,
                    /*sandbox_policy*/ None,
                    /*windows_sandbox_level*/ None,
                    Some(switch_provider_id_for_events.clone()),
                    Some(switch_model_for_events.clone()),
                    Some(Some(default_effort)),
                    /*summary*/ None,
                    /*service_tier*/ None,
                    /*collaboration_mode*/ None,
                    /*personality*/ None,
                )
                .into_core(),
            ));
            tx.send(AppEvent::UpdateModelSelection {
                model: switch_model_for_events.clone(),
                provider_id: switch_provider_id_for_events.clone(),
            });
            tx.send(AppEvent::UpdateReasoningEffort(Some(default_effort)));
        })];

        let keep_actions: Vec<SelectionAction> = Vec::new();
        let never_actions: Vec<SelectionAction> = vec![Box::new(|tx| {
            tx.send(AppEvent::UpdateRateLimitSwitchPromptHidden(true));
            tx.send(AppEvent::PersistRateLimitSwitchPromptHidden);
        })];
        let description = if preset.description.is_empty() {
            Some("Uses fewer credits for upcoming turns.".to_string())
        } else {
            Some(preset.description)
        };

        let items = vec![
            SelectionItem {
                name: format!("Switch to {switch_model}"),
                description,
                selected_description: None,
                is_current: false,
                actions: switch_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Keep current model".to_string(),
                description: None,
                selected_description: None,
                is_current: false,
                actions: keep_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Keep current model (never show again)".to_string(),
                description: Some(
                    "Hide future rate limit reminders about switching models.".to_string(),
                ),
                selected_description: None,
                is_current: false,
                actions: never_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Approaching rate limits".to_string()),
            subtitle: Some(format!("Switch to {switch_model} for lower credit usage?")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) fn token_usage(&self) -> TokenUsage {
        self.token_info
            .as_ref()
            .map(|ti| ti.total_token_usage.clone())
            .unwrap_or_default()
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.token_info = None;
        self.sync_status_budget_message();
    }
}
