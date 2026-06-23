use super::*;

#[test]
fn footer_hint_row_is_separated_from_composer() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let area = Rect::new(0, 0, 40, 6);
    let mut buf = Buffer::empty(area);
    composer.render(area, &mut buf);

    let row_to_string = |y: u16| {
        let mut row = String::new();
        for x in 0..area.width {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        row
    };

    let mut hint_row: Option<(u16, String)> = None;
    for y in 0..area.height {
        let row = row_to_string(y);
        if row.contains("? shortcuts") {
            hint_row = Some((y, row));
            break;
        }
    }

    let (hint_row_idx, hint_row_contents) =
        hint_row.expect("expected footer hint row to be rendered");
    assert_eq!(
        hint_row_idx,
        area.height - 1,
        "hint row should occupy the bottom line: {hint_row_contents:?}",
    );

    assert!(
        hint_row_idx > 0,
        "expected a spacing row above the footer hints",
    );

    let spacing_row = row_to_string(hint_row_idx - 1);
    assert_eq!(
        spacing_row.trim(),
        "",
        "expected blank spacing row above hints but saw: {spacing_row:?}",
    );
}

#[test]
fn footer_flash_overrides_footer_hint_override() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_footer_hint_override(Some(vec![("K".to_string(), "label".to_string())]));
    composer.show_footer_flash(Line::from("FLASH"), Duration::from_secs(10));

    let area = Rect::new(0, 0, 60, 6);
    let mut buf = Buffer::empty(area);
    composer.render(area, &mut buf);

    let mut bottom_row = String::new();
    for x in 0..area.width {
        bottom_row.push(
            buf[(x, area.height - 1)]
                .symbol()
                .chars()
                .next()
                .unwrap_or(' '),
        );
    }
    assert!(
        bottom_row.contains("FLASH"),
        "expected flash content to render in footer row, saw: {bottom_row:?}",
    );
    assert!(
        !bottom_row.contains("K label"),
        "expected flash to override hint override, saw: {bottom_row:?}",
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn remove_recording_meter_placeholder_clears_placeholder_text() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let id = composer.insert_recording_meter_placeholder("⠤⠤⠤⠤");
    composer.remove_recording_meter_placeholder(&id);

    assert_eq!(composer.textarea.text(), "");
    assert!(composer.textarea.named_element_range(&id).is_none());
}

#[test]
fn footer_flash_expires_and_falls_back_to_hint_override() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_footer_hint_override(Some(vec![("K".to_string(), "label".to_string())]));
    composer.show_footer_flash(Line::from("FLASH"), Duration::from_secs(10));
    composer.footer_flash.as_mut().unwrap().expires_at = Instant::now() - Duration::from_secs(1);

    let area = Rect::new(0, 0, 60, 6);
    let mut buf = Buffer::empty(area);
    composer.render(area, &mut buf);

    let mut bottom_row = String::new();
    for x in 0..area.width {
        bottom_row.push(
            buf[(x, area.height - 1)]
                .symbol()
                .chars()
                .next()
                .unwrap_or(' '),
        );
    }
    assert!(
        bottom_row.contains("K label"),
        "expected hint override to render after flash expired, saw: {bottom_row:?}",
    );
    assert!(
        !bottom_row.contains("FLASH"),
        "expected expired flash to be hidden, saw: {bottom_row:?}",
    );
}

fn snapshot_composer_state_with_width<F>(
    name: &str,
    width: u16,
    enhanced_keys_supported: bool,
    setup: F,
) where
    F: FnOnce(&mut ChatComposer),
{
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        enhanced_keys_supported,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    setup(&mut composer);
    let footer_props = composer.footer_props();
    let footer_lines = footer_height(&footer_props);
    let footer_spacing = ChatComposer::footer_spacing(footer_lines);
    let height = footer_lines + footer_spacing + 8;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| composer.render(f.area(), f.buffer_mut()))
        .unwrap();
    insta::assert_snapshot!(name, terminal.backend());
}

