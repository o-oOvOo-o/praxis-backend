use super::*;

#[test]
fn vim_keys_move_option_selection() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, Some(0));

    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('j')));
    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, Some(1));

    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('k')));
    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, Some(0));
}

#[test]
fn typing_in_options_does_not_open_notes() {
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

    assert_eq!(overlay.current_index(), 0);
    assert_eq!(overlay.notes_ui_visible(), false);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('x')));
    assert_eq!(overlay.current_index(), 0);
    assert_eq!(overlay.notes_ui_visible(), false);
    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.composer.current_text_with_pending(), "");
}

#[test]
fn h_l_move_between_questions_in_options() {
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

    assert_eq!(overlay.current_index(), 0);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('l')));
    assert_eq!(overlay.current_index(), 1);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('h')));
    assert_eq!(overlay.current_index(), 0);
}

#[test]
fn left_right_move_between_questions_in_options() {
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

    assert_eq!(overlay.current_index(), 0);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Right));
    assert_eq!(overlay.current_index(), 1);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Left));
    assert_eq!(overlay.current_index(), 0);
}

#[test]
fn options_notes_focus_hides_question_navigation_tip() {
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
    let tips = overlay.footer_tips();
    let tip_texts = tips.iter().map(|tip| tip.text.as_str()).collect::<Vec<_>>();
    assert_eq!(
        tip_texts,
        vec![
            "tab to add notes",
            "enter to submit answer",
            "←/→ to navigate questions",
            "esc to interrupt",
        ]
    );

    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    let tips = overlay.footer_tips();
    let tip_texts = tips.iter().map(|tip| tip.text.as_str()).collect::<Vec<_>>();
    assert_eq!(
        tip_texts,
        vec!["tab or esc to clear notes", "enter to submit answer",]
    );
}

#[test]
fn freeform_shows_ctrl_p_and_ctrl_n_question_navigation_tip() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_with_options("q1", "Area"),
                question_without_options("q2", "Goal"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    overlay.move_question(/*next*/ true);

    let tips = overlay.footer_tips();
    let tip_texts = tips.iter().map(|tip| tip.text.as_str()).collect::<Vec<_>>();
    assert_eq!(
        tip_texts,
        vec![
            "enter to submit all",
            "ctrl + p / ctrl + n change question",
            "esc to interrupt",
        ]
    );
}

#[test]
fn tab_opens_notes_when_option_selected() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(1);

    assert_eq!(overlay.notes_ui_visible(), false);
    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));
    assert_eq!(overlay.notes_ui_visible(), true);
    assert!(matches!(overlay.focus, Focus::Notes));
}

#[test]
fn switching_to_options_resets_notes_focus_when_notes_hidden() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "Notes"),
                question_with_options("q2", "Pick one"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    assert!(matches!(overlay.focus, Focus::Notes));
    overlay.move_question(/*next*/ true);

    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);
}

#[test]
fn switching_from_freeform_with_text_resets_focus_and_keeps_last_option_empty() {
    let (tx, mut rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![
                question_without_options("q1", "Notes"),
                question_with_options("q2", "Pick one"),
            ],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    overlay
        .composer
        .set_text_content("freeform notes".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();

    overlay.move_question(/*next*/ true);

    assert!(matches!(overlay.focus, Focus::Options));
    assert_eq!(overlay.notes_ui_visible(), false);

    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert!(overlay.confirm_unanswered_active());
    assert!(
        rx.try_recv().is_err(),
        "unexpected AppEvent before confirmation submit"
    );
    overlay.handle_key_event(KeyEvent::from(KeyCode::Char('1')));
    overlay.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::AgentOp(Op::UserInputAnswer { response, .. }) = event else {
        panic!("expected UserInputAnswer");
    };
    let answer = response.answers.get("q1").expect("answer missing");
    assert_eq!(answer.answers, Vec::<String>::new());
    let answer = response.answers.get("q2").expect("answer missing");
    assert_eq!(answer.answers, vec!["Option 1".to_string()]);
}
