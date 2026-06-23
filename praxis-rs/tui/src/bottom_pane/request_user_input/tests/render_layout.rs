use super::*;

#[test]
fn request_user_input_options_snapshot() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Area")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let area = Rect::new(0, 0, 120, 16);
    insta::assert_snapshot!(
        "request_user_input_options",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_options_notes_visible_snapshot() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Area")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(0);
    }
    overlay.handle_key_event(KeyEvent::from(KeyCode::Tab));

    let area = Rect::new(0, 0, 120, 16);
    insta::assert_snapshot!(
        "request_user_input_options_notes_visible",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_tight_height_snapshot() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Area")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let area = Rect::new(0, 0, 120, 10);
    insta::assert_snapshot!(
        "request_user_input_tight_height",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn layout_allocates_all_wrapped_options_when_space_allows() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_wrapped_options("q1", "Next Step")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    let width = 48u16;
    let question_height = overlay.wrapped_question_lines(width).len() as u16;
    let options_height = overlay.options_required_height(width);
    let extras = 1u16 // progress
        .saturating_add(DESIRED_SPACERS_BETWEEN_SECTIONS)
        .saturating_add(overlay.footer_required_height(width));
    let height = question_height
        .saturating_add(options_height)
        .saturating_add(extras);
    let sections = overlay.layout_sections(Rect::new(0, 0, width, height));

    assert_eq!(sections.options_area.height, options_height);
}

#[test]
fn desired_height_keeps_spacers_and_preferred_options_visible() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_wrapped_options("q1", "Next Step")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    let width = 110u16;
    let height = overlay.desired_height(width);
    let content_area = menu_surface_inset(Rect::new(0, 0, width, height));
    let sections = overlay.layout_sections(content_area);
    let preferred = overlay.options_preferred_height(content_area.width);

    assert_eq!(sections.options_area.height, preferred);
    let question_bottom = sections.question_area.y + sections.question_area.height;
    let options_bottom = sections.options_area.y + sections.options_area.height;
    let spacer_after_question = sections.options_area.y.saturating_sub(question_bottom);
    let spacer_after_options = sections.notes_area.y.saturating_sub(options_bottom);
    assert_eq!(spacer_after_question, 1);
    assert_eq!(spacer_after_options, 1);
}

#[test]
fn footer_wraps_tips_without_splitting_individual_tips() {
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
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(0);

    let width = 36u16;
    let lines = overlay.footer_tip_lines(width);
    assert!(lines.len() > 1);
    let separator_width = UnicodeWidthStr::width(TIP_SEPARATOR);
    for tips in lines {
        let used = tips.iter().enumerate().fold(0usize, |acc, (idx, tip)| {
            let tip_width = UnicodeWidthStr::width(tip.text.as_str()).min(width as usize);
            let extra = if idx == 0 {
                tip_width
            } else {
                separator_width.saturating_add(tip_width)
            };
            acc.saturating_add(extra)
        });
        assert!(used <= width as usize);
    }
}

