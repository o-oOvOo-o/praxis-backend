use super::*;

#[test]
fn enter_submits_when_file_popup_has_no_selection() {
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

    let input = "npx -y @kaeawc/auto-mobile@latest";
    composer.textarea.insert_str(input);
    composer.textarea.set_cursor(input.len());
    composer.sync_popups();

    assert!(matches!(composer.active_popup, ActivePopup::File(_)));

    let (result, consumed) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(consumed);
    match result {
        InputResult::Submitted { text, .. } => assert_eq!(text, input),
        _ => panic!("expected Submitted"),
    }
}

/// Behavior: if the ASCII path has a pending first char (flicker suppression) and a non-ASCII
/// char arrives next, the pending ASCII char should still be preserved and the overall input
/// should submit normally (i.e. we should not misclassify this as a paste burst).
#[test]
fn ascii_prefix_survives_non_ascii_followup() {
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

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    assert!(composer.is_in_paste_burst());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('あ'), KeyModifiers::NONE));

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match result {
        InputResult::Submitted { text, .. } => assert_eq!(text, "1あ"),
        _ => panic!("expected Submitted"),
    }
}

/// Behavior: a single non-ASCII char should be inserted immediately (IME-friendly) and should
/// not create any paste-burst state.
#[test]
fn non_ascii_char_inserts_immediately_without_burst_state() {
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

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('あ'), KeyModifiers::NONE));

    assert_eq!(composer.textarea.text(), "あ");
    assert!(!composer.is_in_paste_burst());
}

/// Behavior: while we're capturing a paste-like burst, Enter should be treated as a newline
/// within the burst (not as "submit"), and the whole payload should flush as one paste.
#[test]
fn non_ascii_burst_buffers_enter_and_flushes_multiline() {
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

    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('你'), KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('好'), KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

    assert!(composer.textarea.text().is_empty());
    let _ = flush_after_paste_burst(&mut composer);
    assert_eq!(composer.textarea.text(), "你好\nhi");
}

/// Behavior: a paste-like burst may include a full-width/ideographic space (U+3000). It should
/// still be captured as a single paste payload and preserve the exact Unicode content.
#[test]
fn non_ascii_burst_preserves_ideographic_space_and_ascii() {
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

    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    for ch in ['你', '　', '好'] {
        let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for ch in ['h', 'i'] {
        let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    assert!(composer.textarea.text().is_empty());
    let _ = flush_after_paste_burst(&mut composer);
    assert_eq!(composer.textarea.text(), "你　好\nhi");
}

/// Behavior: a large multi-line payload containing both non-ASCII and ASCII (e.g. "UTF-8",
/// "Unicode") should be captured as a single paste-like burst, and Enter key events should
/// become `\n` within the buffered content.
#[test]
fn non_ascii_burst_buffers_large_multiline_mixed_ascii_and_unicode() {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    const LARGE_MIXED_PAYLOAD: &str = "天地玄黄 宇宙洪荒\n\
日月盈昃 辰宿列张\n\
寒来暑往 秋收冬藏\n\
\n\
你好世界 编码测试\n\
汉字处理 UTF-8\n\
终端显示 正确无误\n\
\n\
风吹竹林 月照大江\n\
白云千载 青山依旧\n\
程序员 与 Unicode 同行";

    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );

    // Force an active burst so the test doesn't depend on timing heuristics.
    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    for ch in LARGE_MIXED_PAYLOAD.chars() {
        let code = if ch == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(ch)
        };
        let _ = composer.handle_key_event(KeyEvent::new(code, KeyModifiers::NONE));
    }

    assert!(composer.textarea.text().is_empty());
    let _ = flush_after_paste_burst(&mut composer);
    assert_eq!(composer.textarea.text(), LARGE_MIXED_PAYLOAD);
}

/// Behavior: while a paste-like burst is active, Enter should not submit; it should insert a
/// newline into the buffered payload and flush as a single paste later.
#[test]
fn ascii_burst_treats_enter_as_newline() {
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

    let mut now = Instant::now();
    let step = Duration::from_millis(1);

    let _ = composer
        .handle_input_basic_with_time(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), now);
    now += step;
    let _ = composer
        .handle_input_basic_with_time(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE), now);
    now += step;

    let (result, _) = composer.handle_submission_with_time(/*should_queue*/ false, now);
    assert!(
        matches!(result, InputResult::None),
        "Enter during a burst should insert newline, not submit"
    );

    for ch in ['t', 'h', 'e', 'r', 'e'] {
        now += step;
        let _ = composer.handle_input_basic_with_time(
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }

    assert!(composer.textarea.text().is_empty());
    let flush_time = now + PasteBurst::recommended_active_flush_delay() + step;
    let flushed = composer.handle_paste_burst_flush(flush_time);
    assert!(flushed, "expected paste burst to flush");
    assert_eq!(composer.textarea.text(), "hi\nthere");
}

/// Behavior: even if Enter suppression would normally be active for a burst, Enter should
/// still dispatch a built-in slash command when the first line begins with `/`.
#[test]
fn slash_context_enter_ignores_paste_burst_enter_suppression() {
    use crate::slash_command::SlashCommand;
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

    composer.textarea.set_text_clearing_elements("/diff");
    composer.textarea.set_cursor("/diff".len());
    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Command(SlashCommand::Diff)));
}

/// Behavior: if a burst is buffering text and the user presses a non-char key, flush the
/// buffered burst *before* applying that key so the buffer cannot get stuck.
#[test]
fn non_char_key_flushes_active_burst_before_input() {
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

    // Force an active burst so we can deterministically buffer characters without relying on
    // timing.
    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert!(composer.textarea.text().is_empty());
    assert!(composer.is_in_paste_burst());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "hi");
    assert_eq!(composer.textarea.cursor(), 1);
    assert!(!composer.is_in_paste_burst());
}

/// Behavior: enabling `disable_paste_burst` flushes any held first character (flicker
/// suppression) and then inserts subsequent chars immediately without creating burst state.
#[test]
fn disable_paste_burst_flushes_pending_first_char_and_inserts_immediately() {
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

    // First ASCII char is normally held briefly. Flip the config mid-stream and ensure the
    // held char is not dropped.
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(composer.is_in_paste_burst());
    assert!(composer.textarea.text().is_empty());

    composer.set_disable_paste_burst(/*disabled*/ true);
    assert_eq!(composer.textarea.text(), "a");
    assert!(!composer.is_in_paste_burst());

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(composer.textarea.text(), "ab");
    assert!(!composer.is_in_paste_burst());
}
