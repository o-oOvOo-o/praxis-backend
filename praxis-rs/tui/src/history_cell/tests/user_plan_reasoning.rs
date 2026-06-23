use super::*;

#[test]
fn user_history_cell_wraps_and_prefixes_each_line_snapshot() {
    let msg = "one two three four five six seven";
    let cell = UserHistoryCell {
        message: msg.to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: Vec::new(),
    };

    // Small width to force wrapping more clearly. Effective wrap width is width-2 due to the ▌ prefix and trailing space.
    let width: u16 = 12;
    let lines = cell.display_lines(width);
    let rendered = render_lines(&lines).join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn user_history_cell_renders_remote_image_urls() {
    let cell = UserHistoryCell {
        message: "describe these".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec!["https://example.com/example.png".to_string()],
    };

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    assert!(rendered.contains("[Image #1]"));
    assert!(rendered.contains("describe these"));
    insta::assert_snapshot!(rendered);
}

#[test]
fn user_history_cell_summarizes_inline_data_urls() {
    let cell = UserHistoryCell {
        message: "describe inline image".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec!["data:image/png;base64,aGVsbG8=".to_string()],
    };

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    assert!(rendered.contains("[Image #1]"));
    assert!(rendered.contains("describe inline image"));
}

#[test]
fn user_history_cell_numbers_multiple_remote_images() {
    let cell = UserHistoryCell {
        message: "describe both".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec![
            "https://example.com/one.png".to_string(),
            "https://example.com/two.png".to_string(),
        ],
    };

    let rendered = render_lines(&cell.display_lines(/*width*/ 80)).join("\n");

    assert!(rendered.contains("[Image #1]"));
    assert!(rendered.contains("[Image #2]"));
    insta::assert_snapshot!(rendered);
}

#[test]
fn user_history_cell_height_matches_rendered_lines_with_remote_images() {
    let cell = UserHistoryCell {
        message: "line one\nline two".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec![
            "https://example.com/one.png".to_string(),
            "https://example.com/two.png".to_string(),
        ],
    };

    let width = 80;
    let rendered_len: u16 = cell
        .display_lines(width)
        .len()
        .try_into()
        .unwrap_or(u16::MAX);
    assert_eq!(cell.desired_height(width), rendered_len);
    assert_eq!(cell.desired_transcript_height(width), rendered_len);
}

#[test]
fn user_history_cell_trims_trailing_blank_message_lines() {
    let cell = UserHistoryCell {
        message: "line one\n\n   \n\t \n".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec!["https://example.com/one.png".to_string()],
    };

    let rendered = render_lines(&cell.display_lines(/*width*/ 80));
    let trailing_blank_count = rendered
        .iter()
        .rev()
        .take_while(|line| line.trim().is_empty())
        .count();
    assert_eq!(trailing_blank_count, 1);
    assert!(rendered.iter().any(|line| line.contains("line one")));
}

#[test]
fn user_history_cell_trims_trailing_blank_message_lines_with_text_elements() {
    let message = "tokenized\n\n\n".to_string();
    let cell = UserHistoryCell {
        message,
        text_elements: vec![TextElement::new(
            (0..8).into(),
            Some("tokenized".to_string()),
        )],
        local_image_paths: Vec::new(),
        remote_image_urls: vec!["https://example.com/one.png".to_string()],
    };

    let rendered = render_lines(&cell.display_lines(/*width*/ 80));
    let trailing_blank_count = rendered
        .iter()
        .rev()
        .take_while(|line| line.trim().is_empty())
        .count();
    assert_eq!(trailing_blank_count, 1);
    assert!(rendered.iter().any(|line| line.contains("tokenized")));
}

#[test]
fn render_uses_wrapping_for_long_url_like_line() {
    let url = "https://example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890/artifacts/reports/performance/summary/detail/with/a/very/long/path/that/keeps/going/for/testing/purposes-only-and-does/not/need/to/resolve/index.html?session_id=abc123def456ghi789jkl012mno345pqr678stu901vwx234yz";
    let cell: Box<dyn HistoryCell> = Box::new(UserHistoryCell {
        message: url.to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: Vec::new(),
    });

    let width: u16 = 52;
    let height = cell.desired_height(width);
    assert!(
        height > 1,
        "expected wrapped height for long URL, got {height}"
    );

    let area = Rect::new(0, 0, width, height);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    cell.render(area, &mut buf);

    let rendered = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| {
                    let symbol = buf[(x, y)].symbol();
                    if symbol.is_empty() {
                        ' '
                    } else {
                        symbol.chars().next().unwrap_or(' ')
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let rendered_blob = rendered.join("\n");

    assert!(
        rendered_blob.contains("session_id=abc123"),
        "expected URL tail to be visible after wrapping, got:\n{rendered_blob}"
    );

    let non_empty_rows = rendered.iter().filter(|row| !row.trim().is_empty()).count() as u16;
    assert!(
        non_empty_rows > 3,
        "expected long URL to span multiple visible rows, got:\n{rendered_blob}"
    );
}

#[test]
fn plan_update_with_note_and_wrapping_snapshot() {
    // Long explanation forces wrapping; include long step text to verify step wrapping and alignment.
    let update = UpdatePlanArgs {
            explanation: Some(
                "I’ll update Grafana call error handling by adding retries and clearer messages when the backend is unreachable."
                    .to_string(),
            ),
            plan: vec![
                PlanItemArg {
                    step: "Investigate existing error paths and logging around HTTP timeouts".into(),
                    status: StepStatus::Completed,
                },
                PlanItemArg {
                    step: "Harden Grafana client error handling with retry/backoff and user‑friendly messages".into(),
                    status: StepStatus::InProgress,
                },
                PlanItemArg {
                    step: "Add tests for transient failure scenarios and surfacing to the UI".into(),
                    status: StepStatus::Pending,
                },
            ],
        };

    let cell = new_plan_update(update);
    // Narrow width to force wrapping for both the note and steps
    let lines = cell.display_lines(/*width*/ 32);
    let rendered = render_lines(&lines).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn plan_update_without_note_snapshot() {
    let update = UpdatePlanArgs {
        explanation: None,
        plan: vec![
            PlanItemArg {
                step: "Define error taxonomy".into(),
                status: StepStatus::InProgress,
            },
            PlanItemArg {
                step: "Implement mapping to user messages".into(),
                status: StepStatus::Pending,
            },
        ],
    };

    let cell = new_plan_update(update);
    let lines = cell.display_lines(/*width*/ 40);
    let rendered = render_lines(&lines).join("\n");
    insta::assert_snapshot!(rendered);
}

#[test]
fn plan_update_does_not_split_url_like_tokens_in_note_or_step() {
    let note_url = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890";
    let step_url = "example.test/api/v1/projects/beta-team/releases/2026-02-17/builds/0987654321/artifacts/reports/performance";
    let update = UpdatePlanArgs {
        explanation: Some(format!(
            "Investigate failures under {note_url} immediately."
        )),
        plan: vec![PlanItemArg {
            step: format!("Validate callbacks under {step_url} before rollout."),
            status: StepStatus::InProgress,
        }],
    };

    let cell = new_plan_update(update);
    let rendered = render_lines(&cell.display_lines(/*width*/ 30));

    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains(note_url))
            .count(),
        1,
        "expected full note URL-like token in one rendered line, got: {rendered:?}"
    );
    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains(step_url))
            .count(),
        1,
        "expected full step URL-like token in one rendered line, got: {rendered:?}"
    );
}

