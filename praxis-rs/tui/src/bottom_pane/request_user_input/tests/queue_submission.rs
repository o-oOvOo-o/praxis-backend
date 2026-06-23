use super::*;

#[test]
fn queued_requests_are_fifo() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "First")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    overlay.try_consume_user_input_request(request_event(
        "turn-2",
        vec![question_with_options("q2", "Second")],
    ));
    overlay.try_consume_user_input_request(request_event(
        "turn-3",
        vec![question_with_options("q3", "Third")],
    ));

    overlay.submit_answers();
    assert_eq!(overlay.request.turn_id, "turn-2");

    overlay.submit_answers();
    assert_eq!(overlay.request.turn_id, "turn-3");
}

#[test]
fn interrupt_discards_queued_requests_and_emits_interrupt() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "First")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    overlay.try_consume_user_input_request(RequestUserInputEvent {
        call_id: "call-2".to_string(),
        turn_id: "turn-2".to_string(),
        questions: vec![question_with_options("q2", "Second")],
    });
    overlay.try_consume_user_input_request(RequestUserInputEvent {
        call_id: "call-3".to_string(),
        turn_id: "turn-3".to_string(),
        questions: vec![question_with_options("q3", "Third")],
    });

    overlay.handle_key_event(KeyEvent::from(KeyCode::Esc));

    assert!(overlay.done, "expected overlay to be done");
    expect_interrupt_only(&mut rx);
}

#[test]
fn options_can_submit_empty_when_unanswered() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.submit_answers();

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { id, response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    assert_eq!(id, "turn-1");
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, Vec::<String>::new());
}

#[test]
fn enter_commits_default_selection_on_last_option_question() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, vec!["Option 1".to_string()]);
}

#[test]
fn enter_commits_default_selection_on_non_last_option_question() {
    let (tx, mut rx) = test_sender();
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

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_eq!(overlay.current_index(), 1);
    let first_answer = &overlay.answers[0];
    assert!(first_answer.answer_committed);
    assert_eq!(first_answer.options_state.selected_idx, Some(0));
    assert!(
        rx.try_recv().is_err(),
        "unexpected AppEvent before full submission"
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let mut expected = HashMap::new();
    expected.insert(
        "q1".to_string(),
        RequestUserInputAnswer {
            answers: vec!["Option 1".to_string()],
        },
    );
    expected.insert(
        "q2".to_string(),
        RequestUserInputAnswer {
            answers: vec!["Option 1".to_string()],
        },
    );
    assert_eq!(response.answers, expected);
}

#[test]
fn number_keys_select_and_submit_options() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('2')));

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, vec!["Option 2".to_string()]);
}
