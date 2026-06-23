use super::*;
use crate::app_event::AppEvent;
use crate::render::renderable::Renderable;
use crate::status_indicator_widget::STATUS_DETAILS_DEFAULT_MAX_LINES;
use crate::status_indicator_widget::StatusDetailsCapitalization;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use insta::assert_snapshot;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SkillScope;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use tokio::sync::mpsc::unbounded_channel;

fn snapshot_buffer(buf: &Buffer) -> String {
    let mut lines = Vec::new();
    for y in 0..buf.area().height {
        let mut row = String::new();
        for x in 0..buf.area().width {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        lines.push(row);
    }
    lines.join("\n")
}

fn render_snapshot(pane: &BottomPane, area: Rect) -> String {
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);
    snapshot_buffer(&buf)
}

fn exec_request() -> ApprovalRequest {
    ApprovalRequest::Exec {
        thread_id: praxis_protocol::ThreadId::new(),
        thread_label: None,
        id: "1".to_string(),
        command: vec!["echo".into(), "ok".into()],
        reason: None,
        available_decisions: vec![
            praxis_protocol::protocol::ReviewDecision::Approved,
            praxis_protocol::protocol::ReviewDecision::Abort,
        ],
        network_approval_context: None,
        additional_permissions: None,
    }
}

#[test]
fn ctrl_c_on_modal_consumes_without_showing_quit_hint() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let features = Features::with_defaults();
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: true,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });
    pane.push_approval_request(exec_request(), &features);
    assert_eq!(CancellationEvent::Handled, pane.on_ctrl_c());
    assert!(!pane.quit_shortcut_hint_visible());
    assert_eq!(CancellationEvent::NotHandled, pane.on_ctrl_c());
}

// live ring removed; related tests deleted.

#[test]
fn overlay_not_shown_above_approval_modal() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let features = Features::with_defaults();
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    // Create an approval modal (active view).
    pane.push_approval_request(exec_request(), &features);

    // Render and verify the top row does not include an overlay.
    let area = Rect::new(0, 0, 60, 6);
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);

    let mut r0 = String::new();
    for x in 0..area.width {
        r0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
    }
    assert!(
        !r0.contains("Turn running"),
        "overlay should not render above modal"
    );
}

#[test]
fn composer_shown_after_denied_while_task_running() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let features = Features::with_defaults();
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    // Start a running task so the status indicator is active above the composer.
    pane.set_task_running(/*running*/ true);

    // Push an approval modal (e.g., command approval) which should hide the status view.
    pane.push_approval_request(exec_request(), &features);

    // Simulate pressing 'n' (No) on the modal.
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    pane.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

    // After denial, since the task is still running, the status indicator should be
    // visible above the composer. The modal should be gone.
    assert!(
        pane.view_stack.is_empty(),
        "no active modal view after denial"
    );

    // Render and ensure the top row includes the running-turn header and a composer line below.
    // Give the animation thread a moment to tick.
    std::thread::sleep(Duration::from_millis(120));
    let area = Rect::new(0, 0, 40, 6);
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);
    let mut row0 = String::new();
    for x in 0..area.width {
        row0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
    }
    assert!(
        row0.contains("Turn running"),
        "expected running-turn header after denial on row 0: {row0:?}"
    );

    // Composer placeholder should be visible somewhere below.
    let mut found_composer = false;
    for y in 1..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        if row.contains("Ask Praxis") {
            found_composer = true;
            break;
        }
    }
    assert!(
        found_composer,
        "expected composer visible under status line"
    );
}

#[test]
fn status_indicator_visible_during_command_execution() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    // Begin a task: show initial status.
    pane.set_task_running(/*running*/ true);

    // Use a height that allows the status line to be visible above the composer.
    let area = Rect::new(0, 0, 40, 6);
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);

    let bufs = snapshot_buffer(&buf);
    assert!(
        bufs.contains("• Turn running"),
        "expected running-turn header"
    );
}

#[test]
fn status_and_composer_fill_height_without_bottom_padding() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    // Activate spinner (status view replaces composer) with no live ring.
    pane.set_task_running(/*running*/ true);

    // Use height == desired_height; expect spacer + status + composer rows without trailing padding.
    let height = pane.desired_height(/*width*/ 30);
    assert!(
        height >= 3,
        "expected at least 3 rows to render spacer, status, and composer; got {height}"
    );
    let area = Rect::new(0, 0, 30, height);
    assert_snapshot!(
        "status_and_composer_fill_height_without_bottom_padding",
        render_snapshot(&pane, area)
    );
}

#[test]
fn status_only_snapshot() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);

    let width = 48;
    let height = pane.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    assert_snapshot!("status_only_snapshot", render_snapshot(&pane, area));
}

#[test]
fn unified_exec_summary_does_not_increase_height_when_status_visible() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);
    let width = 120;
    let before = pane.desired_height(width);

    pane.set_unified_exec_processes(vec!["sleep 5".to_string()]);
    let after = pane.desired_height(width);

    assert_eq!(after, before);

    let area = Rect::new(0, 0, width, after);
    let rendered = render_snapshot(&pane, area);
    assert!(rendered.contains("background terminal running · /ps to view"));
}