#[test]
fn reasoning_summary_block() {
    let cell = new_reasoning_summary_block(
        "**High level reasoning**\n\nDetailed reasoning goes here.".to_string(),
        &test_cwd(),
    );

    let rendered_display = render_lines(&cell.display_lines(/*width*/ 80));
    assert_eq!(rendered_display, vec!["• Detailed reasoning goes here."]);

    let rendered_transcript = render_transcript(cell.as_ref());
    assert_eq!(rendered_transcript, vec!["• Detailed reasoning goes here."]);
}

#[test]
fn reasoning_summary_height_matches_wrapped_rendering_for_url_like_content() {
    let summary = "example.test/api/v1/projects/alpha-team/releases/2026-02-17/builds/1234567890/artifacts/reports/performance/summary/detail/with/a/very/long/path/that/keeps/going";
    let cell: Box<dyn HistoryCell> = Box::new(ReasoningSummaryCell::new(
        "High level reasoning".to_string(),
        summary.to_string(),
        &test_cwd(),
        /*transcript_only*/ false,
    ));
    let width: u16 = 24;

    let logical_height = cell.display_lines(width).len() as u16;
    let wrapped_height = cell.desired_height(width);
    let expected_wrapped_height = Paragraph::new(Text::from(cell.display_lines(width)))
        .wrap(Wrap { trim: false })
        .line_count(width) as u16;
    assert_eq!(wrapped_height, expected_wrapped_height);
    assert!(
        wrapped_height >= logical_height,
        "expected wrapped height to be at least logical line count ({logical_height}), got {wrapped_height}"
    );

    let wrapped_transcript_height = cell.desired_transcript_height(width);
    assert_eq!(wrapped_transcript_height, wrapped_height);

    let area = Rect::new(0, 0, width, wrapped_height);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    cell.render(area, &mut buf);

    let first_row = (0..area.width)
        .map(|x| {
            let symbol = buf[(x, 0)].symbol();
            if symbol.is_empty() {
                ' '
            } else {
                symbol.chars().next().unwrap_or(' ')
            }
        })
        .collect::<String>();
    assert!(
        first_row.contains("•"),
        "expected first rendered row to keep summary bullet visible, got: {first_row:?}"
    );
}

