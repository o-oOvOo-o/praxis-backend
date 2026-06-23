use super::*;

#[test]
fn notes_are_captured_for_selected_option() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(1);
    }
    overlay.select_current_option(/*committed*/ false);
    overlay
        .composer
        .set_text_content("Notes for option 2".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();
    let draft = overlay.capture_composer_draft();
    if let Some(answer) = overlay.current_answer_mut() {
        answer.draft = draft;
        answer.answer_committed = true;
    }

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(
        answer.answers,
        vec![
            "Option 2".to_string(),
            "user_note: Notes for option 2".to_string(),
        ]
    );
}

#[test]
fn notes_submission_commits_selected_option() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_with_options("q1", "Pick one"),
                question_with_options("q2", "Pick two"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Down));
    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    overlay
        .composer
        .set_text_content("Notes".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_eq!(overlay.current_index(), 1);
    let answer = overlay.answers.first().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, Some(1));
    assert!(answer.answer_committed);
}

#[test]
fn is_other_adds_none_of_the_above_and_submits_it() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_options_and_other("q1", "Pick one")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    let rows = overlay.option_rows();
    let other_row = rows.last().expect("expected none-of-the-above row");
    assert_eq!(other_row.name, "  4. None of the above");
    assert_eq!(
        other_row.description.as_deref(),
        Some(OTHER_OPTION_DESCRIPTION)
    );

    let other_idx = overlay.options_len().saturating_sub(1);
    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(other_idx);
    }
    overlay
        .composer
        .set_text_content("Custom answer".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();
    let draft = overlay.capture_composer_draft();
    if let Some(answer) = overlay.current_answer_mut() {
        answer.draft = draft;
        answer.answer_committed = true;
    }

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(
        answer.answers,
        vec![
            OTHER_OPTION_LABEL.to_string(),
            "user_note: Custom answer".to_string(),
        ]
    );
}

#[test]
fn large_paste_is_preserved_when_switching_questions() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "First"),
                question_without_options("q2", "Second"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    let large = "x".repeat(1_500);
    overlay.composer.handle_paste(large.clone());
    overlay.move_question(/*next*/ true);

    let draft = &overlay.answers[0].draft;
    assert_eq!(draft.pending_pastes.len(), 1);
    assert_eq!(draft.pending_pastes[0].1, large);
    assert!(draft.text.contains(&draft.pending_pastes[0].0));
    assert_eq!(draft.text_with_pending(), large);
}

#[test]
fn pending_paste_placeholder_survives_submission_and_back_navigation() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_with_options("q1", "First"),
                question_with_options("q2", "Second"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    let large = "x".repeat(1_200);
    overlay.focus = Focus::Notes;
    overlay.ensure_selected_for_notes();
    overlay.composer.handle_paste(large.clone());

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    overlay.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));

    let draft = &overlay.answers[0].draft;
    assert_eq!(draft.pending_pastes.len(), 1);
    assert!(draft.text.contains(&draft.pending_pastes[0].0));
    assert_eq!(draft.text_with_pending(), large);
}
