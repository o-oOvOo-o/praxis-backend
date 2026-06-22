//! A live task status row rendered above the composer while the agent is busy.
//!
//! The row owns spinner timing, the optional interrupt hint, and short inline
//! context (for example, the unified-exec background-process summary). Keeping
//! these pieces on one line avoids vertical layout churn in the bottom pane.

use std::time::Duration;
use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app_event_sender::AppEventSender;
use crate::exec_cell::spinner;
use crate::key_hint;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use crate::shimmer::shimmer_spans;
use crate::status_runtime::GENERIC_STATUS_HEADER;
use crate::status_runtime::STATUS_ANIMATION_FRAME_DELAY_FOCUSED;
use crate::status_runtime::STATUS_ANIMATION_FRAME_DELAY_UNFOCUSED;
use crate::text_formatting::capitalize_first;
use crate::thinking_persona::ThinkingPersona;
use crate::tui::FrameRequester;
use crate::wrapping::RtOptions;
use crate::wrapping::word_wrap_lines;

pub(crate) const STATUS_DETAILS_DEFAULT_MAX_LINES: usize = 3;
const DETAILS_PREFIX: &str = "  · ";
const FOOTER_PREFIX: &str = "  · ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusDetailsCapitalization {
    CapitalizeFirst,
    Preserve,
}

/// Displays a single-line in-progress status with optional wrapped details.
pub(crate) struct StatusIndicatorWidget {
    /// Live header text supplied by the runtime status snapshot.
    header: String,
    details: Option<String>,
    activity_message: Option<String>,
    footer_lines: Vec<String>,
    details_max_lines: usize,
    thinking_persona: ThinkingPersona,
    /// Optional suffix rendered after the elapsed/interrupt segment.
    inline_message: Option<String>,
    show_interrupt_hint: bool,

    elapsed_running: Duration,
    last_resume_at: Instant,
    is_paused: bool,
    app_event_tx: AppEventSender,
    frame_requester: FrameRequester,
    animations_enabled: bool,
    terminal_focused: bool,
}

// Format elapsed seconds into a compact human-friendly form used by the status line.
// Examples: 0s, 59s, 1m 00s, 59m 59s, 1h 00m 00s, 2h 03m 09s
pub fn fmt_elapsed_compact(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        return format!("{elapsed_secs}s");
    }
    if elapsed_secs < 3600 {
        let minutes = elapsed_secs / 60;
        let seconds = elapsed_secs % 60;
        return format!("{minutes}m {seconds:02}s");
    }
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    format!("{hours}h {minutes:02}m {seconds:02}s")
}

impl StatusIndicatorWidget {
    pub(crate) fn new(
        app_event_tx: AppEventSender,
        frame_requester: FrameRequester,
        animations_enabled: bool,
    ) -> Self {
        Self {
            header: GENERIC_STATUS_HEADER.to_string(),
            details: None,
            activity_message: None,
            footer_lines: Vec::new(),
            details_max_lines: STATUS_DETAILS_DEFAULT_MAX_LINES,
            thinking_persona: ThinkingPersona::None,
            inline_message: None,
            show_interrupt_hint: true,
            elapsed_running: Duration::ZERO,
            last_resume_at: Instant::now(),
            is_paused: false,

            app_event_tx,
            frame_requester,
            animations_enabled,
            terminal_focused: true,
        }
    }

    pub(crate) fn interrupt(&self) {
        self.app_event_tx.interrupt();
    }

    /// Update the animated header label (left of the brackets).
    pub(crate) fn update_header(&mut self, header: String) {
        self.header = header;
    }

    /// Update the details text shown below the header.
    pub(crate) fn update_details(
        &mut self,
        details: Option<String>,
        capitalization: StatusDetailsCapitalization,
        max_lines: usize,
    ) {
        self.details_max_lines = max_lines.max(1);
        self.details = details
            .filter(|details| !details.is_empty())
            .map(|details| {
                let trimmed = details.trim_start();
                match capitalization {
                    StatusDetailsCapitalization::CapitalizeFirst => capitalize_first(trimmed),
                    StatusDetailsCapitalization::Preserve => trimmed.to_string(),
                }
            });
    }

    pub(crate) fn update_thinking_persona(&mut self, persona: ThinkingPersona) {
        self.thinking_persona = persona;
    }