#[test]
fn reasoning_summary_block_returns_reasoning_cell_when_feature_disabled() {
    let cell =
        new_reasoning_summary_block("Detailed reasoning goes here.".to_string(), &test_cwd());

    let rendered_display = render_lines(&cell.display_lines(/*width*/ 80));
    assert_eq!(rendered_display, vec!["• Detailed reasoning goes here."]);

    let rendered = render_transcript(cell.as_ref());
    assert_eq!(rendered, vec!["• Detailed reasoning goes here."]);
}

#[tokio::test]
async fn reasoning_summary_block_respects_config_overrides() {
    let mut config = test_config().await;
    config.model = Some("gpt-3.5-turbo".to_string());
    config.model_supports_reasoning_summaries = Some(true);
    let cell = new_reasoning_summary_block(
        "**High level reasoning**\n\nDetailed reasoning goes here.".to_string(),
        &test_cwd(),
    );

    let rendered_display = render_lines(&cell.display_lines(/*width*/ 80));
    assert_eq!(rendered_display, vec!["• Detailed reasoning goes here."]);
}

#[test]
fn reasoning_summary_block_falls_back_when_header_is_missing() {
    let cell = new_reasoning_summary_block(
        "**High level reasoning without closing".to_string(),
        &test_cwd(),
    );

    let rendered = render_transcript(cell.as_ref());
    assert_eq!(rendered, vec!["• **High level reasoning without closing"]);
}

#[test]
fn reasoning_summary_block_falls_back_when_summary_is_missing() {
    let cell = new_reasoning_summary_block(
        "**High level reasoning without closing**".to_string(),
        &test_cwd(),
    );

    let rendered = render_transcript(cell.as_ref());
    assert_eq!(rendered, vec!["• High level reasoning without closing"]);

    let cell = new_reasoning_summary_block(
        "**High level reasoning without closing**\n\n  ".to_string(),
        &test_cwd(),
    );

    let rendered = render_transcript(cell.as_ref());
    assert_eq!(rendered, vec!["• High level reasoning without closing"]);
}

#[test]
fn reasoning_summary_block_splits_header_and_summary_when_present() {
    let cell = new_reasoning_summary_block(
        "**High level plan**\n\nWe should fix the bug next.".to_string(),
        &test_cwd(),
    );

    let rendered_display = render_lines(&cell.display_lines(/*width*/ 80));
    assert_eq!(rendered_display, vec!["• We should fix the bug next."]);

    let rendered_transcript = render_transcript(cell.as_ref());
    assert_eq!(rendered_transcript, vec!["• We should fix the bug next."]);
}

#[test]
fn deprecation_notice_renders_summary_with_details() {
    let cell = new_deprecation_notice(
        "Feature flag `foo`".to_string(),
        Some("Use flag `bar` instead.".to_string()),
    );
    let lines = cell.display_lines(/*width*/ 80);
    let rendered = render_lines(&lines);
    assert_eq!(
        rendered,
        vec![
            "⚠ Feature flag `foo`".to_string(),
            "Use flag `bar` instead.".to_string(),
        ]
    );
}
