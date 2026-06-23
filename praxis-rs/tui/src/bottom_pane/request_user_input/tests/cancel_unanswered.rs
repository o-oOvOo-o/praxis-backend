use super::*;

#[test]
fn esc_in_notes_mode_without_options_interrupts() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_without_options("q1", "Notes")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    assert_eq!(overlay.done, true);
    expect_interrupt_only(&mut rx);
}

#[test]
fn esc_in_options_mode_interrupts() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    assert_eq!(overlay.done, true);
    expect_interrupt_only(&mut rx);
}

#[test]
fn esc_in_notes_mode_clears_notes_and_hides_ui() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);
    answer.answer_committed = true;

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(overlay.done, false);
    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);
    assert_eq!(overlay.composer.current_text_with_pending(), "");
    assert_eq!(answer.draft.text, "");
    assert_eq!(answer.options_state.selected_idx, Some(0));
    assert_eq!(answer.answer_committed, false);
    assert!(rx.try_recv().is_err());
}

#[test]
fn esc_in_notes_mode_with_text_clears_notes_and_hides_ui() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);
    answer.answer_committed = true;

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(overlay.done, false);
    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);
    assert_eq!(overlay.composer.current_text_with_pending(), "");
    assert_eq!(answer.draft.text, "");
    assert_eq!(answer.options_state.selected_idx, Some(0));
    assert_eq!(answer.answer_committed, false);
    assert!(rx.try_recv().is_err());
}

#[test]
fn esc_drops_committed_answers() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_with_options("q1", "First"),
                question_without_options("q2", "Second"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert!(
        rx.try_recv().is_err(),
        "unexpected AppEvent before interruption"
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    expect_interrupt_only(&mut rx);
}

#[test]
fn backspace_in_options_clears_selection() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(1);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Backspace));

    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, None);
    assert_eq!(overlay.notes_ui_visible(), false);
    assert!(rx.try_recv().is_err());
}

#[test]
fn backspace_on_empty_notes_closes_notes_ui() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    assert!(matches!(overlay.focus, Focus::Notes));
    assert_eq!(overlay.notes_ui_visible(), true);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Backspace));

    let answer = overlay.current_answer().expect("answer missing");
    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);
    assert_eq!(answer.options_state.selected_idx, Some(0));
    assert!(rx.try_recv().is_err());
}

#[test]
fn tab_in_notes_clears_notes_and_hides_ui() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    overlay
        .composer
        .set_text_content("Some notes".to_string(), Vec::new(), Vec::new());

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));

    let answer = overlay.current_answer().expect("answer missing");
    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);
    assert_eq!(overlay.composer.current_text_with_pending(), "");
    assert_eq!(answer.draft.text, "");
    assert_eq!(answer.options_state.selected_idx, Some(0));
    assert!(rx.try_recv().is_err());
}

#[test]
fn skipped_option_questions_count_as_unanswered() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    assert_eq!(overlay.unanswered_count(), 1);
}

#[test]
fn highlighted_option_questions_are_unanswered() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);

    assert_eq!(overlay.unanswered_count(), 1);
}

#[test]
fn freeform_requires_enter_with_text_to_mark_answered() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "Notes"),
                question_without_options("q2", "More"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay
        .composer
        .set_text_content("Draft".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();
    assert_eq!(overlay.unanswered_count(), 2);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_eq!(overlay.answers[0].answer_committed, true);
    assert_eq!(overlay.unanswered_count(), 1);
}

#[test]
fn freeform_enter_with_empty_text_is_unanswered() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "Notes"),
                question_without_options("q2", "More"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_eq!(overlay.answers[0].answer_committed, false);
    assert_eq!(overlay.unanswered_count(), 2);
}

#[test]
fn freeform_questions_submit_empty_when_empty() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_without_options("q1", "Notes")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, Vec::<String>::new());
}

#[test]
fn freeform_draft_is_not_submitted_without_enter() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_without_options("q1", "Notes")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    overlay
        .composer
        .set_text_content("Draft text".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, Vec::<String>::new());
}

#[test]
fn freeform_commit_resets_when_draft_changes() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "Notes"),
                question_without_options("q2", "More"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay
        .composer
        .set_text_content("Committed".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();
    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_eq!(overlay.answers[0].answer_committed, true);
    let _ = rx.try_recv();

    overlay.move_question(/*next*/ false);
    overlay
        .composer
        .set_text_content("Edited".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();
    overlay.move_question(/*next*/ true);
    assert_eq!(overlay.answers[0].answer_committed, false);

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, Vec::<String>::new());
}
