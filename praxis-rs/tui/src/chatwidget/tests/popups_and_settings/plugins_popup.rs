use super::*;

#[tokio::test]
async fn plugins_popup_loading_state_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    chat.add_plugins_output();

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        popup.contains("Loading available plugins..."),
        "expected /plugins to open in a loading state before the marketplace arrives, got:\n{popup}"
    );
    assert_chatwidget_snapshot!("plugins_popup_loading_state", popup);
}

#[tokio::test]
async fn plugins_popup_snapshot_shows_all_marketplaces_and_sorts_installed_then_name() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    let mut response = plugins_test_response(vec![
        plugins_test_curated_marketplace(vec![
            plugins_test_summary(
                "plugin-bravo",
                "bravo",
                Some("Bravo Search"),
                Some("Search docs and tickets."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
            plugins_test_summary(
                "plugin-alpha",
                "alpha",
                Some("Alpha Sync"),
                Some("Already installed but disabled."),
                /*installed*/ true,
                /*enabled*/ false,
                PluginInstallPolicy::Available,
            ),
            plugins_test_summary(
                "plugin-starter",
                "starter",
                Some("Starter"),
                Some("Included by default."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::InstalledByDefault,
            ),
        ]),
        plugins_test_repo_marketplace(vec![plugins_test_summary(
            "plugin-hidden",
            "hidden",
            Some("Hidden Repo Plugin"),
            Some("Should not be shown in /plugins."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        )]),
    ]);
    response.remote_sync_error = Some("remote sync timed out".to_string());

    let popup = render_loaded_plugins_popup(&mut chat, response);
    assert_chatwidget_snapshot!("plugins_popup_curated_marketplace", popup);
    assert!(
        popup.contains("Hidden Repo Plugin"),
        "expected /plugins to include non-curated marketplaces, got:\n{popup}"
    );
    assert!(
        plugins_test_popup_row_position(&popup, "Alpha Sync")
            < plugins_test_popup_row_position(&popup, "Bravo Search")
            && plugins_test_popup_row_position(&popup, "Bravo Search")
                < plugins_test_popup_row_position(&popup, "Hidden Repo Plugin")
            && plugins_test_popup_row_position(&popup, "Hidden Repo Plugin")
                < plugins_test_popup_row_position(&popup, "Starter"),
        "expected /plugins rows to sort installed plugins first, then alphabetically, got:\n{popup}"
    );
}

#[tokio::test]
async fn plugin_detail_popup_snapshot_shows_install_actions_and_capability_summaries() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    let summary = plugins_test_summary(
        "plugin-figma",
        "figma",
        Some("Figma"),
        Some("Design handoff."),
        /*installed*/ false,
        /*enabled*/ true,
        PluginInstallPolicy::Available,
    );
    let response = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        summary.clone(),
    ])]);
    let cwd = chat.config.cwd.clone();
    chat.on_plugins_loaded(cwd.to_path_buf(), Ok(response));
    chat.add_plugins_output();
    chat.on_plugin_detail_loaded(
        cwd.to_path_buf(),
        Ok(PluginReadResponse {
            plugin: plugins_test_detail(
                summary,
                Some("Turn Figma files into implementation context."),
                &["design-review", "extract-copy"],
                &[("Figma", true), ("Slack", false)],
                &["figma-mcp", "docs-mcp"],
            ),
        }),
    );

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert_chatwidget_snapshot!(
        "plugin_detail_popup_installable",
        strip_osc8_for_snapshot(&popup)
    );
}

#[tokio::test]
async fn plugin_detail_popup_hides_disclosure_for_installed_plugins() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    let summary = plugins_test_summary(
        "plugin-figma",
        "figma",
        Some("Figma"),
        Some("Design handoff."),
        /*installed*/ true,
        /*enabled*/ true,
        PluginInstallPolicy::Available,
    );
    let response = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        summary.clone(),
    ])]);
    let cwd = chat.config.cwd.clone();
    chat.on_plugins_loaded(cwd.to_path_buf(), Ok(response));
    chat.add_plugins_output();
    chat.on_plugin_detail_loaded(
        cwd.to_path_buf(),
        Ok(PluginReadResponse {
            plugin: plugins_test_detail(
                summary,
                Some("Turn Figma files into implementation context."),
                &["design-review", "extract-copy"],
                &[("Figma", true), ("Slack", false)],
                &["figma-mcp", "docs-mcp"],
            ),
        }),
    );

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        !popup.contains("Data shared with this app is subject to the app's"),
        "expected installed plugin details to hide the disclosure line, got:\n{popup}"
    );
    assert_chatwidget_snapshot!(
        "plugin_detail_popup_installed",
        strip_osc8_for_snapshot(&popup)
    );
}

