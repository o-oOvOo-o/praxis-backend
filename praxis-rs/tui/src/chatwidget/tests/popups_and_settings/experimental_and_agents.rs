use super::*;

#[tokio::test]
async fn experimental_features_popup_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    let features = vec![
        ExperimentalFeatureItem {
            feature: Feature::GhostCommit,
            name: "Ghost snapshots".to_string(),
            description: "Capture undo snapshots each turn.".to_string(),
            enabled: false,
        },
        ExperimentalFeatureItem {
            feature: Feature::ShellTool,
            name: "Shell tool".to_string(),
            description: "Allow the model to run shell commands.".to_string(),
            enabled: true,
        },
    ];
    let view = ExperimentalFeaturesView::new(features, chat.app_event_tx.clone());
    chat.bottom_pane.show_view(Box::new(view));

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("experimental_features_popup", popup);
}

#[tokio::test]
async fn experimental_features_toggle_saves_on_exit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    let expected_feature = Feature::GhostCommit;
    let view = ExperimentalFeaturesView::new(
        vec![ExperimentalFeatureItem {
            feature: expected_feature,
            name: "Ghost snapshots".to_string(),
            description: "Capture undo snapshots each turn.".to_string(),
            enabled: false,
        }],
        chat.app_event_tx.clone(),
    );
    chat.bottom_pane.show_view(Box::new(view));

    chat.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

    assert!(
        rx.try_recv().is_err(),
        "expected no updates until saving the popup"
    );

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let mut updates = None;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::UpdateFeatureFlags {
            updates: event_updates,
        } = event
        {
            updates = Some(event_updates);
            break;
        }
    }

    let updates = updates.expect("expected UpdateFeatureFlags event");
    assert_eq!(updates, vec![(expected_feature, true)]);
}

#[tokio::test]
async fn experimental_popup_shows_js_repl_node_requirement() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    let js_repl_description = FEATURES
        .iter()
        .find(|spec| spec.id == Feature::JsRepl)
        .and_then(|spec| spec.stage.experimental_menu_description())
        .expect("expected js_repl experimental description");
    let node_requirement = js_repl_description
        .split(". ")
        .find(|sentence| sentence.starts_with("Requires Node >= v"))
        .map(|sentence| sentence.trim_end_matches(" installed."))
        .expect("expected js_repl description to mention the Node requirement");

    chat.open_experimental_popup();

    let popup = render_bottom_popup(&chat, /*width*/ 120);
    assert!(
        popup.contains(node_requirement),
        "expected js_repl feature description to mention the required Node version, got:\n{popup}"
    );
}

#[tokio::test]
async fn experimental_popup_includes_guardian_approval() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let guardian_stage = FEATURES
        .iter()
        .find(|spec| spec.id == Feature::GuardianApproval)
        .map(|spec| spec.stage)
        .expect("expected guardian approval feature metadata");
    let guardian_name = guardian_stage
        .experimental_menu_name()
        .expect("expected guardian approval experimental menu name");
    let guardian_description = guardian_stage
        .experimental_menu_description()
        .expect("expected guardian approval experimental description");

    chat.open_experimental_popup();

    let popup = render_bottom_popup(&chat, /*width*/ 120);
    let normalized_popup = popup.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        popup.contains(guardian_name),
        "expected guardian approvals entry in experimental popup, got:\n{popup}"
    );
    assert!(
        normalized_popup.contains(guardian_description),
        "expected guardian approvals description in experimental popup, got:\n{popup}"
    );
}

#[tokio::test]
async fn multi_agent_enable_prompt_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.open_multi_agent_enable_prompt();

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("multi_agent_enable_prompt", popup);
}

#[tokio::test]
async fn multi_agent_enable_prompt_updates_feature_and_emits_notice() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.open_multi_agent_enable_prompt();
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::UpdateFeatureFlags { updates }) if updates == vec![(Feature::Collab, true)]
    );
    let cell = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => cell,
        other => panic!("expected InsertHistoryCell event, got {other:?}"),
    };
    let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 120));
    assert!(rendered.contains("Subagents will be enabled in the next session."));
}
