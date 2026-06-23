use super::*;

#[test]
fn ui_snapshots() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut terminal = match Terminal::new(TestBackend::new(100, 10)) {
        Ok(t) => t,
        Err(e) => panic!("Failed to create terminal: {e}"),
    };

    let test_cases = vec![
        ("empty", None),
        ("small", Some("short".to_string())),
        ("large", Some("z".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5))),
        ("multiple_pastes", None),
        ("backspace_after_pastes", None),
    ];

    for (name, input) in test_cases {
        // Create a fresh composer for each test case
        let mut composer = ChatComposer::new(
            /*has_input_focus*/ true,
            sender.clone(),
            /*enhanced_keys_supported*/ false,
            "Ask Praxis to do anything".to_string(),
            /*disable_paste_burst*/ false,
        );

        if let Some(text) = input {
            composer.handle_paste(text);
        } else if name == "multiple_pastes" {
            // First large paste
            composer.handle_paste("x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 3));
            // Second large paste
            composer.handle_paste("y".repeat(LARGE_PASTE_CHAR_THRESHOLD + 7));
            // Small paste
            composer.handle_paste(" another short paste".to_string());
        } else if name == "backspace_after_pastes" {
            // Three large pastes
            composer.handle_paste("a".repeat(LARGE_PASTE_CHAR_THRESHOLD + 2));
            composer.handle_paste("b".repeat(LARGE_PASTE_CHAR_THRESHOLD + 4));
            composer.handle_paste("c".repeat(LARGE_PASTE_CHAR_THRESHOLD + 6));
            // Move cursor to end and press backspace
            composer.textarea.set_cursor(composer.textarea.text().len());
            composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }

        terminal
            .draw(|f| composer.render(f.area(), f.buffer_mut()))
            .unwrap_or_else(|e| panic!("Failed to draw {name} composer: {e}"));

        insta::assert_snapshot!(name, terminal.backend());
    }
}

#[test]
fn image_placeholder_snapshots() {
    snapshot_composer_state(
        "image_placeholder_single",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.attach_image(PathBuf::from("/tmp/image1.png"));
        },
    );

    snapshot_composer_state(
        "image_placeholder_multiple",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.attach_image(PathBuf::from("/tmp/image1.png"));
            composer.attach_image(PathBuf::from("/tmp/image2.png"));
        },
    );
}

#[test]
fn remote_image_rows_snapshots() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    snapshot_composer_state(
        "remote_image_rows",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.set_remote_image_urls(vec![
                "https://example.com/one.png".to_string(),
                "https://example.com/two.png".to_string(),
            ]);
            composer.set_text_content("describe these".to_string(), Vec::new(), Vec::new());
        },
    );

    snapshot_composer_state(
        "remote_image_rows_selected",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.set_remote_image_urls(vec![
                "https://example.com/one.png".to_string(),
                "https://example.com/two.png".to_string(),
            ]);
            composer.set_text_content("describe these".to_string(), Vec::new(), Vec::new());
            composer.textarea.set_cursor(/*pos*/ 0);
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        },
    );

    snapshot_composer_state(
        "remote_image_rows_after_delete_first",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.set_remote_image_urls(vec![
                "https://example.com/one.png".to_string(),
                "https://example.com/two.png".to_string(),
            ]);
            composer.set_text_content("describe these".to_string(), Vec::new(), Vec::new());
            composer.textarea.set_cursor(/*pos*/ 0);
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        },
    );
}

#[test]
fn slash_popup_model_first_for_mo_ui() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);

    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Type "/mo" humanlike so paste-burst doesn’t interfere.
    type_chars_humanlike(&mut composer, &['/', 'm', 'o']);

    let mut terminal = match Terminal::new(TestBackend::new(60, 5)) {
        Ok(t) => t,
        Err(e) => panic!("Failed to create terminal: {e}"),
    };
    terminal
        .draw(|f| composer.render(f.area(), f.buffer_mut()))
        .unwrap_or_else(|e| panic!("Failed to draw composer: {e}"));

    // Visual snapshot should show the slash popup with /model as the first entry.
    insta::assert_snapshot!("slash_popup_mo", terminal.backend());
}

#[test]
fn slash_popup_model_first_for_mo_logic() {
    use super::super::command_popup::CommandItem;
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    type_chars_humanlike(&mut composer, &['/', 'm', 'o']);

    match &composer.active_popup {
        ActivePopup::Command(popup) => match popup.selected_item() {
            Some(CommandItem::Builtin(cmd)) => {
                assert_eq!(cmd.command(), "model")
            }
            None => panic!("no selected command for '/mo'"),
        },
        _ => panic!("slash popup not active after typing '/mo'"),
    }
}

#[test]
fn slash_popup_resume_for_res_ui() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);

    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Type "/res" humanlike so paste-burst doesn’t interfere.
    type_chars_humanlike(&mut composer, &['/', 'r', 'e', 's']);

    let mut terminal = Terminal::new(TestBackend::new(60, 6)).expect("terminal");
    terminal
        .draw(|f| composer.render(f.area(), f.buffer_mut()))
        .expect("draw composer");

    // Snapshot should show /resume as the first entry for /res.
    insta::assert_snapshot!("slash_popup_res", terminal.backend());
}

#[test]
fn slash_popup_resume_for_res_logic() {
    use super::super::command_popup::CommandItem;
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    type_chars_humanlike(&mut composer, &['/', 'r', 'e', 's']);

    match &composer.active_popup {
        ActivePopup::Command(popup) => match popup.selected_item() {
            Some(CommandItem::Builtin(cmd)) => {
                assert_eq!(cmd.command(), "resume")
            }
            None => panic!("no selected command for '/res'"),
        },
        _ => panic!("slash popup not active after typing '/res'"),
    }
}