fn snapshot_composer_state<F>(name: &str, enhanced_keys_supported: bool, setup: F)
where
    F: FnOnce(&mut ChatComposer),
{
    snapshot_composer_state_with_width(name, /*width*/ 100, enhanced_keys_supported, setup);
}

#[test]
fn footer_mode_snapshots() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    snapshot_composer_state(
        "footer_mode_shortcut_overlay",
        /*enhanced_keys_supported*/ true,
        |composer| {
            composer.set_esc_backtrack_hint(/*show*/ true);
            let _ =
                composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        },
    );

    snapshot_composer_state(
        "footer_mode_ctrl_c_quit",
        /*enhanced_keys_supported*/ true,
        |composer| {
            composer.show_quit_shortcut_hint(
                key_hint::ctrl(KeyCode::Char('c')),
                /*has_focus*/ true,
            );
        },
    );

    snapshot_composer_state(
        "footer_mode_ctrl_c_interrupt",
        /*enhanced_keys_supported*/ true,
        |composer| {
            composer.set_task_running(/*running*/ true);
            composer.show_quit_shortcut_hint(
                key_hint::ctrl(KeyCode::Char('c')),
                /*has_focus*/ true,
            );
        },
    );

    snapshot_composer_state(
        "footer_mode_ctrl_c_then_esc_hint",
        /*enhanced_keys_supported*/ true,
        |composer| {
            composer.show_quit_shortcut_hint(
                key_hint::ctrl(KeyCode::Char('c')),
                /*has_focus*/ true,
            );
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        },
    );

    snapshot_composer_state(
        "footer_mode_esc_hint_from_overlay",
        /*enhanced_keys_supported*/ true,
        |composer| {
            let _ =
                composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        },
    );

    snapshot_composer_state(
        "footer_mode_esc_hint_backtrack",
        /*enhanced_keys_supported*/ true,
        |composer| {
            composer.set_esc_backtrack_hint(/*show*/ true);
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        },
    );

    snapshot_composer_state(
        "footer_mode_overlay_then_external_esc_hint",
        /*enhanced_keys_supported*/ true,
        |composer| {
            let _ =
                composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
            composer.set_esc_backtrack_hint(/*show*/ true);
        },
    );

    snapshot_composer_state(
        "footer_mode_hidden_while_typing",
        /*enhanced_keys_supported*/ true,
        |composer| {
            type_chars_humanlike(composer, &['h']);
        },
    );
}