#[tokio::test]
async fn plugins_popup_refresh_replaces_selection_with_first_row() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    let initial = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        plugins_test_summary(
            "plugin-notion",
            "notion",
            Some("Notion"),
            Some("Workspace docs."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
        plugins_test_summary(
            "plugin-slack",
            "slack",
            Some("Slack"),
            Some("Team chat."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
    ])]);
    render_loaded_plugins_popup(&mut chat, initial);
    chat.handle_key_event(KeyEvent::from(KeyCode::Down));

    let before = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        before.contains("› Slack"),
        "expected Slack to be selected before refresh, got:\n{before}"
    );

    let refreshed = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        plugins_test_summary(
            "plugin-airtable",
            "airtable",
            Some("Airtable"),
            Some("Structured records."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
        plugins_test_summary(
            "plugin-notion",
            "notion",
            Some("Notion"),
            Some("Workspace docs."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
        plugins_test_summary(
            "plugin-slack",
            "slack",
            Some("Slack"),
            Some("Team chat."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
    ])]);
    let cwd = chat.config.cwd.clone();
    chat.on_plugins_loaded(cwd.to_path_buf(), Ok(refreshed));

    let after = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        after.contains("› Airtable"),
        "expected refresh to rebuild the popup from the new first row, got:\n{after}"
    );
    assert!(
        after.contains("Slack"),
        "expected refreshed popup to include the updated plugin list, got:\n{after}"
    );
}

#[tokio::test]
async fn plugins_popup_refreshes_installed_counts_after_install() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    let initial = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        plugins_test_summary(
            "plugin-calendar",
            "calendar",
            Some("Calendar"),
            Some("Schedule management."),
            /*installed*/ false,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
        plugins_test_summary(
            "plugin-drive",
            "drive",
            Some("Drive"),
            Some("Document access."),
            /*installed*/ true,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
    ])]);
    let before = render_loaded_plugins_popup(&mut chat, initial);
    assert!(
        before.contains("Installed 1 of 2 available plugins."),
        "expected initial installed count before refresh, got:\n{before}"
    );
    assert!(
        before.contains("Available"),
        "expected pre-install popup copy before refresh, got:\n{before}"
    );

    let refreshed = plugins_test_response(vec![plugins_test_curated_marketplace(vec![
        plugins_test_summary(
            "plugin-calendar",
            "calendar",
            Some("Calendar"),
            Some("Schedule management."),
            /*installed*/ true,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
        plugins_test_summary(
            "plugin-drive",
            "drive",
            Some("Drive"),
            Some("Document access."),
            /*installed*/ true,
            /*enabled*/ true,
            PluginInstallPolicy::Available,
        ),
    ])]);
    let cwd = chat.config.cwd.clone();
    chat.on_plugins_loaded(cwd.to_path_buf(), Ok(refreshed));

    let after = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        after.contains("Installed 2 of 2 available plugins."),
        "expected /plugins to refresh installed counts after install, got:\n{after}"
    );
    assert!(
        after.contains("Installed   Press Enter to view plugin details."),
        "expected refreshed selected row copy to reflect the installed plugin state, got:\n{after}"
    );
}

#[tokio::test]
async fn plugins_popup_search_filters_visible_rows_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    render_loaded_plugins_popup(
        &mut chat,
        plugins_test_response(vec![plugins_test_curated_marketplace(vec![
            plugins_test_summary(
                "plugin-calendar",
                "calendar",
                Some("Calendar"),
                Some("Schedule management."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
            plugins_test_summary(
                "plugin-slack",
                "slack",
                Some("Slack"),
                Some("Team chat."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
            plugins_test_summary(
                "plugin-drive",
                "drive",
                Some("Drive"),
                Some("Document access."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
        ])]),
    );

    type_plugins_search_query(&mut chat, "sla");

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert_chatwidget_snapshot!("plugins_popup_search_filtered", popup);
    assert!(
        !popup.contains("Calendar") && !popup.contains("Drive"),
        "expected search to leave only matching rows visible, got:\n{popup}"
    );
}

#[tokio::test]
async fn plugins_popup_search_no_matches_and_backspace_restores_results() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);

    render_loaded_plugins_popup(
        &mut chat,
        plugins_test_response(vec![plugins_test_curated_marketplace(vec![
            plugins_test_summary(
                "plugin-calendar",
                "calendar",
                Some("Calendar"),
                Some("Schedule management."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
            plugins_test_summary(
                "plugin-slack",
                "slack",
                Some("Slack"),
                Some("Team chat."),
                /*installed*/ false,
                /*enabled*/ true,
                PluginInstallPolicy::Available,
            ),
        ])]),
    );

    type_plugins_search_query(&mut chat, "zzz");

    let no_matches = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        no_matches.contains("zzz"),
        "expected popup to show the typed search query, got:\n{no_matches}"
    );
    assert!(
        no_matches.contains("no matches"),
        "expected popup to render the no-matches UX, got:\n{no_matches}"
    );

    for _ in 0..3 {
        chat.handle_key_event(KeyEvent::from(KeyCode::Backspace));
    }

    let restored = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        restored.contains("Calendar") && restored.contains("Slack"),
        "expected clearing the query to restore the plugin rows, got:\n{restored}"
    );
    assert!(
        !restored.contains("no matches"),
        "did not expect the no-matches state after clearing the query, got:\n{restored}"
    );
}