    /// Update the inline suffix text shown after the elapsed/interrupt segment.
    ///
    /// Callers should provide plain, already-contextualized text. Passing
    /// verbose status prose here can cause frequent width truncation and hide
    /// the more important elapsed/interrupt hint.
    pub(crate) fn update_inline_message(&mut self, message: Option<String>) {
        self.inline_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn update_activity_message(&mut self, message: Option<String>) {
        self.activity_message = message
            .map(|message| message.trim().to_string())
            .filter(|message| !message.is_empty());
    }

    pub(crate) fn update_footer_message(&mut self, message: Option<String>) {
        self.footer_lines = message
            .into_iter()
            .flat_map(|message| {
                message
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
    }

    #[cfg(test)]
    pub(crate) fn header(&self) -> &str {
        &self.header
    }

    #[cfg(test)]
    pub(crate) fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn activity_message(&self) -> Option<&str> {
        self.activity_message.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn footer_lines(&self) -> &[String] {
        &self.footer_lines
    }

    pub(crate) fn set_interrupt_hint_visible(&mut self, visible: bool) {
        self.show_interrupt_hint = visible;
    }

    pub(crate) fn set_terminal_focused(&mut self, focused: bool) {
        self.terminal_focused = focused;
    }

    #[cfg(test)]
    pub(crate) fn interrupt_hint_visible(&self) -> bool {
        self.show_interrupt_hint
    }

    pub(crate) fn pause_timer(&mut self) {
        self.pause_timer_at(Instant::now());
    }

    pub(crate) fn resume_timer(&mut self) {
        self.resume_timer_at(Instant::now());
    }

    pub(crate) fn pause_timer_at(&mut self, now: Instant) {
        if self.is_paused {
            return;
        }
        self.elapsed_running += now.saturating_duration_since(self.last_resume_at);
        self.is_paused = true;
    }

    pub(crate) fn resume_timer_at(&mut self, now: Instant) {
        if !self.is_paused {
            return;
        }
        self.last_resume_at = now;
        self.is_paused = false;
        self.frame_requester.schedule_frame();
    }

    fn elapsed_duration_at(&self, now: Instant) -> Duration {
        let mut elapsed = self.elapsed_running;
        if !self.is_paused {
            elapsed += now.saturating_duration_since(self.last_resume_at);
        }
        elapsed
    }

    fn animation_frame_delay(&self) -> Duration {
        if self.terminal_focused {
            STATUS_ANIMATION_FRAME_DELAY_FOCUSED
        } else {
            STATUS_ANIMATION_FRAME_DELAY_UNFOCUSED
        }
    }

    fn elapsed_seconds_at(&self, now: Instant) -> u64 {
        self.elapsed_duration_at(now).as_secs()
    }

    pub fn elapsed_seconds(&self) -> u64 {
        self.elapsed_seconds_at(Instant::now())
    }

    fn wrapped_prefixed_lines(
        &self,
        prefix: &'static str,
        text: &str,
        width: u16,
        max_lines: usize,
    ) -> Vec<Line<'static>> {
        if width == 0 || text.is_empty() {
            return Vec::new();
        }

        let prefix_width = UnicodeWidthStr::width(prefix);
        let opts = RtOptions::new(usize::from(width))
            .initial_indent(Line::from(prefix.dim()))
            .subsequent_indent(Line::from(Span::from(" ".repeat(prefix_width)).dim()))
            .break_words(/*break_words*/ true);

        let mut out = word_wrap_lines(text.lines().map(|line| vec![line.dim()]), opts);
        if out.len() > max_lines {
            out.truncate(max_lines);
            let content_width = usize::from(width).saturating_sub(prefix_width).max(1);
            let max_base_len = content_width.saturating_sub(1);
            if let Some(last) = out.last_mut()
                && let Some(span) = last.spans.last_mut()
            {
                let trimmed: String = span.content.as_ref().chars().take(max_base_len).collect();
                *span = format!("{trimmed}…").dim();
            }
        }

        out
    }

    /// Wrap the details text into a fixed width and return the lines, truncating if necessary.
    fn wrapped_details_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out = if let Some(details) = self.details.as_deref() {
            self.wrapped_prefixed_lines(DETAILS_PREFIX, details, width, self.details_max_lines)
        } else {
            Vec::new()
        };

        if let Some(activity) = self.activity_message.as_deref()
            && self.details.as_deref() != Some(activity)
        {
            out.extend(self.wrapped_prefixed_lines(
                DETAILS_PREFIX,
                activity,
                width,
                /*max_lines*/ 1,
            ));
        }

        if out.is_empty()
            && let Some(activity) = self.activity_message.as_deref()
        {
            out =
                self.wrapped_prefixed_lines(DETAILS_PREFIX, activity, width, /*max_lines*/ 1);
        }

        for footer in &self.footer_lines {
            out.extend(self.wrapped_prefixed_lines(
                FOOTER_PREFIX,
                footer,
                width,
                /*max_lines*/ 1,
            ));
        }

        out
    }

    fn thinking_frame_visible(&self, width: u16) -> bool {
        self.thinking_persona.is_visible() && width >= 4
    }

    fn content_width_for(&self, width: u16) -> u16 {
        if self.thinking_frame_visible(width) {
            width.saturating_sub(2)
        } else {
            width
        }
    }
}

impl Renderable for StatusIndicatorWidget {
    fn desired_height(&self, width: u16) -> u16 {
        let content_width = self.content_width_for(width);
        let border_height = if self.thinking_frame_visible(width) {
            2
        } else {
            0
        };
        self.thinking_persona
            .desired_height(content_width)
            .saturating_add(1)
            .saturating_add(
                u16::try_from(self.wrapped_details_lines(content_width).len()).unwrap_or(0),
            )
            .saturating_add(border_height)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        if self.animations_enabled {
            // Schedule next animation frame.
            self.frame_requester
                .schedule_frame_in(self.animation_frame_delay());
        }
        let now = Instant::now();
        let elapsed_duration = self.elapsed_duration_at(now);
        let pretty_elapsed = fmt_elapsed_compact(elapsed_duration.as_secs());
        let rendered_header = self.header.as_str();

        let mut spans = Vec::with_capacity(5);
        spans.push(spinner(Some(self.last_resume_at), self.animations_enabled));
        spans.push(" ".into());
        if self.animations_enabled {
            spans.extend(shimmer_spans(rendered_header));
        } else if !rendered_header.is_empty() {
            spans.push(rendered_header.to_string().into());
        }
        spans.push(" ".into());
        spans.push("· ".dim());
        spans.push(pretty_elapsed.into());
        if self.show_interrupt_hint {
            spans.push(" · ".dim());
            spans.push(key_hint::plain(KeyCode::Esc).into());
            spans.push(" to interrupt".into());
        }
        if let Some(message) = &self.inline_message {
            // Keep optional context after elapsed/interrupt text so that core
            // interrupt affordances stay in a fixed visual location.
            spans.push(" · ".dim());
            spans.push(message.clone().dim());
        }
        let persona_elapsed = if self.animations_enabled {
            elapsed_duration
        } else {
            Duration::ZERO
        };
        let framed = self.thinking_frame_visible(area.width);
        let content_area = if framed {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White));
            block.render(area, buf);
            area.inner(Margin::new(1, 1))
        } else {
            area
        };
        if content_area.is_empty() {
            return;
        }

        let mut lines = self
            .thinking_persona
            .live_lines(content_area.width, persona_elapsed);
        let persona_line_count = lines.len();
        lines.push(truncate_line_with_ellipsis_if_overflow(
            Line::from(spans),
            usize::from(content_area.width),
        ));
        let used_lines = u16::try_from(persona_line_count)
            .unwrap_or(u16::MAX)
            .saturating_add(1);
        if content_area.height > used_lines {
            // If there is enough space, add the details lines below the header.
            let details = self.wrapped_details_lines(content_area.width);
            let max_details = usize::from(content_area.height.saturating_sub(used_lines));
            lines.extend(details.into_iter().take(max_details));
        }

        Paragraph::new(Text::from(lines)).render(content_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;
    use std::time::Instant;
    use tokio::sync::mpsc::unbounded_channel;

    use pretty_assertions::assert_eq;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn fmt_elapsed_compact_formats_seconds_minutes_hours() {
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 0), "0s");
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 1), "1s");
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 59), "59s");
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 60), "1m 00s");
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 61), "1m 01s");
        assert_eq!(fmt_elapsed_compact(3 * 60 + 5), "3m 05s");
        assert_eq!(fmt_elapsed_compact(59 * 60 + 59), "59m 59s");
        assert_eq!(fmt_elapsed_compact(/*elapsed_secs*/ 3600), "1h 00m 00s");
        assert_eq!(fmt_elapsed_compact(3600 + 60 + 1), "1h 01m 01s");
        assert_eq!(fmt_elapsed_compact(25 * 3600 + 2 * 60 + 3), "25h 02m 03s");
    }

    #[test]
    fn renders_with_working_header() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );

        // Render into a fixed-size test terminal and snapshot the backend.
        let mut terminal = Terminal::new(TestBackend::new(80, 2)).expect("terminal");
        terminal
            .draw(|f| w.render(f.area(), f.buffer_mut()))
            .expect("draw");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn renders_truncated() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );

        // Render into a fixed-size test terminal and snapshot the backend.
        let mut terminal = Terminal::new(TestBackend::new(20, 2)).expect("terminal");
        terminal
            .draw(|f| w.render(f.area(), f.buffer_mut()))
            .expect("draw");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn renders_wrapped_details_panama_two_lines() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        w.update_details(
            Some("A man a plan a canal panama".to_string()),
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );
        w.set_interrupt_hint_visible(/*visible*/ false);

        // Freeze time-dependent rendering (elapsed + spinner) to keep the snapshot stable.
        w.is_paused = true;
        w.elapsed_running = Duration::ZERO;

        // Prefix is 4 columns, so a width of 30 yields a content width of 26: one column
        // short of fitting the whole phrase (27 cols), forcing exactly one wrap without ellipsis.
        let mut terminal = Terminal::new(TestBackend::new(30, 3)).expect("terminal");
        terminal
            .draw(|f| w.render(f.area(), f.buffer_mut()))
            .expect("draw");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn timer_pauses_when_requested() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut widget = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );

        let baseline = Instant::now();
        widget.last_resume_at = baseline;

        let before_pause = widget.elapsed_seconds_at(baseline + Duration::from_secs(5));
        assert_eq!(before_pause, 5);

        widget.pause_timer_at(baseline + Duration::from_secs(5));
        let paused_elapsed = widget.elapsed_seconds_at(baseline + Duration::from_secs(10));
        assert_eq!(paused_elapsed, before_pause);

        widget.resume_timer_at(baseline + Duration::from_secs(10));
        let after_resume = widget.elapsed_seconds_at(baseline + Duration::from_secs(13));
        assert_eq!(after_resume, before_pause + 3);
    }

    #[test]
    fn details_overflow_adds_ellipsis() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        w.update_details(
            Some("abcd abcd abcd abcd".to_string()),
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );

        let lines = w.wrapped_details_lines(/*width*/ 6);
        assert_eq!(lines.len(), STATUS_DETAILS_DEFAULT_MAX_LINES);
        let last = lines.last().expect("expected last details line");
        assert!(
            last.spans[1].content.as_ref().ends_with("…"),
            "expected ellipsis in last line: {last:?}"
        );
    }

    #[test]
    fn details_args_can_disable_capitalization_and_limit_lines() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        w.update_details(
            Some("cargo test -p praxis-core and then cargo test -p praxis-tui".to_string()),
            StatusDetailsCapitalization::Preserve,
            /*max_lines*/ 1,
        );

        assert_eq!(
            w.details(),
            Some("cargo test -p praxis-core and then cargo test -p praxis-tui")
        );

        let lines = w.wrapped_details_lines(/*width*/ 24);
        assert_eq!(lines.len(), 1);
        let last = lines.last().expect("expected one details line");
        assert!(
            last.spans
                .last()
                .is_some_and(|span| span.content.as_ref().contains('…')),
            "expected one-line details to be ellipsized, got {last:?}"
        );
    }

    #[test]
    fn footer_message_renders_each_line_independently() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(
            tx,
            crate::tui::FrameRequester::test_dummy(),
            /*animations_enabled*/ true,
        );
        w.update_footer_message(Some(
            "Target: 2,400 / 8,000 (30%)\nUp next: Review diff".to_string(),
        ));

        let rendered = w
            .wrapped_details_lines(/*width*/ 80)
            .into_iter()
            .map(|line| line_text(&line))
            .collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec![
                "  · Target: 2,400 / 8,000 (30%)".to_string(),
                "  · Up next: Review diff".to_string(),
            ]
        );
    }
}