#[test]
fn footer_collapse_snapshots() {
    fn setup_collab_footer(
        composer: &mut ChatComposer,
        context_percent: i64,
        indicator: Option<CollaborationModeIndicator>,
    ) {
        composer.set_collaboration_modes_enabled(/*enabled*/ true);
        composer.set_collaboration_mode_indicator(indicator);
        composer.set_context_window(Some(context_percent), /*used_tokens*/ None);
    }

    // Empty textarea, agent idle: shortcuts hint can show, and cycle hint is hidden.
    snapshot_composer_state_with_width(
        "footer_collapse_empty_full",
        /*width*/ 120,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 100, /*indicator*/ None,
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_empty_mode_cycle_with_context",
        /*width*/ 60,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 100, /*indicator*/ None,
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_empty_mode_cycle_without_context",
        /*width*/ 44,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 100, /*indicator*/ None,
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_empty_mode_only",
        /*width*/ 26,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 100, /*indicator*/ None,
            );
        },
    );

    // Empty textarea, plan mode idle: shortcuts hint and cycle hint are available.
    snapshot_composer_state_with_width(
        "footer_collapse_plan_empty_full",
        /*width*/ 120,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 100,
                Some(CollaborationModeIndicator::Plan),
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_empty_mode_cycle_with_context",
        /*width*/ 60,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 100,
                Some(CollaborationModeIndicator::Plan),
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_empty_mode_cycle_without_context",
        /*width*/ 44,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 100,
                Some(CollaborationModeIndicator::Plan),
            );
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_empty_mode_only",
        /*width*/ 26,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 100,
                Some(CollaborationModeIndicator::Plan),
            );
        },
    );

    // Textarea has content, agent running: queue hint is shown.
    snapshot_composer_state_with_width(
        "footer_collapse_queue_full",
        /*width*/ 120,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 98, /*indicator*/ None,
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_queue_short_with_context",
        /*width*/ 50,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 98, /*indicator*/ None,
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_queue_message_without_context",
        /*width*/ 40,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 98, /*indicator*/ None,
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_queue_short_without_context",
        /*width*/ 30,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 98, /*indicator*/ None,
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_queue_mode_only",
        /*width*/ 20,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer, /*context_percent*/ 98, /*indicator*/ None,
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );

    // Textarea has content, plan mode active, agent running: queue hint + mode.
    snapshot_composer_state_with_width(
        "footer_collapse_plan_queue_full",
        /*width*/ 120,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 98,
                Some(CollaborationModeIndicator::Plan),
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_queue_short_with_context",
        /*width*/ 50,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 98,
                Some(CollaborationModeIndicator::Plan),
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_queue_message_without_context",
        /*width*/ 40,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 98,
                Some(CollaborationModeIndicator::Plan),
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_queue_short_without_context",
        /*width*/ 30,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 98,
                Some(CollaborationModeIndicator::Plan),
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
    snapshot_composer_state_with_width(
        "footer_collapse_plan_queue_mode_only",
        /*width*/ 20,
        /*enhanced_keys_supported*/ true,
        |composer| {
            setup_collab_footer(
                composer,
                /*context_percent*/ 98,
                Some(CollaborationModeIndicator::Plan),
            );
            composer.set_task_running(/*running*/ true);
            composer.set_text_content("Test".to_string(), Vec::new(), Vec::new());
        },
    );
}

#[test]
fn esc_hint_stays_hidden_with_draft_content() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ true,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    type_chars_humanlike(&mut composer, &['d']);

    assert!(!composer.is_empty());
    assert_eq!(composer.current_text(), "d");
    assert_eq!(composer.footer_mode, FooterMode::ComposerEmpty);
    assert!(matches!(composer.active_popup, ActivePopup::None));

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(composer.footer_mode, FooterMode::ComposerEmpty);
    assert!(!composer.esc_backtrack_hint);
}

#[test]
fn base_footer_mode_tracks_empty_state_after_quit_hint_expires() {
    use crossterm::event::KeyCode;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    type_chars_humanlike(&mut composer, &['d']);
    composer.show_quit_shortcut_hint(key_hint::ctrl(KeyCode::Char('c')), /*has_focus*/ true);
    composer.quit_shortcut_expires_at = Some(Instant::now() - std::time::Duration::from_secs(1));

    assert_eq!(composer.footer_mode(), FooterMode::ComposerHasDraft);

    composer.set_text_content(String::new(), Vec::new(), Vec::new());
    assert_eq!(composer.footer_mode(), FooterMode::ComposerEmpty);
}

#[test]
fn clear_for_ctrl_c_records_cleared_draft() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    composer.set_text_content("draft text".to_string(), Vec::new(), Vec::new());
    assert_eq!(composer.clear_for_ctrl_c(), Some("draft text".to_string()));
    assert!(composer.is_empty());

    assert_eq!(
        composer.history.navigate_up(&composer.app_event_tx),
        Some(HistoryEntry::new("draft text".to_string()))
    );
}

