use super::*;

#[derive(Debug)]
pub(crate) struct SessionInfoCell(CompositeHistoryCell);

impl HistoryCell for SessionInfoCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.display_lines(width)
    }

    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.committed_display_lines(width)
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.0.desired_height(width)
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.transcript_lines(width)
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.0.transcript_animation_tick()
    }

    fn mouse_targets(&self, width: u16) -> Vec<HistoryCellMouseTarget> {
        self.0.mouse_targets(width)
    }
}

pub(crate) fn new_session_info(
    _config: &Config,
    tui_config: &TuiRuntimeConfig,
    requested_model: &str,
    event: SessionConfiguredEvent,
    is_first_event: bool,
    tooltip_override: Option<String>,
    auth_plan: Option<PlanType>,
    show_fast_status: bool,
) -> SessionInfoCell {
    let SessionConfiguredEvent {
        model,
        cwd,
        reasoning_effort,
        ..
    } = event;

    let mut parts: Vec<Box<dyn HistoryCell>> = Vec::new();
    if is_first_event {
        let mut header = SessionHeaderHistoryCell::new_with_style_internal(
            model.clone(),
            Style::default(),
            reasoning_effort,
            show_fast_status,
            cwd,
            PRAXIS_CLI_VERSION,
            tui_config.animations,
        );
        header.billing_label = session_header_billing_label(auth_plan);
        if tui_config.show_tooltips {
            header.set_startup_notice(tooltip_override);
        }
        parts.push(Box::new(header));
    }
    if !is_first_event && requested_model != model {
        let lines = vec![
            "model changed:".magenta().bold().into(),
            format!("requested: {requested_model}").into(),
            format!("used: {model}").into(),
        ];
        parts.push(Box::new(PlainHistoryCell { lines }));
    }

    SessionInfoCell(CompositeHistoryCell { parts })
}

fn session_header_billing_label(auth_plan: Option<PlanType>) -> String {
    match auth_plan {
        Some(PlanType::Free) => "ChatGPT Free".to_string(),
        Some(PlanType::Go) => "ChatGPT Go".to_string(),
        Some(PlanType::Plus) => "ChatGPT Plus".to_string(),
        Some(PlanType::Pro) => "ChatGPT Pro".to_string(),
        Some(PlanType::Team) => "ChatGPT Team".to_string(),
        Some(PlanType::SelfServeBusinessUsageBased) => "ChatGPT Business".to_string(),
        Some(PlanType::Business) => "ChatGPT Business".to_string(),
        Some(PlanType::EnterpriseCbpUsageBased) => "ChatGPT Enterprise".to_string(),
        Some(PlanType::Enterprise) => "ChatGPT Enterprise".to_string(),
        Some(PlanType::Edu) => "ChatGPT Edu".to_string(),
        Some(PlanType::Unknown) | None => "API Usage Billing".to_string(),
    }
}

pub(crate) fn new_user_prompt(
    message: String,
    text_elements: Vec<TextElement>,
    local_image_paths: Vec<PathBuf>,
    remote_image_urls: Vec<String>,
) -> UserHistoryCell {
    UserHistoryCell {
        message,
        text_elements,
        local_image_paths,
        remote_image_urls,
    }
}

