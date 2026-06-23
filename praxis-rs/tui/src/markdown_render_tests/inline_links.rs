use super::*;

#[test]
fn inline_code() {
    let text = render_markdown_text("Example of `Inline code`");
    let expected = Line::from_iter(["Example of ".into(), "Inline code".cyan()]).into();
    assert_eq!(text, expected);
}

#[test]
fn strong() {
    assert_eq!(
        render_markdown_text("**Strong**"),
        Text::from(Line::from("Strong".bold()))
    );
}

#[test]
fn emphasis() {
    assert_eq!(
        render_markdown_text("*Emphasis*"),
        Text::from(Line::from("Emphasis".italic()))
    );
}

#[test]
fn strikethrough() {
    assert_eq!(
        render_markdown_text("~~Strikethrough~~"),
        Text::from(Line::from("Strikethrough".crossed_out()))
    );
}

#[test]
fn strong_emphasis() {
    let text = render_markdown_text("**Strong *emphasis***");
    let expected = Text::from(Line::from_iter([
        "Strong ".bold(),
        "emphasis".bold().italic(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn link() {
    let text = render_markdown_text("[Link](https://example.com)");
    let expected = Text::from(Line::from_iter([
        "Link".into(),
        " (".into(),
        "https://example.com".cyan().underlined(),
        ")".into(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn load_location_suffix_regexes() {
    let _colon = &*COLON_LOCATION_SUFFIX_RE;
    let _hash = &*HASH_LOCATION_SUFFIX_RE;
}

#[test]
fn file_link_hides_destination() {
    let text = render_markdown_text_for_cwd(
        "[praxis-rs/tui/src/markdown_render.rs](/Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs".cyan()
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_appends_line_number_when_label_lacks_it() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs](/Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs:74)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74".cyan()
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_keeps_absolute_paths_outside_cwd() {
    let text = render_markdown_text_for_cwd(
        "[README.md:74](/Users/example/code/praxis/README.md:74)",
        Path::new("/Users/example/code/praxis/praxis-rs/tui"),
    );
    let expected = Text::from(Line::from_iter([
        "/Users/example/code/praxis/README.md:74".cyan()
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_appends_hash_anchor_when_label_lacks_it() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs](file:///Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs#L74C3)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_uses_target_path_for_hash_anchor() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs#L74C3](file:///Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs#L74C3)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_appends_range_when_label_lacks_it() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs](/Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs:74:3-76:9)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3-76:9".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_uses_target_path_for_range() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs:74:3-76:9](/Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs:74:3-76:9)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3-76:9".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_appends_hash_range_when_label_lacks_it() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs](file:///Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs#L74C3-L76C9)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3-76:9".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn multiline_file_link_label_after_styled_prefix_does_not_panic() {
    let text = render_markdown_text_for_cwd(
        "**bold** plain [foo\nbar](file:///Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs#L74C3)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "bold".bold(),
        " plain ".into(),
        "praxis-rs/tui/src/markdown_render.rs:74:3".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn file_link_uses_target_path_for_hash_range() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs#L74C3-L76C9](file:///Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs#L74C3-L76C9)",
        Path::new("/Users/example/code/praxis"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74:3-76:9".cyan(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn url_link_shows_destination() {
    let text = render_markdown_text("[docs](https://example.com/docs)");
    let expected = Text::from(Line::from_iter([
        "docs".into(),
        " (".into(),
        "https://example.com/docs".cyan().underlined(),
        ")".into(),
    ]));
    assert_eq!(text, expected);
}

#[test]
fn bare_relative_file_link_hides_destination() {
    let text = render_markdown_text_for_cwd(
        "[markdown_render.rs](praxis-rs/tui/src/markdown_render.rs:74)",
        Path::new("/Users/example/code"),
    );
    let expected = Text::from(Line::from_iter([
        "praxis-rs/tui/src/markdown_render.rs:74".cyan()
    ]));
    assert_eq!(text, expected);
}

#[test]
fn relative_local_link_hyperlink_target_resolves_against_cwd() {
    let target = hyperlink_target_for_local_link_text(
        "praxis-rs/tui/src/markdown_render.rs:74:3",
        Some(Path::new("/Users/example/code")),
    );

    assert_eq!(
        target,
        Some("file:///Users/example/code/praxis-rs/tui/src/markdown_render.rs#L74C3".to_string())
    );
}

#[test]
fn markdown_render_file_link_snapshot() {
    let text = render_markdown_text_for_cwd(
        "See [markdown_render.rs:74](/Users/example/code/praxis/praxis-rs/tui/src/markdown_render.rs:74).",
        Path::new("/Users/example/code/praxis"),
    );
    let rendered = text
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.clone())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_snapshot!(rendered);
}

#[test]
fn unordered_list_local_file_link_stays_inline_with_following_text() {
    let text = render_markdown_text_with_width_and_cwd(
        "- [binary](/Users/example/code/praxis/praxis-rs/README.md:93): core is the agent/business logic, tui is the terminal UI, exec is the headless automation surface, and cli is the top-level multitool binary.",
        Some(72),
        Some(Path::new("/Users/example/code/praxis")),
    );
    let rendered = text
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "- praxis-rs/README.md:93: core is the agent/business logic, tui is the",
            "  terminal UI, exec is the headless automation surface, and cli is the",
            "  top-level multitool binary.",
        ]
    );
}

#[test]
fn unordered_list_local_file_link_soft_break_before_colon_stays_inline() {
    let text = render_markdown_text_with_width_and_cwd(
        "- [binary](/Users/example/code/praxis/praxis-rs/README.md:93)\n  : core is the agent/business logic.",
        Some(72),
        Some(Path::new("/Users/example/code/praxis")),
    );
    let rendered = text
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec!["- praxis-rs/README.md:93: core is the agent/business logic.",]
    );
}

#[test]
fn consecutive_unordered_list_local_file_links_do_not_detach_paths() {
    let text = render_markdown_text_with_width_and_cwd(
        "- [binary](/Users/example/code/praxis/praxis-rs/README.md:93)\n  : cli is the top-level multitool binary.\n- [expectations](/Users/example/code/praxis/praxis-rs/core/README.md:1)\n  : praxis-core owns the real runtime behavior.",
        Some(72),
        Some(Path::new("/Users/example/code/praxis")),
    );
    let rendered = text
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "- praxis-rs/README.md:93: cli is the top-level multitool binary.",
            "- praxis-rs/core/README.md:1: praxis-core owns the real runtime behavior.",
        ]
    );
}

#[test]
fn wrapped_relative_local_file_link_token_stays_whole() {
    let text = render_markdown_text_with_width_and_cwd(
        "See [markdown_render.rs](praxis-rs/tui/src/markdown_render.rs:74) for details.",
        Some(18),
        Some(Path::new("/Users/example/code")),
    );
    let rendered = text
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains("praxis-rs/tui/src/markdown_render.rs:74"))
            .count(),
        1,
        "expected local file token to stay on one rendered line, got: {rendered:?}"
    );
}
