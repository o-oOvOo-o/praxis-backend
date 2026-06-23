use super::*;

#[test]
fn slash_path_input_submits_without_command_error() {
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

    composer
        .textarea
        .set_text_clearing_elements("/Users/example/project/src/main.rs");

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    if let InputResult::Submitted { text, .. } = result {
        assert_eq!(text, "/Users/example/project/src/main.rs");
    } else {
        panic!("expected Submitted");
    }
    assert!(composer.textarea.is_empty());
    match rx.try_recv() {
        Ok(event) => panic!("unexpected event: {event:?}"),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        Err(err) => panic!("unexpected channel state: {err:?}"),
    }
}

#[test]
fn slash_with_leading_space_submits_as_text() {
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

    composer
        .textarea
        .set_text_clearing_elements(" /this-looks-like-a-command");

    let (result, _needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    if let InputResult::Submitted { text, .. } = result {
        assert_eq!(text, "/this-looks-like-a-command");
    } else {
        panic!("expected Submitted");
    }
    assert!(composer.textarea.is_empty());
    match rx.try_recv() {
        Ok(event) => panic!("unexpected event: {event:?}"),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        Err(err) => panic!("unexpected channel state: {err:?}"),
    }
}

/// Behavior: the first fast ASCII character is held briefly to avoid flicker; if no burst
/// follows, it should eventually flush as normal typed input (not as a paste).
#[test]
fn pending_first_ascii_char_flushes_as_typed() {
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

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert!(composer.is_in_paste_burst());
    assert!(composer.textarea.text().is_empty());

    std::thread::sleep(ChatComposer::recommended_paste_flush_delay());
    let flushed = composer.flush_paste_burst_if_due();
    assert!(flushed, "expected pending first char to flush");
    assert_eq!(composer.textarea.text(), "h");
    assert!(!composer.is_in_paste_burst());
}

/// Behavior: fast "paste-like" ASCII input should buffer and then flush as a single paste. If
/// the payload is small, it should insert directly (no placeholder).
#[test]
fn burst_paste_fast_small_buffers_and_flushes_on_stop() {
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

    let count = 32;
    let mut now = Instant::now();
    let step = Duration::from_millis(1);
    for _ in 0..count {
        let _ = composer.handle_input_basic_with_time(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            now,
        );
        assert!(
            composer.is_in_paste_burst(),
            "expected active paste burst during fast typing"
        );
        assert!(
            composer.textarea.text().is_empty(),
            "text should not appear during burst"
        );
        now += step;
    }

    assert!(
        composer.textarea.text().is_empty(),
        "text should remain empty until flush"
    );
    let flush_time = now + PasteBurst::recommended_active_flush_delay() + step;
    let flushed = composer.handle_paste_burst_flush(flush_time);
    assert!(flushed, "expected buffered text to flush after stop");
    assert_eq!(composer.textarea.text(), "a".repeat(count));
    assert!(
        composer.pending_pastes.is_empty(),
        "no placeholder for small burst"
    );
}

/// Behavior: fast "paste-like" ASCII input should buffer and then flush as a single paste. If
/// the payload is large, it should insert a placeholder and defer the full text until submit.
#[test]
fn burst_paste_fast_large_inserts_placeholder_on_flush() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let count = LARGE_PASTE_CHAR_THRESHOLD + 1; // > threshold to trigger placeholder
    let mut now = Instant::now();
    let step = Duration::from_millis(1);
    for _ in 0..count {
        let _ = composer.handle_input_basic_with_time(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            now,
        );
        now += step;
    }

    // Nothing should appear until we stop and flush
    assert!(composer.textarea.text().is_empty());
    let flush_time = now + PasteBurst::recommended_active_flush_delay() + step;
    let flushed = composer.handle_paste_burst_flush(flush_time);
    assert!(flushed, "expected flush after stopping fast input");

    let expected_placeholder = format!("[Pasted Content {count} chars]");
    assert_eq!(composer.textarea.text(), expected_placeholder);
    assert_eq!(composer.pending_pastes.len(), 1);
    assert_eq!(composer.pending_pastes[0].0, expected_placeholder);
    assert_eq!(composer.pending_pastes[0].1.len(), count);
    assert!(composer.pending_pastes[0].1.chars().all(|c| c == 'x'));
}

/// Behavior: human-like typing (with delays between chars) should not be classified as a paste
/// burst. Characters should appear immediately and should not trigger a paste placeholder.
#[test]
fn humanlike_typing_1000_chars_appears_live_no_placeholder() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    let count = LARGE_PASTE_CHAR_THRESHOLD; // 1000 in current config
    let chars: Vec<char> = vec!['z'; count];
    type_chars_humanlike(&mut composer, &chars);

    assert_eq!(composer.textarea.text(), "z".repeat(count));
    assert!(composer.pending_pastes.is_empty());
}

#[test]
fn slash_popup_not_activated_for_slash_space_text_history_like_input() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use tokio::sync::mpsc::unbounded_channel;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Simulate history-like content: "/ test"
    composer.set_text_content("/ test".to_string(), Vec::new(), Vec::new());

    // After set_text_content -> sync_popups is called; popup should NOT be Command.
    assert!(
        matches!(composer.active_popup, ActivePopup::None),
        "expected no slash popup for '/ test'"
    );

    // Up should be handled by history navigation path, not slash popup handler.
    let (result, _redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(result, InputResult::None);
}

#[test]
fn slash_popup_activated_for_bare_slash_and_valid_prefixes() {
    // use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tokio::sync::mpsc::unbounded_channel;

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Case 1: bare "/"
    composer.set_text_content("/".to_string(), Vec::new(), Vec::new());
    assert!(
        matches!(composer.active_popup, ActivePopup::Command(_)),
        "bare '/' should activate slash popup"
    );

    // Case 2: valid prefix "/re" (matches /review, /resume, etc.)
    composer.set_text_content("/re".to_string(), Vec::new(), Vec::new());
    assert!(
        matches!(composer.active_popup, ActivePopup::Command(_)),
        "'/re' should activate slash popup via prefix match"
    );

    // Case 3: fuzzy match "/ac" (subsequence of /compact and /feedback)
    composer.set_text_content("/ac".to_string(), Vec::new(), Vec::new());
    assert!(
        matches!(composer.active_popup, ActivePopup::Command(_)),
        "'/ac' should activate slash popup via fuzzy match"
    );

    // Case 4: invalid prefix "/zzz" – still allowed to open popup if it
    // matches no built-in command; our current logic will not open popup.
    // Verify that explicitly.
    composer.set_text_content("/zzz".to_string(), Vec::new(), Vec::new());
    assert!(
        matches!(composer.active_popup, ActivePopup::None),
        "'/zzz' should not activate slash popup because it is not a prefix of any built-in command"
    );
}