#[derive(Debug)]
pub(crate) struct SessionHeaderHistoryCell {
    version: &'static str,
    model: String,
    model_style: Style,
    reasoning_effort: Option<ReasoningEffortConfig>,
    show_fast_status: bool,
    directory: PathBuf,
    billing_label: String,
    startup_notice: Option<String>,
    recent_activity: Vec<StartupRecentActivity>,
    show_home_directory_warning: bool,
    animations_enabled: bool,
    created_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StartupRecentActivity {
    thread_id: ThreadId,
    title: String,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PuppyPose {
    Alert,
    Blink,
    PawsUp,
    WagTailLeft,
    WagTailRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PuppyAnimationFrame {
    pose: PuppyPose,
    offset_rows: usize,
}

const SESSION_HEADER_PUPPY_CANVAS_HEIGHT: usize = 5;
const SESSION_HEADER_PUPPY_FRAME_MS: u128 = 60;
const SESSION_HEADER_PUPPY_CYCLE_FRAMES: u64 = 64;
const SESSION_HEADER_HORIZONTAL_BREAKPOINT: usize = 70;
const SESSION_HEADER_DIVIDER_WIDTH: usize = 3;
const SESSION_HEADER_LEFT_MIN_WIDTH: usize = 28;
const SESSION_HEADER_LEFT_MAX_WIDTH: usize = 36;
const SESSION_HEADER_RIGHT_MIN_WIDTH: usize = 30;
const SESSION_HEADER_HORIZONTAL_MIN_INNER_WIDTH: usize =
    SESSION_HEADER_LEFT_MIN_WIDTH + SESSION_HEADER_DIVIDER_WIDTH + SESSION_HEADER_RIGHT_MIN_WIDTH;
const SESSION_HEADER_ACCENT: Color = Color::Rgb(245, 142, 44);
const SESSION_HEADER_ACCENT_SOFT: Color = Color::Rgb(255, 196, 128);
const SESSION_HEADER_INACTIVE: Color = Color::Rgb(153, 153, 153);
const SESSION_HEADER_RECENT_ACTIVITY_FOOTER: &str = "/resume for more";
const RESTING_PUPPY_FRAME: PuppyAnimationFrame = PuppyAnimationFrame {
    pose: PuppyPose::Alert,
    offset_rows: 0,
};

impl SessionHeaderHistoryCell {
    #[cfg(test)]
    fn new(
        model: String,
        reasoning_effort: Option<ReasoningEffortConfig>,
        show_fast_status: bool,
        directory: PathBuf,
        version: &'static str,
    ) -> Self {
        Self::new_with_style_internal(
            model,
            Style::default(),
            reasoning_effort,
            show_fast_status,
            directory,
            version,
            /*animations_enabled*/ false,
        )
    }

    #[cfg(test)]
    fn new_animated(
        model: String,
        reasoning_effort: Option<ReasoningEffortConfig>,
        show_fast_status: bool,
        directory: PathBuf,
        version: &'static str,
    ) -> Self {
        Self::new_with_style_internal(
            model,
            Style::default(),
            reasoning_effort,
            show_fast_status,
            directory,
            version,
            /*animations_enabled*/ true,
        )
    }

    fn new_with_style_internal(
        model: String,
        model_style: Style,
        reasoning_effort: Option<ReasoningEffortConfig>,
        show_fast_status: bool,
        directory: PathBuf,
        version: &'static str,
        animations_enabled: bool,
    ) -> Self {
        let show_home_directory_warning = dirs::home_dir().is_some_and(|home| home == directory);
        Self {
            version,
            model,
            model_style,
            reasoning_effort,
            show_fast_status,
            directory,
            billing_label: "API Usage Billing".to_string(),
            startup_notice: None,
            recent_activity: Vec::new(),
            show_home_directory_warning,
            animations_enabled,
            created_at: Instant::now(),
        }
    }

    fn set_startup_notice(&mut self, notice: Option<String>) {
        self.startup_notice = notice.and_then(|notice| {
            let trimmed = notice.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });
    }

    fn current_puppy_frame(&self) -> PuppyAnimationFrame {
        if !self.animations_enabled {
            return RESTING_PUPPY_FRAME;
        }

        let frame_idx = ((self.created_at.elapsed().as_millis() / SESSION_HEADER_PUPPY_FRAME_MS)
            as u64)
            % SESSION_HEADER_PUPPY_CYCLE_FRAMES;
        Self::puppy_frame_for_cycle(frame_idx)
    }

    fn puppy_animation_tick(&self) -> Option<u64> {
        if !self.animations_enabled {
            return None;
        }

        Some((self.created_at.elapsed().as_millis() / SESSION_HEADER_PUPPY_FRAME_MS) as u64)
    }

    fn puppy_frame_for_cycle(frame_idx: u64) -> PuppyAnimationFrame {
        match frame_idx {
            0..=5 => RESTING_PUPPY_FRAME,
            6..=7 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailRight,
                offset_rows: 0,
            },
            8..=9 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailLeft,
                offset_rows: 0,
            },
            10..=11 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailRight,
                offset_rows: 0,
            },
            12..=13 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailLeft,
                offset_rows: 0,
            },
            14..=15 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailRight,
                offset_rows: 0,
            },
            16..=17 => PuppyAnimationFrame {
                pose: PuppyPose::WagTailLeft,
                offset_rows: 0,
            },
            18..=25 => RESTING_PUPPY_FRAME,
            26 => PuppyAnimationFrame {
                pose: PuppyPose::Blink,
                offset_rows: 0,
            },
            27..=35 => RESTING_PUPPY_FRAME,
            36..=37 => PuppyAnimationFrame {
                pose: PuppyPose::Alert,
                offset_rows: 1,
            },
            38..=40 => PuppyAnimationFrame {
                pose: PuppyPose::PawsUp,
                offset_rows: 0,
            },
            41 => RESTING_PUPPY_FRAME,
            42..=43 => PuppyAnimationFrame {
                pose: PuppyPose::Alert,
                offset_rows: 1,
            },
            44..=46 => PuppyAnimationFrame {
                pose: PuppyPose::PawsUp,
                offset_rows: 0,
            },
            47..=54 => RESTING_PUPPY_FRAME,
            55 => PuppyAnimationFrame {
                pose: PuppyPose::Blink,
                offset_rows: 0,
            },
            56..=63 => RESTING_PUPPY_FRAME,
            _ => RESTING_PUPPY_FRAME,
        }
    }

    fn render_display_lines(
        &self,
        width: u16,
        puppy_frame: PuppyAnimationFrame,
    ) -> Vec<Line<'static>> {
        let Some((inner_width, horizontal_layout)) = self.resolved_layout(width) else {
            return Vec::new();
        };

        if horizontal_layout {
            self.render_horizontal_display_lines(inner_width, puppy_frame)
        } else {
            self.render_compact_display_lines(inner_width, puppy_frame)
        }
    }

    fn resolved_layout(&self, width: u16) -> Option<(usize, bool)> {
        let available_inner_width = card_inner_width(width, usize::MAX)?;
        if available_inner_width >= SESSION_HEADER_HORIZONTAL_BREAKPOINT {
            return Some((
                self.preferred_horizontal_inner_width(available_inner_width),
                true,
            ));
        }
        Some((available_inner_width, false))
    }

    fn preferred_horizontal_inner_width(&self, available_inner_width: usize) -> usize {
        let Some((left_width, max_right_width)) =
            self.horizontal_panel_widths(available_inner_width)
        else {
            return available_inner_width;
        };
        let preferred_right_width = self.preferred_right_panel_width(max_right_width);
        (left_width + SESSION_HEADER_DIVIDER_WIDTH + preferred_right_width).clamp(
            SESSION_HEADER_HORIZONTAL_MIN_INNER_WIDTH,
            available_inner_width,
        )
    }

    fn format_directory(&self, max_width: Option<usize>) -> String {
        Self::format_directory_inner(&self.directory, max_width)
    }

    fn format_directory_inner(directory: &Path, max_width: Option<usize>) -> String {
        let formatted = if let Some(rel) = relativize_to_home(directory) {
            if rel.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~{}{}", std::path::MAIN_SEPARATOR, rel.display())
            }
        } else {
            directory.display().to_string()
        };

        if let Some(max_width) = max_width {
            if max_width == 0 {
                return String::new();
            }
            if UnicodeWidthStr::width(formatted.as_str()) > max_width {
                return crate::text_formatting::workspace_truncate_path(&formatted, max_width);
            }
        }

        formatted
    }

    fn reasoning_label(&self) -> Option<&'static str> {
        self.reasoning_effort.map(|effort| match effort {
            ReasoningEffortConfig::Minimal => "minimal",
            ReasoningEffortConfig::Low => "low",
            ReasoningEffortConfig::Medium => "medium",
            ReasoningEffortConfig::High => "high",
            ReasoningEffortConfig::XHigh => "xhigh",
            ReasoningEffortConfig::None => "none",
        })
    }

    fn model_display_name(&self) -> String {
        let mut label = self.model.clone();
        if let Some(reasoning) = self.reasoning_label() {
            label.push(' ');
            label.push_str(reasoning);
        }
        if self.show_fast_status {
            label.push_str(" fast");
        }
        label
    }

    fn render_horizontal_display_lines(
        &self,
        inner_width: usize,
        puppy_frame: PuppyAnimationFrame,
    ) -> Vec<Line<'static>> {
        let Some((left_width, right_width)) = self.horizontal_panel_widths(inner_width) else {
            return self.render_compact_display_lines(inner_width, puppy_frame);
        };
        let dir_line_plain = self.format_directory(Some(left_width.saturating_sub(2)));

        let mut left_lines: Vec<Line<'static>> = vec![
            Self::blank_padded_line(left_width),
            Self::workspace_line(
                Line::from(vec![Span::styled("Welcome back!", Self::welcome_style())]),
                left_width,
            ),
            Self::blank_padded_line(left_width),
        ];
        let puppy_lines = Self::puppy_logo_lines(9, puppy_frame)
            .into_iter()
            .map(|line| Self::workspace_line(line, left_width));
        left_lines.extend(puppy_lines);
        left_lines.push(Self::blank_padded_line(left_width));
        left_lines.push(Self::workspace_line(self.model_billing_line(), left_width));
        left_lines.push(Self::workspace_line(
            Line::from(vec![Span::from(dir_line_plain).dim()]),
            left_width,
        ));

        let right_lines = self.render_right_panel(right_width);
        let total_rows = left_lines.len().max(right_lines.len());
        left_lines.resize_with(total_rows, || Self::blank_padded_line(left_width));
        let mut right_lines = right_lines;
        right_lines.resize_with(total_rows, || Self::blank_padded_line(right_width));

        let divider = Span::styled(
            " │ ",
            Style::default()
                .fg(SESSION_HEADER_ACCENT)
                .add_modifier(Modifier::DIM),
        );
        let mut body_lines = Vec::with_capacity(total_rows);
        for idx in 0..total_rows {
            let mut spans = left_lines[idx].spans.clone();
            spans.push(divider.clone());
            spans.extend(right_lines[idx].spans.clone());
            body_lines.push(Line::from(spans));
        }

        Self::with_titled_border(body_lines, inner_width, self.border_title_spans())
    }

    fn render_compact_display_lines(
        &self,
        inner_width: usize,
        puppy_frame: PuppyAnimationFrame,
    ) -> Vec<Line<'static>> {
        let mut body = vec![
            Self::workspace_line(
                Line::from(vec![Span::styled("Welcome back!", Self::welcome_style())]),
                inner_width,
            ),
            Self::blank_padded_line(inner_width),
        ];
        body.extend(
            Self::puppy_logo_lines(9, puppy_frame)
                .into_iter()
                .map(|line| Self::workspace_line(line, inner_width)),
        );
        body.push(Self::blank_padded_line(inner_width));
        body.push(self.model_billing_line_padded(inner_width));
        body.push(Self::workspace_line(
            Line::from(vec![
                Span::from(self.format_directory(Some(inner_width))).dim(),
            ]),
            inner_width,
        ));
        body.push(Self::blank_padded_line(inner_width));
        body.extend(self.render_right_panel(inner_width));

        Self::with_titled_border(body, inner_width, self.border_title_spans())
    }

    fn render_right_panel(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Self::section_title_line("Tips for getting started", width));
        for line in self.tip_feed_lines() {
            lines.extend(Self::wrap_and_pad_line(line, width));
        }
        if self.show_home_directory_warning {
            lines.extend(Self::wrap_and_pad_line(
                Self::home_directory_warning_line(),
                width,
            ));
        }

        if let Some(notice) = self.startup_notice.as_ref() {
            lines.push(Self::separator_line(width));
            lines.push(Self::section_title_line("What's new", width));
            lines.extend(Self::wrap_and_pad_line(
                Line::from(vec![Span::from(notice.clone())]),
                width,
            ));
        }

        lines.push(Self::separator_line(width));
        lines.push(Self::section_title_line("Recent activity", width));

        if self.recent_activity.is_empty() {
            lines.push(Self::left_align_line(
                Line::from(vec![Span::styled(
                    "No recent activity",
                    Self::muted_text_style(),
                )]),
                width,
            ));
            return lines;
        }

        let timestamps = self
            .recent_activity
            .iter()
            .map(|activity| activity.updated_at.map(Self::human_time_ago_short))
            .collect::<Vec<_>>();
        let timestamp_width = timestamps
            .iter()
            .flatten()
            .map(|label| UnicodeWidthStr::width(label.as_str()))
            .max()
            .unwrap_or(0);
        let has_timestamp_column = timestamp_width > 0;

        for (activity, timestamp) in self.recent_activity.iter().zip(timestamps.iter()) {
            let text_width = width
                .saturating_sub(timestamp_width + if has_timestamp_column { 2 } else { 0 })
                .max(1);
            let title = truncate_text(&activity.title, text_width);
            let mut spans = Vec::new();
            if has_timestamp_column {
                let label = timestamp.as_deref().unwrap_or("");
                if !label.is_empty() {
                    spans.push(Span::styled(label.to_string(), Self::muted_text_style()));
                }
                let padding = timestamp_width.saturating_sub(UnicodeWidthStr::width(label));
                if padding > 0 {
                    spans.push(Span::styled(" ".repeat(padding), Self::muted_text_style()));
                }
                spans.push(Span::from("  "));
            }
            spans.push(Span::from(title));
            lines.push(Self::left_align_line(Line::from(spans), width));
        }

        lines.push(Self::footer_line(
            SESSION_HEADER_RECENT_ACTIVITY_FOOTER,
            width,
        ));

        lines
    }

    fn tip_feed_lines(&self) -> Vec<Line<'static>> {
        vec![
            Self::tip_command_line("Run ", "/init", " to create an AGENTS.md for this repo."),
            Self::tip_command_line("Use ", "/resume", " to reopen an earlier thread."),
            Self::tip_command_line("Use ", "/model", " to switch model or reasoning effort."),
        ]
    }

    fn tip_command_line(
        prefix: &'static str,
        command: &'static str,
        suffix: &'static str,
    ) -> Line<'static> {
        Line::from(vec![
            Span::from(prefix),
            Span::styled(command, Self::command_style()),
            Span::from(suffix),
        ])
    }

    fn home_directory_warning_line() -> Line<'static> {
        Line::from(vec![
            Span::styled(
                "Note:",
                Self::muted_text_style().add_modifier(Modifier::BOLD),
            ),
            Span::from(
                " praxis is running in your home directory. Launch it in a project directory for the best experience.",
            ),
        ])
    }

    fn preferred_right_panel_width(&self, max_width: usize) -> usize {
        let mut preferred_width = UnicodeWidthStr::width("Tips for getting started");
        for line in self.tip_feed_lines() {
            preferred_width = preferred_width.max(Self::line_width(&line));
        }
        if self.show_home_directory_warning {
            preferred_width =
                preferred_width.max(Self::line_width(&Self::home_directory_warning_line()));
        }

        if let Some(notice) = self.startup_notice.as_ref() {
            preferred_width = preferred_width
                .max(UnicodeWidthStr::width("What's new"))
                .max(UnicodeWidthStr::width(notice.as_str()));
        }

        preferred_width = preferred_width.max(UnicodeWidthStr::width("Recent activity"));
        if self.recent_activity.is_empty() {
            preferred_width = preferred_width.max(UnicodeWidthStr::width("No recent activity"));
        } else {
            let timestamps = self
                .recent_activity
                .iter()
                .map(|activity| activity.updated_at.map(Self::human_time_ago_short))
                .collect::<Vec<_>>();
            let timestamp_width = timestamps
                .iter()
                .flatten()
                .map(|label| UnicodeWidthStr::width(label.as_str()))
                .max()
                .unwrap_or(0);
            let title_width = self
                .recent_activity
                .iter()
                .map(|activity| UnicodeWidthStr::width(activity.title.as_str()))
                .max()
                .unwrap_or(0);
            preferred_width = preferred_width.max(
                title_width
                    + if timestamp_width > 0 {
                        timestamp_width + 2
                    } else {
                        0
                    },
            );
            preferred_width = preferred_width.max(UnicodeWidthStr::width(
                SESSION_HEADER_RECENT_ACTIVITY_FOOTER,
            ));
        }

        preferred_width.clamp(SESSION_HEADER_RIGHT_MIN_WIDTH, max_width)
    }

    fn model_billing_line(&self) -> Line<'static> {
        let muted_style = Self::muted_text_style();
        let mut spans = vec![Span::styled(
            self.model_display_name(),
            self.model_style.patch(muted_style),
        )];
        spans.push(Span::styled(" · ", muted_style));
        spans.push(Span::styled(self.billing_label.clone(), muted_style));
        Line::from(spans)
    }

    fn model_billing_line_padded(&self, width: usize) -> Line<'static> {
        Self::workspace_line(self.model_billing_line(), width)
    }

    fn border_title_spans(&self) -> Vec<Span<'static>> {
        vec![
            Span::styled(" Praxis CLI ", Self::section_title_style()),
            Span::styled(
                format!("v{} ", self.version),
                Style::default().fg(SESSION_HEADER_INACTIVE),
            ),
        ]
    }

    fn welcome_style() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn section_title_style() -> Style {
        Style::default()
            .fg(SESSION_HEADER_ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    fn command_style() -> Style {
        Style::default()
            .fg(SESSION_HEADER_ACCENT_SOFT)
            .add_modifier(Modifier::BOLD)
    }

    fn muted_text_style() -> Style {
        Style::default().fg(SESSION_HEADER_INACTIVE)
    }

    fn footer_style() -> Style {
        Self::muted_text_style().add_modifier(Modifier::ITALIC)
    }

    fn footer_line(text: &'static str, width: usize) -> Line<'static> {
        Self::left_align_line(
            Line::from(vec![Span::styled(text, Self::footer_style())]),
            width,
        )
    }

    fn section_title_line(title: &'static str, width: usize) -> Line<'static> {
        Self::left_align_line(
            Line::from(vec![Span::styled(title, Self::section_title_style())]),
            width,
        )
    }

    fn separator_line(width: usize) -> Line<'static> {
        Self::left_align_line(
            Line::from(vec![Span::styled(
                "─".repeat(width),
                Style::default()
                    .fg(SESSION_HEADER_ACCENT)
                    .add_modifier(Modifier::DIM),
            )]),
            width,
        )
    }

    fn horizontal_panel_widths(&self, inner_width: usize) -> Option<(usize, usize)> {
        if inner_width < SESSION_HEADER_HORIZONTAL_MIN_INNER_WIDTH {
            return None;
        }

        let model_line_plain = format!("{} · {}", self.model_display_name(), self.billing_label);
        let left_content_width = [
            UnicodeWidthStr::width("Welcome back!"),
            UnicodeWidthStr::width(model_line_plain.as_str()),
            9usize,
        ]
        .into_iter()
        .max()
        .unwrap_or(SESSION_HEADER_LEFT_MIN_WIDTH)
            + 2;

        let max_left_width = inner_width
            .saturating_sub(SESSION_HEADER_DIVIDER_WIDTH + SESSION_HEADER_RIGHT_MIN_WIDTH)
            .min(SESSION_HEADER_LEFT_MAX_WIDTH);
        if max_left_width < SESSION_HEADER_LEFT_MIN_WIDTH {
            return None;
        }

        let left_width = left_content_width.clamp(SESSION_HEADER_LEFT_MIN_WIDTH, max_left_width);
        let right_width = inner_width.saturating_sub(left_width + SESSION_HEADER_DIVIDER_WIDTH);
        (right_width >= SESSION_HEADER_RIGHT_MIN_WIDTH).then_some((left_width, right_width))
    }

    fn blank_padded_line(width: usize) -> Line<'static> {
        Line::from(" ".repeat(width))
    }

    fn left_align_line(line: Line<'static>, width: usize) -> Line<'static> {
        Self::pad_line(line, 0, width)
    }

    fn workspace_line(line: Line<'static>, width: usize) -> Line<'static> {
        let used = Self::line_width(&line);
        if used >= width {
            return line;
        }
        let left = (width - used) / 2;
        let right = width - used - left;
        Self::pad_line_with_sides(line, left, right)
    }

    fn wrap_and_pad_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
        adaptive_wrap_lines([line], RtOptions::new(width))
            .into_iter()
            .map(|line| Self::left_align_line(line, width))
            .collect()
    }

    fn line_width(line: &Line<'static>) -> usize {
        line.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum()
    }

    fn pad_line(line: Line<'static>, _left: usize, width: usize) -> Line<'static> {
        let used = Self::line_width(&line);
        if used >= width {
            return line;
        }
        Self::pad_line_with_sides(line, 0, width - used)
    }

    fn pad_line_with_sides(line: Line<'static>, left: usize, right: usize) -> Line<'static> {
        let mut spans = Vec::with_capacity(line.spans.len() + 2);
        if left > 0 {
            spans.push(Span::from(" ".repeat(left)));
        }
        spans.extend(line.spans);
        if right > 0 {
            spans.push(Span::from(" ".repeat(right)));
        }
        Line::from(spans)
    }

    fn with_titled_border(
        lines: Vec<Line<'static>>,
        content_width: usize,
        title_spans: Vec<Span<'static>>,
    ) -> Vec<Line<'static>> {
        let border_style = Style::default().fg(SESSION_HEADER_ACCENT);
        let border_inner_width = content_width + 2;
        let title_width = title_spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        let left_rule = 3usize.min(border_inner_width.saturating_sub(title_width));
        let right_rule = border_inner_width.saturating_sub(left_rule + title_width);

        let mut out = Vec::with_capacity(lines.len() + 2);
        let mut top = vec![
            Span::styled("╭", border_style),
            Span::styled("─".repeat(left_rule), border_style),
        ];
        top.extend(title_spans);
        top.push(Span::styled("─".repeat(right_rule), border_style));
        top.push(Span::styled("╮", border_style));
        out.push(Line::from(top));

        for line in lines {
            let mut spans = Vec::with_capacity(line.spans.len() + 4);
            spans.push(Span::styled("│ ", border_style));
            spans.extend(line.spans);
            let used_width = spans
                .iter()
                .skip(1)
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>();
            if used_width < content_width + 1 {
                spans.push(Span::from(" ".repeat(content_width + 1 - used_width)));
            }
            spans.push(Span::styled("│", border_style));
            out.push(Line::from(spans));
        }

        out.push(Line::from(vec![
            Span::styled("╰", border_style),
            Span::styled("─".repeat(border_inner_width), border_style),
            Span::styled("╯", border_style),
        ]));
        out
    }

    fn human_time_ago_short(ts: DateTime<Utc>) -> String {
        let now = Utc::now();
        let secs = (now - ts).num_seconds();
        let abs_secs = secs.unsigned_abs();
        let intervals = [
            ("y", 31_536_000u64),
            ("mo", 2_592_000u64),
            ("w", 604_800u64),
            ("d", 86_400u64),
            ("h", 3_600u64),
            ("m", 60u64),
            ("s", 1u64),
        ];

        for (unit, interval_secs) in intervals {
            if abs_secs >= interval_secs {
                let value = abs_secs / interval_secs;
                return if secs < 0 {
                    format!("in {value}{unit}")
                } else {
                    format!("{value}{unit} ago")
                };
            }
        }

        "0s ago".to_string()
    }

    fn puppy_logo_lines(inner_width: usize, frame: PuppyAnimationFrame) -> Vec<Line<'static>> {
        if inner_width < 9 {
            return Vec::new();
        }

        let outline = Style::default().fg(Color::Rgb(245, 142, 44)).bold();
        let fill_rgb = Color::Rgb(255, 191, 110);
        let fill = Style::default().fg(fill_rgb).bg(fill_rgb).bold();
        let fill_gap = Style::default().bg(fill_rgb);
        let face = Style::default()
            .fg(Color::Rgb(76, 39, 8))
            .bg(fill_rgb)
            .bold();
        let feet = Style::default().fg(Color::Rgb(245, 142, 44));

        let eyes = match frame.pose {
            PuppyPose::Blink => "▔",
            PuppyPose::Alert
            | PuppyPose::PawsUp
            | PuppyPose::WagTailLeft
            | PuppyPose::WagTailRight => "●",
        };
        let mouth = match frame.pose {
            PuppyPose::PawsUp => "◡",
            PuppyPose::Alert
            | PuppyPose::Blink
            | PuppyPose::WagTailLeft
            | PuppyPose::WagTailRight => "▿",
        };
        let tail = match frame.pose {
            PuppyPose::WagTailLeft => "╱",
            PuppyPose::WagTailRight => "╲",
            _ => "│",
        };

        let top_row = match frame.pose {
            PuppyPose::PawsUp => ("▗▟▜", "███", "▛▙▖"),
            PuppyPose::Alert
            | PuppyPose::Blink
            | PuppyPose::WagTailLeft
            | PuppyPose::WagTailRight => (" ▗▟", "███", "▙▖ "),
        };
        let brow_row = match frame.pose {
            PuppyPose::PawsUp => (" ▜", "█████", "▛ "),
            PuppyPose::Alert
            | PuppyPose::Blink
            | PuppyPose::WagTailLeft
            | PuppyPose::WagTailRight => ("▐█", "█████", "█▌"),
        };

        let art = vec![
            Line::from(vec![
                Span::styled(top_row.0, outline),
                Span::styled(top_row.1, fill),
                Span::styled(top_row.2, outline),
            ]),
            Line::from(vec![
                Span::styled(brow_row.0, outline),
                Span::styled(brow_row.1, fill),
                Span::styled(brow_row.2, outline),
            ]),
            Line::from(vec![
                Span::styled("▐█", outline),
                Span::styled("█", fill),
                Span::styled(eyes.to_string(), face),
                Span::styled(" ", fill_gap),
                Span::styled(eyes.to_string(), face),
                Span::styled("█", fill),
                Span::styled("█▌", outline),
            ]),
            Line::from(vec![
                Span::styled(" ▜", outline),
                Span::styled("██", fill),
                Span::styled(mouth.to_string(), face),
                Span::styled("██", fill),
                Span::styled("▛ ", outline),
                Span::styled(tail, outline),
            ]),
            Line::from(vec![Span::styled("  ▘▘ ▝▝  ".to_string(), feet)]),
        ];

        let mut logo = Vec::with_capacity(SESSION_HEADER_PUPPY_CANVAS_HEIGHT);
        for _ in 0..frame.offset_rows.min(SESSION_HEADER_PUPPY_CANVAS_HEIGHT) {
            logo.push(Line::from(Vec::<Span<'static>>::new()));
        }
        let visible_rows = SESSION_HEADER_PUPPY_CANVAS_HEIGHT.saturating_sub(logo.len());
        logo.extend(art.into_iter().take(visible_rows));
        logo
    }
}

impl HistoryCell for SessionHeaderHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.render_display_lines(width, self.current_puppy_frame())
    }

    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.render_display_lines(width, RESTING_PUPPY_FRAME)
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.puppy_animation_tick()
    }
}

#[derive(Debug)]
pub(crate) struct CompositeHistoryCell {
    parts: Vec<Box<dyn HistoryCell>>,
}

impl CompositeHistoryCell {
    pub(crate) fn new(parts: Vec<Box<dyn HistoryCell>>) -> Self {
        Self { parts }
    }
}

impl HistoryCell for CompositeHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.display_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.committed_display_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.parts
            .iter()
            .filter_map(|part| part.transcript_animation_tick())
            .max()
    }
}