#[test]
fn status_with_details_and_queued_messages_snapshot() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);
    pane.update_status(
        "Turn running".to_string(),
        Some("First detail line\nSecond detail line".to_string()),
        StatusDetailsCapitalization::CapitalizeFirst,
        STATUS_DETAILS_DEFAULT_MAX_LINES,
    );
    pane.set_pending_input_preview(
        vec!["Queued follow-up question".to_string()],
        Vec::new(),
        Vec::new(),
    );

    let width = 48;
    let height = pane.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    assert_snapshot!(
        "status_with_details_and_queued_messages_snapshot",
        render_snapshot(&pane, area)
    );
}

#[test]
fn queued_messages_visible_when_status_hidden_snapshot() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);
    pane.set_pending_input_preview(
        vec!["Queued follow-up question".to_string()],
        Vec::new(),
        Vec::new(),
    );
    pane.hide_status_indicator();

    let width = 48;
    let height = pane.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    assert_snapshot!(
        "queued_messages_visible_when_status_hidden_snapshot",
        render_snapshot(&pane, area)
    );
}

#[test]
fn status_and_queued_messages_snapshot() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);
    pane.set_pending_input_preview(
        vec!["Queued follow-up question".to_string()],
        Vec::new(),
        Vec::new(),
    );

    let width = 48;
    let height = pane.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    assert_snapshot!(
        "status_and_queued_messages_snapshot",
        render_snapshot(&pane, area)
    );
}

#[test]
fn remote_images_render_above_composer_text() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_remote_image_urls(vec![
        "https://example.com/one.png".to_string(),
        "data:image/png;base64,aGVsbG8=".to_string(),
    ]);

    assert_eq!(pane.composer_text(), "");
    let width = 48;
    let height = pane.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    let snapshot = render_snapshot(&pane, area);
    assert!(snapshot.contains("[Image #1]"));
    assert!(snapshot.contains("[Image #2]"));
}

#[test]
fn drain_pending_submission_state_clears_remote_image_urls() {
    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_remote_image_urls(vec!["https://example.com/one.png".to_string()]);
    assert_eq!(pane.remote_image_urls().len(), 1);

    pane.drain_pending_submission_state();

    assert!(pane.remote_image_urls().is_empty());
}

#[test]
fn esc_with_skill_popup_does_not_interrupt_task() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(vec![SkillMetadata {
            name: "test-skill".to_string(),
            description: "test skill".to_string(),
            short_description: None,
            interface: None,
            dependencies: None,
            policy: None,
            path_to_skills_md: PathBuf::from("test-skill"),
            scope: SkillScope::User,
        }]),
    });

    pane.set_task_running(/*running*/ true);

    // Repro: a running task + skill popup + Esc should dismiss the popup, not interrupt.
    pane.insert_str("$");
    assert!(
        pane.composer.popup_active(),
        "expected skill popup after typing `$`"
    );

    pane.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    while let Ok(ev) = rx.try_recv() {
        assert!(
            !matches!(ev, AppEvent::AgentOp(Op::Interrupt)),
            "expected Esc to not send Op::Interrupt when dismissing skill popup"
        );
    }
    assert!(
        !pane.composer.popup_active(),
        "expected Esc to dismiss skill popup"
    );
}

#[test]
fn esc_with_slash_command_popup_does_not_interrupt_task() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);

    // Repro: a running task + slash-command popup + Esc should not interrupt the task.
    pane.insert_str("/");
    assert!(
        pane.composer.popup_active(),
        "expected command popup after typing `/`"
    );

    pane.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    while let Ok(ev) = rx.try_recv() {
        assert!(
            !matches!(ev, AppEvent::AgentOp(Op::Interrupt)),
            "expected Esc to not send Op::Interrupt while command popup is active"
        );
    }
    assert_eq!(pane.composer_text(), "/");
}

#[test]
fn esc_with_agent_command_without_popup_does_not_interrupt_task() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);

    // Repro: `/agent ` hides the popup (cursor past command name). Esc should
    // keep editing command text instead of interrupting the running task.
    pane.insert_str("/agent ");
    assert!(
        !pane.composer.popup_active(),
        "expected command popup to be hidden after entering `/agent `"
    );

    pane.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    while let Ok(ev) = rx.try_recv() {
        assert!(
            !matches!(ev, AppEvent::AgentOp(Op::Interrupt)),
            "expected Esc to not send Op::Interrupt while typing `/agent`"
        );
    }
    assert_eq!(pane.composer_text(), "/agent ");
}

#[test]
fn esc_release_after_dismissing_agent_picker_does_not_interrupt_task() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);
    pane.show_selection_view(SelectionViewParams {
        title: Some("Agents".to_string()),
        items: vec![SelectionItem {
            name: "Main".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });

    pane.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Esc,
        KeyModifiers::NONE,
        KeyEventKind::Press,
    ));
    pane.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Esc,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));

    while let Ok(ev) = rx.try_recv() {
        assert!(
            !matches!(ev, AppEvent::AgentOp(Op::Interrupt)),
            "expected Esc release after dismissing agent picker to not interrupt"
        );
    }
    assert!(
        pane.no_modal_or_popup_active(),
        "expected Esc press to dismiss the agent picker"
    );
}

