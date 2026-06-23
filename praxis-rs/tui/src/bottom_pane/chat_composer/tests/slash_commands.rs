use super::*;

#[test]
fn slash_init_dispatches_command_and_does_not_submit_literal_text() {
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

    // Type the slash command.
    type_chars_humanlike(&mut composer, &['/', 'i', 'n', 'i', 't']);

    // Press Enter to dispatch the selected command.
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // When a slash command is dispatched, the composer should return a
    // Command result (not submit literal text) and clear its textarea.
    match result {
        InputResult::Command(cmd) => {
            assert_eq!(cmd.command(), "init");
        }
        InputResult::CommandWithArgs(_, _, _) => {
            panic!("expected command dispatch without args for '/init'")
        }
        InputResult::Submitted { text, .. } => {
            panic!("expected command dispatch, but composer submitted literal text: {text}")
        }
        InputResult::Queued { .. } => {
            panic!("expected command dispatch, but composer queued literal text")
        }
        InputResult::None => panic!("expected Command result for '/init'"),
    }
    assert!(composer.textarea.is_empty(), "composer should be cleared");
}

#[test]
fn slash_codex_dispatches_thread_command_and_does_not_submit_literal_text() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use praxis_app_core::thread_commands::ExternalThreadCommandAction;

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.textarea.insert_str("/codex fork");

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match result {
        InputResult::ThreadCommand(intent) => {
            assert_eq!(intent.action, ExternalThreadCommandAction::Fork);
        }
        InputResult::Submitted { text, .. } => {
            panic!("expected thread command dispatch, but composer submitted literal text: {text}")
        }
        other => panic!("expected ThreadCommand result for '/codex fork', got {other:?}"),
    }
    assert!(composer.textarea.is_empty(), "composer should be cleared");
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            assert!(
                !cell
                    .display_lines(/*width*/ 80)
                    .into_iter()
                    .map(|line| line.to_string())
                    .any(|line| line.contains("Unrecognized command")),
                "composer emitted an unrecognized-command history cell"
            );
        }
    }
}

#[test]
fn kill_buffer_persists_after_submit() {
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
    composer.textarea.insert_str("restore me");
    composer.textarea.set_cursor(/*pos*/ 0);

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert!(composer.textarea.is_empty());

    composer.textarea.insert_str("hello");
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { .. }));
    assert!(composer.textarea.is_empty());

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(composer.textarea.text(), "restore me");
}

#[test]
fn kill_buffer_persists_after_slash_command_dispatch() {
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
    composer.textarea.insert_str("restore me");
    composer.textarea.set_cursor(/*pos*/ 0);

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert!(composer.textarea.is_empty());

    composer.textarea.insert_str("/diff");
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Command(cmd) => {
            assert_eq!(cmd.command(), "diff");
        }
        _ => panic!("expected Command result for '/diff'"),
    }
    assert!(composer.textarea.is_empty());

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(composer.textarea.text(), "restore me");
}

#[test]
fn slash_command_disabled_while_task_running_keeps_text() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_task_running(/*running*/ true);
    composer
        .textarea
        .set_text_clearing_elements("/review these changes");

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(InputResult::None, result);
    assert_eq!("/review these changes", composer.textarea.text());

    let mut found_error = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            let message = cell
                .display_lines(/*width*/ 80)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(message.contains("disabled while a task is in progress"));
            found_error = true;
            break;
        }
    }
    assert!(found_error, "expected error history cell to be sent");
}

#[test]
fn slash_tab_completion_moves_cursor_to_end() {
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

    type_chars_humanlike(&mut composer, &['/', 'c']);

    let (_result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(composer.textarea.text(), "/compact ");
    assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());
}

#[test]
fn slash_tab_then_enter_dispatches_builtin_command() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Type a prefix and complete with Tab, which inserts a trailing space
    // and moves the cursor beyond the '/name' token (hides the popup).
    type_chars_humanlike(&mut composer, &['/', 'd', 'i']);
    let (_res, _redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "/diff ");

    // Press Enter: should dispatch the command, not submit literal text.
    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Command(cmd) => assert_eq!(cmd.command(), "diff"),
        InputResult::CommandWithArgs(_, _, _) => {
            panic!("expected command dispatch without args for '/diff'")
        }
        InputResult::Submitted { text, .. } => {
            panic!("expected command dispatch after Tab completion, got literal submit: {text}")
        }
        InputResult::Queued { .. } => {
            panic!("expected command dispatch after Tab completion, got literal queue")
        }
        InputResult::None => panic!("expected Command result for '/diff'"),
    }
    assert!(composer.textarea.is_empty());
}

