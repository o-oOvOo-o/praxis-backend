use super::*;

/// Behavior: `?` toggles the shortcut overlay only when the composer is otherwise empty. After
/// any typing has occurred, `?` should be inserted as a literal character.
#[test]
fn question_mark_only_toggles_on_first_char() {
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

    let (result, needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(result, InputResult::None);
    assert!(needs_redraw, "toggling overlay should request redraw");
    assert_eq!(composer.footer_mode, FooterMode::ShortcutOverlay);

    // Toggle back to prompt mode so subsequent typing captures characters.
    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(composer.footer_mode, FooterMode::ComposerEmpty);

    type_chars_humanlike(&mut composer, &['h']);
    assert_eq!(composer.textarea.text(), "h");
    assert_eq!(composer.footer_mode(), FooterMode::ComposerHasDraft);

    let (result, needs_redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(result, InputResult::None);
    assert!(needs_redraw, "typing should still mark the view dirty");
    let _ = flush_after_paste_burst(&mut composer);
    assert_eq!(composer.textarea.text(), "h?");
    assert_eq!(composer.footer_mode, FooterMode::ComposerEmpty);
    assert_eq!(composer.footer_mode(), FooterMode::ComposerHasDraft);
}

/// Behavior: while a paste-like burst is being captured, `?` must not toggle the shortcut
/// overlay; it should be treated as part of the pasted content.
#[test]
fn question_mark_does_not_toggle_during_paste_burst() {
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

    // Force an active paste burst so this test doesn't depend on tight timing.
    composer
        .paste_burst
        .begin_with_retro_grabbed(String::new(), Instant::now());

    for ch in ['h', 'i', '?', 't', 'h', 'e', 'r', 'e'] {
        let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert!(composer.is_in_paste_burst());
    assert_eq!(composer.textarea.text(), "");

    let _ = flush_after_paste_burst(&mut composer);

    assert_eq!(composer.textarea.text(), "hi?there");
    assert_ne!(composer.footer_mode, FooterMode::ShortcutOverlay);
}

#[test]
fn set_connector_mentions_refreshes_open_mention_popup() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_connectors_enabled(/*enabled*/ true);
    composer.set_text_content("$".to_string(), Vec::new(), Vec::new());
    assert!(matches!(composer.active_popup, ActivePopup::None));

    let connectors = vec![AppInfo {
        id: "connector_1".to_string(),
        name: "Notion".to_string(),
        description: Some("Workspace docs".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://example.test/notion".to_string()),
        is_accessible: true,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];
    composer.set_connector_mentions(Some(ConnectorsSnapshot { connectors }));

    let ActivePopup::Skill(popup) = &composer.active_popup else {
        panic!("expected mention popup to open after connectors update");
    };
    let mention = popup
        .selected_mention()
        .expect("expected connector mention to be selected");
    assert_eq!(mention.insert_text, "$notion".to_string());
    assert_eq!(mention.path, Some("app://connector_1".to_string()));
}

#[test]
fn set_connector_mentions_skips_disabled_connectors() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_connectors_enabled(/*enabled*/ true);
    composer.set_text_content("$".to_string(), Vec::new(), Vec::new());
    assert!(matches!(composer.active_popup, ActivePopup::None));

    let connectors = vec![AppInfo {
        id: "connector_1".to_string(),
        name: "Notion".to_string(),
        description: Some("Workspace docs".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://example.test/notion".to_string()),
        is_accessible: true,
        is_enabled: false,
        plugin_display_names: Vec::new(),
    }];
    composer.set_connector_mentions(Some(ConnectorsSnapshot { connectors }));

    assert!(
        matches!(composer.active_popup, ActivePopup::None),
        "disabled connectors should not appear in the mention popup"
    );
}

#[test]
fn set_plugin_mentions_refreshes_open_mention_popup() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_text_content("$".to_string(), Vec::new(), Vec::new());
    assert!(matches!(composer.active_popup, ActivePopup::None));

    composer.set_plugin_mentions(Some(vec![PluginCapabilitySummary {
        config_name: "sample@test".to_string(),
        display_name: "Sample Plugin".to_string(),
        description: None,
        has_skills: true,
        has_llm: false,
        mcp_server_names: vec!["sample".to_string()],
        app_connector_ids: Vec::new(),
        commands: Vec::new(),
    }]));

    let ActivePopup::Skill(popup) = &composer.active_popup else {
        panic!("expected mention popup to open after plugin update");
    };
    let mention = popup
        .selected_mention()
        .expect("expected plugin mention to be selected");
    assert_eq!(mention.insert_text, "$sample".to_string());
    assert_eq!(mention.path, Some("plugin://sample@test".to_string()));
}

#[test]
fn mention_items_show_plugin_owned_skill_and_app_duplicates() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_connectors_enabled(/*enabled*/ true);
    composer.set_text_content("$goog".to_string(), Vec::new(), Vec::new());
    composer.set_skill_mentions(Some(vec![SkillMetadata {
        name: "google-calendar:availability".to_string(),
        description: "Find availability and plan event changes".to_string(),
        short_description: None,
        interface: Some(praxis_core::skills::model::SkillInterface {
            display_name: Some("Google Calendar".to_string()),
            short_description: None,
            icon_small: None,
            icon_large: None,
            brand_color: None,
            default_prompt: None,
        }),
        dependencies: None,
        policy: None,
        path_to_skills_md: PathBuf::from("/tmp/repo/google-calendar/SKILL.md"),
        scope: praxis_protocol::protocol::SkillScope::Repo,
    }]));
    composer.set_plugin_mentions(Some(vec![PluginCapabilitySummary {
        config_name: "google-calendar@debug".to_string(),
        display_name: "Google Calendar".to_string(),
        description: Some(
            "Connect Google Calendar for scheduling, availability, and event management."
                .to_string(),
        ),
        has_skills: true,
        has_llm: false,
        mcp_server_names: vec!["google-calendar".to_string()],
        app_connector_ids: vec![praxis_core::plugins::AppConnectorId(
            "google_calendar".to_string(),
        )],
        commands: Vec::new(),
    }]));
    composer.set_connector_mentions(Some(ConnectorsSnapshot {
        connectors: vec![AppInfo {
            id: "google_calendar".to_string(),
            name: "Google Calendar".to_string(),
            description: Some("Look up events and availability".to_string()),
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: Some("https://example.test/google-calendar".to_string()),
            is_accessible: true,
            is_enabled: true,
            plugin_display_names: vec!["Google Calendar".to_string()],
        }],
    }));

    let mentions = composer.mention_items();
    assert_eq!(mentions.len(), 3);
    assert_eq!(mentions[0].category_tag, Some("[Skill]".to_string()));
    assert_eq!(
        mentions[0].path,
        Some("/tmp/repo/google-calendar/SKILL.md".to_string())
    );
    assert_eq!(mentions[0].display_name, "Google Calendar".to_string());
    assert_eq!(mentions[1].category_tag, Some("[Plugin]".to_string()));
    assert_eq!(
        mentions[1].path,
        Some("plugin://google-calendar@debug".to_string())
    );
    assert_eq!(mentions[2].category_tag, Some("[App]".to_string()));
    assert_eq!(mentions[2].path, Some("app://google_calendar".to_string()));
}

#[test]
fn plugin_mention_popup_snapshot() {
    snapshot_composer_state(
        "plugin_mention_popup",
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.set_text_content("$sa".to_string(), Vec::new(), Vec::new());
            composer.set_plugin_mentions(Some(vec![PluginCapabilitySummary {
                config_name: "sample@test".to_string(),
                display_name: "Sample Plugin".to_string(),
                description: Some(
                    "Plugin that includes the Figma MCP server and Skills for common workflows"
                        .to_string(),
                ),
                has_skills: true,
                has_llm: false,
                mcp_server_names: vec!["sample".to_string()],
                app_connector_ids: vec![praxis_core::plugins::AppConnectorId(
                    "calendar".to_string(),
                )],
                commands: Vec::new(),
            }]));
        },
    );
}

#[test]
fn mention_popup_type_prefixes_snapshot() {
    snapshot_composer_state_with_width(
        "mention_popup_type_prefixes",
        /*width*/ 72,
        /*enhanced_keys_supported*/ false,
        |composer| {
            composer.set_connectors_enabled(/*enabled*/ true);
            composer.set_text_content("$goog".to_string(), Vec::new(), Vec::new());
            composer.set_skill_mentions(Some(vec![SkillMetadata {
                name: "google-calendar-skill".to_string(),
                description: "Find availability and plan event changes".to_string(),
                short_description: None,
                interface: Some(praxis_core::skills::model::SkillInterface {
                    display_name: Some("Google Calendar".to_string()),
                    short_description: None,
                    icon_small: None,
                    icon_large: None,
                    brand_color: None,
                    default_prompt: None,
                }),
                dependencies: None,
                policy: None,
                path_to_skills_md: PathBuf::from("/tmp/repo/google-calendar/SKILL.md"),
                scope: praxis_protocol::protocol::SkillScope::Repo,
            }]));
            composer.set_plugin_mentions(Some(vec![PluginCapabilitySummary {
                config_name: "google-calendar@debug".to_string(),
                display_name: "Google Calendar".to_string(),
                description: Some(
                    "Connect Google Calendar for scheduling, availability, and event management."
                        .to_string(),
                ),
                has_skills: false,
                has_llm: false,
                mcp_server_names: vec!["google-calendar".to_string()],
                app_connector_ids: Vec::new(),
                commands: Vec::new(),
            }]));
            composer.set_connector_mentions(Some(ConnectorsSnapshot {
                connectors: vec![AppInfo {
                    id: "google_calendar".to_string(),
                    name: "Google Calendar".to_string(),
                    description: Some("Look up events and availability".to_string()),
                    logo_url: None,
                    logo_url_dark: None,
                    distribution_channel: None,
                    branding: None,
                    app_metadata: None,
                    labels: None,
                    install_url: Some("https://example.test/google-calendar".to_string()),
                    is_accessible: true,
                    is_enabled: true,
                    plugin_display_names: Vec::new(),
                }],
            }));
        },
    );
}

#[test]
fn set_connector_mentions_excludes_disabled_apps_from_mention_popup() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let sender = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(
        /*has_input_focus*/ true,
        sender,
        /*enhanced_keys_supported*/ false,
        "Ask Praxis to do anything".to_string(),
        /*disable_paste_burst*/ false,
    );
    composer.set_connectors_enabled(/*enabled*/ true);
    composer.set_text_content("$".to_string(), Vec::new(), Vec::new());

    let connectors = vec![AppInfo {
        id: "connector_1".to_string(),
        name: "Notion".to_string(),
        description: Some("Workspace docs".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://example.test/notion".to_string()),
        is_accessible: true,
        is_enabled: false,
        plugin_display_names: Vec::new(),
    }];
    composer.set_connector_mentions(Some(ConnectorsSnapshot { connectors }));

    assert!(matches!(composer.active_popup, ActivePopup::None));
}

#[test]
fn shortcut_overlay_persists_while_task_running() {
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

    let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(composer.footer_mode, FooterMode::ShortcutOverlay);

    composer.set_task_running(/*running*/ true);

    assert_eq!(composer.footer_mode, FooterMode::ShortcutOverlay);
    assert_eq!(composer.footer_mode(), FooterMode::ShortcutOverlay);
}

#[test]
fn test_current_at_token_basic_cases() {
    let test_cases = vec![
        // Valid @ tokens
        ("@hello", 3, Some("hello".to_string()), "Basic ASCII token"),
        (
            "@file.txt",
            4,
            Some("file.txt".to_string()),
            "ASCII with extension",
        ),
        (
            "hello @world test",
            8,
            Some("world".to_string()),
            "ASCII token in middle",
        ),
        (
            "@test123",
            5,
            Some("test123".to_string()),
            "ASCII with numbers",
        ),
        // Unicode examples
        ("@İstanbul", 3, Some("İstanbul".to_string()), "Turkish text"),
        (
            "@testЙЦУ.rs",
            8,
            Some("testЙЦУ.rs".to_string()),
            "Mixed ASCII and Cyrillic",
        ),
        ("@诶", 2, Some("诶".to_string()), "Chinese character"),
        ("@👍", 2, Some("👍".to_string()), "Emoji token"),
        // Invalid cases (should return None)
        ("hello", 2, None, "No @ symbol"),
        (
            "@",
            1,
            Some("".to_string()),
            "Only @ symbol triggers empty query",
        ),
        ("@ hello", 2, None, "@ followed by space"),
        ("test @ world", 6, None, "@ with spaces around"),
    ];

    for (input, cursor_pos, expected, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert_eq!(
            result, expected,
            "Failed for case: {description} - input: '{input}', cursor: {cursor_pos}"
        );
    }
}

#[test]
fn test_current_at_token_cursor_positions() {
    let test_cases = vec![
        // Different cursor positions within a token
        ("@test", 0, Some("test".to_string()), "Cursor at @"),
        ("@test", 1, Some("test".to_string()), "Cursor after @"),
        ("@test", 5, Some("test".to_string()), "Cursor at end"),
        // Multiple tokens - cursor determines which token
        ("@file1 @file2", 0, Some("file1".to_string()), "First token"),
        (
            "@file1 @file2",
            8,
            Some("file2".to_string()),
            "Second token",
        ),
        // Edge cases
        ("@", 0, Some("".to_string()), "Only @ symbol"),
        ("@a", 2, Some("a".to_string()), "Single character after @"),
        ("", 0, None, "Empty input"),
    ];

    for (input, cursor_pos, expected, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert_eq!(
            result, expected,
            "Failed for cursor position case: {description} - input: '{input}', cursor: {cursor_pos}",
        );
    }
}

#[test]
fn test_current_at_token_whitespace_boundaries() {
    let test_cases = vec![
        // Space boundaries
        (
            "aaa@aaa",
            4,
            None,
            "Connected @ token - no completion by design",
        ),
        (
            "aaa @aaa",
            5,
            Some("aaa".to_string()),
            "@ token after space",
        ),
        (
            "test @file.txt",
            7,
            Some("file.txt".to_string()),
            "@ token after space",
        ),
        // Full-width space boundaries
        (
            "test　@İstanbul",
            8,
            Some("İstanbul".to_string()),
            "@ token after full-width space",
        ),
        (
            "@ЙЦУ　@诶",
            10,
            Some("诶".to_string()),
            "Full-width space between Unicode tokens",
        ),
        // Tab and newline boundaries
        (
            "test\t@file",
            6,
            Some("file".to_string()),
            "@ token after tab",
        ),
    ];

    for (input, cursor_pos, expected, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert_eq!(
            result, expected,
            "Failed for whitespace boundary case: {description} - input: '{input}', cursor: {cursor_pos}",
        );
    }
}

#[test]
fn test_current_at_token_tracks_tokens_with_second_at() {
    let input = "npx -y @kaeawc/auto-mobile@latest";
    let token_start = input.find("@kaeawc").expect("scoped npm package present");
    let version_at = input
        .rfind("@latest")
        .expect("version suffix present in scoped npm package");
    let test_cases = vec![
        (token_start, "Cursor at leading @"),
        (token_start + 8, "Cursor inside scoped package name"),
        (version_at, "Cursor at version @"),
        (input.len(), "Cursor at end of token"),
    ];

    for (cursor_pos, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert_eq!(
            result,
            Some("kaeawc/auto-mobile@latest".to_string()),
            "Failed for case: {description} - input: '{input}', cursor: {cursor_pos}"
        );
    }
}

#[test]
fn test_current_at_token_allows_file_queries_with_second_at() {
    let input = "@icons/icon@2x.png";
    let version_at = input
        .rfind("@2x")
        .expect("second @ in file token should be present");
    let test_cases = vec![
        (0, "Cursor at leading @"),
        (8, "Cursor before second @"),
        (version_at, "Cursor at second @"),
        (input.len(), "Cursor at end of token"),
    ];

    for (cursor_pos, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert!(
            result.is_some(),
            "Failed for case: {description} - input: '{input}', cursor: {cursor_pos}"
        );
    }
}

#[test]
fn test_current_at_token_ignores_mid_word_at() {
    let input = "foo@bar";
    let at_pos = input.find('@').expect("@ present");
    let test_cases = vec![
        (at_pos, "Cursor at mid-word @"),
        (input.len(), "Cursor at end of word containing @"),
    ];

    for (cursor_pos, description) in test_cases {
        let mut textarea = TextArea::new();
        textarea.insert_str(input);
        textarea.set_cursor(cursor_pos);

        let result = ChatComposer::current_at_token(&textarea);
        assert_eq!(
            result, None,
            "Failed for case: {description} - input: '{input}', cursor: {cursor_pos}"
        );
    }
}