#[test]
fn request_user_input_wrapped_options_snapshot() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_wrapped_options("q1", "Next Step")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );

    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(0);
    }

    let width = 110u16;
    let question_height = overlay.wrapped_question_lines(width).len() as u16;
    let options_height = overlay.options_required_height(width);
    let height = 1u16
        .saturating_add(question_height)
        .saturating_add(options_height)
        .saturating_add(8);
    let area = Rect::new(0, 0, width, height);
    insta::assert_snapshot!(
        "request_user_input_wrapped_options",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_long_option_text_snapshot() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_very_long_option_text("q1", "Status")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let area = Rect::new(0, 0, 120, 18);
    insta::assert_snapshot!(
        "request_user_input_long_option_text",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn selected_long_wrapped_option_stays_visible() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![question_with_long_scroll_options("q1", "Scroll")],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(2);

    let rendered = render_snapshot(&overlay, Rect::new(0, 0, 80, 20));
    assert!(
        rendered.contains("› 3. Use Detailed Hint C"),
        "expected selected option to be visible in viewport\n{rendered}"
    );
}

#[test]
fn request_user_input_footer_wrap_snapshot() {
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
    let answer = overlay.current_answer_mut().expect("answer missing");
    answer.options_state.selected_idx = Some(1);

    let width = 52u16;
    let height = overlay.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    insta::assert_snapshot!(
        "request_user_input_footer_wrap",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_scroll_options_snapshot() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![RequestUserInputQuestion {
                id: "q1".to_string(),
                header: "Next Step".to_string(),
                question: "What would you like to do next?".to_string(),
                is_other: false,
                is_secret: false,
                options: Some(vec![
                    RequestUserInputQuestionOption {
                        label: "Discuss a code change (Recommended)".to_string(),
                        description: "Walk through a plan and edit code together.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Run tests".to_string(),
                        description: "Pick a crate and run its tests.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Review a diff".to_string(),
                        description: "Summarize or review current changes.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Refactor".to_string(),
                        description: "Tighten structure and remove dead code.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Ship it".to_string(),
                        description: "Finalize and open a PR.".to_string(),
                    },
                ]),
            }],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(3);
    }
    let area = Rect::new(0, 0, 120, 12);
    insta::assert_snapshot!(
        "request_user_input_scrolling_options",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_hidden_options_footer_snapshot() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event(
            "turn-1",
            vec![RequestUserInputQuestion {
                id: "q1".to_string(),
                header: "Next Step".to_string(),
                question: "What would you like to do next?".to_string(),
                is_other: false,
                is_secret: false,
                options: Some(vec![
                    RequestUserInputQuestionOption {
                        label: "Discuss a code change (Recommended)".to_string(),
                        description: "Walk through a plan and edit code together.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Run tests".to_string(),
                        description: "Pick a crate and run its tests.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Review a diff".to_string(),
                        description: "Summarize or review current changes.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Refactor".to_string(),
                        description: "Tighten structure and remove dead code.".to_string(),
                    },
                    RequestUserInputQuestionOption {
                        label: "Ship it".to_string(),
                        description: "Finalize and open a PR.".to_string(),
                    },
                ]),
            }],
        ),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    {
        let answer = overlay.current_answer_mut().expect("answer missing");
        answer.options_state.selected_idx = Some(3);
    }
    let area = Rect::new(0, 0, 80, 10);
    insta::assert_snapshot!(
        "request_user_input_hidden_options_footer",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_freeform_snapshot() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_without_options("q1", "Goal")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    let area = Rect::new(0, 0, 120, 10);
    insta::assert_snapshot!(
        "request_user_input_freeform",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_multi_question_first_snapshot() {
    let (tx, _rx) = test_sender();
    let overlay = RequestUserInputOverlay::new(
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
    let area = Rect::new(0, 0, 120, 15);
    insta::assert_snapshot!(
        "request_user_input_multi_question_first",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_multi_question_last_snapshot() {
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
    let area = Rect::new(0, 0, 120, 12);
    insta::assert_snapshot!(
        "request_user_input_multi_question_last",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn request_user_input_unanswered_confirmation_snapshot() {
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

    overlay.open_unanswered_confirmation();

    let area = Rect::new(0, 0, 80, 12);
    insta::assert_snapshot!(
        "request_user_input_unanswered_confirmation",
        render_snapshot(&overlay, area)
    );
}

#[test]
fn options_scroll_while_editing_notes() {
    let (tx, _rx) = test_sender();
    let mut overlay = RequestUserInputOverlay::new(
        request_event("turn-1", vec![question_with_options("q1", "Pick one")]),
        tx,
        /*has_input_focus*/ true,
        /*enhanced_keys_supported*/ false,
        /*disable_paste_burst*/ false,
    );
    overlay.select_current_option(/*committed*/ false);
    overlay.focus = Focus::Notes;
    overlay
        .composer
        .set_text_content("Notes".to_string(), Vec::new(), Vec::new());
    overlay.composer.move_cursor_to_end();

    overlay.handle_key_event(KeyEvent::from(KeyCode::Down));

    let answer = overlay.current_answer().expect("answer missing");
    assert_eq!(answer.options_state.selected_idx, Some(1));
    assert!(!answer.answer_committed);
}