#[test]
fn slash_command_elementizes_on_space() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_collaboration_modes_enabled(/*enabled*/ true);

    type_chars_humanlike(&mut composer, &['/', 'p', 'l', 'a', 'n', ' ']);

    let text = composer.textarea.text().to_string();
    let elements = composer.textarea.text_elements();
    assert_eq!(text, "/plan ");
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].placeholder(&text), Some("/plan"));
}

#[test]
fn slash_command_elementizes_only_known_commands() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_collaboration_modes_enabled(/*enabled*/ true);

    type_chars_humanlike(&mut composer, &['/', 'U', 's', 'e', 'r', 's', ' ']);

    let text = composer.textarea.text().to_string();
    let elements = composer.textarea.text_elements();
    assert_eq!(text, "/Users ");
    assert!(elements.is_empty());
}

#[test]
fn slash_command_element_removed_when_not_at_start() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    type_chars_humanlike(&mut composer, &['/', 'r', 'e', 'v', 'i', 'e', 'w', ' ']);

    let text = composer.textarea.text().to_string();
    let elements = composer.textarea.text_elements();
    assert_eq!(text, "/review ");
    assert_eq!(elements.len(), 1);

    composer.textarea.set_cursor(/*pos*/ 0);
    type_chars_humanlike(&mut composer, &['x']);

    let text = composer.textarea.text().to_string();
    let elements = composer.textarea.text_elements();
    assert_eq!(text, "x/review ");
    assert!(elements.is_empty());
}

#[test]
fn tab_submits_when_no_task_running() {
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

    type_chars_humanlike(&mut composer, &['h', 'i']);

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert!(matches!(
        result,
        InputResult::Submitted { ref text, .. } if text == "hi"
    ));
    assert!(composer.textarea.is_empty());
}

#[test]
fn tab_does_not_submit_for_bang_shell_command() {
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
    composer.set_task_running(/*running*/ false);

    type_chars_humanlike(&mut composer, &['!', 'l', 's']);

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert!(matches!(result, InputResult::None));
    assert!(
        composer.textarea.text().starts_with("!ls"),
        "expected Tab not to submit or clear a `!` command"
    );
}

#[test]
fn slash_mention_dispatches_command_and_inserts_at() {
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

    type_chars_humanlike(&mut composer, &['/', 'm', 'e', 'n', 't', 'i', 'o', 'n']);

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match result {
        InputResult::Command(cmd) => {
            assert_eq!(cmd.command(), "mention");
        }
        InputResult::CommandWithArgs(_, _, _) => {
            panic!("expected command dispatch without args for '/mention'")
        }
        InputResult::Submitted { text, .. } => {
            panic!("expected command dispatch, but composer submitted literal text: {text}")
        }
        InputResult::Queued { .. } => {
            panic!("expected command dispatch, but composer queued literal text")
        }
        InputResult::None => panic!("expected Command result for '/mention'"),
    }
    assert!(composer.textarea.is_empty(), "composer should be cleared");
    composer.insert_str("@");
    assert_eq!(composer.textarea.text(), "@");
}

#[test]
fn slash_plan_args_preserve_text_elements() {
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
    composer.set_collaboration_modes_enabled(/*enabled*/ true);

    type_chars_humanlike(&mut composer, &['/', 'p', 'l', 'a', 'n', ' ']);
    let placeholder = local_image_label_text(/*label_number*/ 1);
    composer.attach_image(PathBuf::from("/tmp/plan.png"));

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match result {
        InputResult::CommandWithArgs(cmd, args, text_elements) => {
            assert_eq!(cmd.command(), "plan");
            assert_eq!(args, placeholder);
            assert_eq!(text_elements.len(), 1);
            assert_eq!(
                text_elements[0].placeholder(&args),
                Some(placeholder.as_str())
            );
        }
        _ => panic!("expected CommandWithArgs for /plan with args"),
    }
}