#[test]
fn clear_for_ctrl_c_preserves_pending_paste_history_entry() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let large = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5);
    composer.handle_paste(large.clone());
    let char_count = large.chars().count();
    let placeholder = format!("[Pasted Content {char_count} chars]");
    assert_eq!(composer.textarea.text(), placeholder);
    assert_eq!(
        composer.pending_pastes,
        vec![(placeholder.clone(), large.clone())]
    );

    composer.clear_for_ctrl_c();
    assert!(composer.is_empty());

    let history_entry = composer
        .history
        .navigate_up(&composer.app_event_tx)
        .expect("expected history entry");
    let text_elements = vec![TextElement::new(
        (0..placeholder.len()).into(),
        Some(placeholder.clone()),
    )];
    assert_eq!(
        history_entry,
        HistoryEntry::with_pending(
            placeholder.clone(),
            text_elements,
            Vec::new(),
            vec![(placeholder.clone(), large.clone())]
        )
    );

    composer.apply_history_entry(history_entry);
    assert_eq!(composer.textarea.text(), placeholder);
    assert_eq!(composer.pending_pastes, vec![(placeholder.clone(), large)]);
    assert_eq!(composer.textarea.element_payloads(), vec![placeholder]);

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted {
            text,
            text_elements,
        } => {
            assert_eq!(text, "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5));
            assert!(text_elements.is_empty());
        }
        _ => panic!("expected Submitted"),
    }
}

#[test]
fn clear_for_ctrl_c_preserves_image_draft_state() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let path = PathBuf::from("example.png");
    composer.attach_image(path.clone());
    let placeholder = local_image_label_text(/*label_number*/ 1);

    composer.clear_for_ctrl_c();
    assert!(composer.is_empty());

    let history_entry = composer
        .history
        .navigate_up(&composer.app_event_tx)
        .expect("expected history entry");
    let text_elements = vec![TextElement::new(
        (0..placeholder.len()).into(),
        Some(placeholder.clone()),
    )];
    assert_eq!(
        history_entry,
        HistoryEntry::with_pending(
            placeholder.clone(),
            text_elements,
            vec![path.clone()],
            Vec::new()
        )
    );

    composer.apply_history_entry(history_entry);
    assert_eq!(composer.textarea.text(), placeholder);
    assert_eq!(composer.local_image_paths(), vec![path]);
    assert_eq!(composer.textarea.element_payloads(), vec![placeholder]);
}

#[test]
fn clear_for_ctrl_c_preserves_remote_offset_image_labels() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    let remote_image_url = "https://example.com/one.png".to_string();
    composer.set_remote_image_urls(vec![remote_image_url.clone()]);
    let text = "[Image #2] draft".to_string();
    let text_elements = vec![TextElement::new(
        (0.."[Image #2]".len()).into(),
        Some("[Image #2]".to_string()),
    )];
    let local_image_path = PathBuf::from("/tmp/local-draft.png");
    composer.set_text_content(text, text_elements, vec![local_image_path.clone()]);
    let expected_text = composer.current_text();
    let expected_elements = composer.text_elements();
    assert_eq!(expected_text, "[Image #2] draft");
    assert_eq!(
        expected_elements[0].placeholder(&expected_text),
        Some("[Image #2]")
    );

    assert_eq!(composer.clear_for_ctrl_c(), Some(expected_text.clone()));

    assert_eq!(
        composer.history.navigate_up(&composer.app_event_tx),
        Some(HistoryEntry::with_pending_and_remote(
            expected_text,
            expected_elements,
            vec![local_image_path],
            Vec::new(),
            vec![remote_image_url],
        ))
    );
}

#[test]
fn apply_history_entry_preserves_local_placeholders_after_remote_prefix() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let remote_image_url = "https://example.com/one.png".to_string();
    let local_image_path = PathBuf::from("/tmp/local-draft.png");
    composer.apply_history_entry(HistoryEntry::with_pending_and_remote(
        "[Image #2] draft".to_string(),
        vec![TextElement::new(
            (0.."[Image #2]".len()).into(),
            Some("[Image #2]".to_string()),
        )],
        vec![local_image_path.clone()],
        Vec::new(),
        vec![remote_image_url.clone()],
    ));

    let restored_text = composer.current_text();
    assert_eq!(restored_text, "[Image #2] draft");
    let restored_elements = composer.text_elements();
    assert_eq!(restored_elements.len(), 1);
    assert_eq!(
        restored_elements[0].placeholder(&restored_text),
        Some("[Image #2]")
    );
    assert_eq!(composer.local_image_paths(), vec![local_image_path]);
    assert_eq!(composer.remote_image_urls(), vec![remote_image_url]);
}