#[test]
fn esc_interrupts_running_task_when_no_popup() {
    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_task_running(/*running*/ true);

    pane.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(
        matches!(rx.try_recv(), Ok(AppEvent::AgentOp(Op::Interrupt))),
        "expected Esc to send Op::Interrupt while a task is running"
    );
}

#[test]
fn esc_routes_to_handle_key_event_when_requested() {
    #[derive(Default)]
    struct EscRoutingView {
        on_ctrl_c_calls: Rc<Cell<usize>>,
        handle_calls: Rc<Cell<usize>>,
    }

    impl Renderable for EscRoutingView {
        fn render(&self, _area: Rect, _buf: &mut Buffer) {}

        fn desired_height(&self, _width: u16) -> u16 {
            0
        }
    }

    impl BottomPaneView for EscRoutingView {
        fn handle_key_event(&mut self, _key_event: KeyEvent) {
            self.handle_calls
                .set(self.handle_calls.get().saturating_add(1));
        }

        fn on_ctrl_c(&mut self) -> CancellationEvent {
            self.on_ctrl_c_calls
                .set(self.on_ctrl_c_calls.get().saturating_add(1));
            CancellationEvent::Handled
        }

        fn prefer_esc_to_handle_key_event(&self) -> bool {
            true
        }
    }

    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    let on_ctrl_c_calls = Rc::new(Cell::new(0));
    let handle_calls = Rc::new(Cell::new(0));
    pane.push_view(Box::new(EscRoutingView {
        on_ctrl_c_calls: Rc::clone(&on_ctrl_c_calls),
        handle_calls: Rc::clone(&handle_calls),
    }));

    pane.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(on_ctrl_c_calls.get(), 0);
    assert_eq!(handle_calls.get(), 1);
}

#[test]
fn release_events_are_ignored_for_active_view() {
    #[derive(Default)]
    struct CountingView {
        handle_calls: Rc<Cell<usize>>,
    }

    impl Renderable for CountingView {
        fn render(&self, _area: Rect, _buf: &mut Buffer) {}

        fn desired_height(&self, _width: u16) -> u16 {
            0
        }
    }

    impl BottomPaneView for CountingView {
        fn handle_key_event(&mut self, _key_event: KeyEvent) {
            self.handle_calls
                .set(self.handle_calls.get().saturating_add(1));
        }
    }

    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    let handle_calls = Rc::new(Cell::new(0));
    pane.push_view(Box::new(CountingView {
        handle_calls: Rc::clone(&handle_calls),
    }));

    pane.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Press,
    ));
    pane.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));

    assert_eq!(handle_calls.get(), 1);
}

#[test]
fn paste_completion_clears_stacked_views_and_restores_composer_input() {
    #[derive(Default)]
    struct BlockingView {
        handle_calls: Rc<Cell<usize>>,
    }

    impl Renderable for BlockingView {
        fn render(&self, _area: Rect, _buf: &mut Buffer) {}

        fn desired_height(&self, _width: u16) -> u16 {
            0
        }
    }

    impl BottomPaneView for BlockingView {
        fn handle_key_event(&mut self, _key_event: KeyEvent) {
            self.handle_calls
                .set(self.handle_calls.get().saturating_add(1));
        }
    }

    #[derive(Default)]
    struct PasteCompletesView {
        complete: bool,
    }

    impl Renderable for PasteCompletesView {
        fn render(&self, _area: Rect, _buf: &mut Buffer) {}

        fn desired_height(&self, _width: u16) -> u16 {
            0
        }
    }

    impl BottomPaneView for PasteCompletesView {
        fn handle_paste(&mut self, _pasted: String) -> bool {
            self.complete = true;
            true
        }

        fn is_complete(&self) -> bool {
            self.complete
        }
    }

    let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let mut pane = BottomPane::new(BottomPaneParams {
        app_event_tx: tx,
        frame_requester: FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: "Ask Praxis to do anything".to_string(),
        disable_paste_burst: false,
        animations_enabled: true,
        skills: Some(Vec::new()),
    });

    pane.set_composer_input_enabled(/*enabled*/ false, /*placeholder*/ None);

    let lower_view_handle_calls = Rc::new(Cell::new(0));
    pane.push_view(Box::new(BlockingView {
        handle_calls: Rc::clone(&lower_view_handle_calls),
    }));
    pane.push_view(Box::new(PasteCompletesView::default()));

    pane.handle_paste("hello".to_string());

    assert!(
        pane.view_stack.is_empty(),
        "paste completion should tear down the active modal flow"
    );

    pane.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    let area = Rect::new(0, 0, 40, pane.desired_height(/*width*/ 40).max(2));
    assert!(pane.cursor_pos(area).is_some());
    assert_eq!(lower_view_handle_calls.get(), 0);
}
