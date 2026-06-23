use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_store_and_load_values_across_turns() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call(
                "call-1",
                "exec",
                r#"
store("nb", { title: "Notebook", items: [1, true, null] });
text("stored");
"#,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let first_follow_up = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "stored"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("store value for later").await?;

    let first_request = first_follow_up.single_request();
    let (first_output, first_success) =
        custom_tool_output_body_and_success(&first_request, "call-1");
    assert_ne!(
        first_success,
        Some(false),
        "exec store call failed unexpectedly: {first_output}"
    );
    assert_eq!(first_output, "stored");

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            ev_custom_tool_call(
                "call-2",
                "exec",
                r#"
text(JSON.stringify(load("nb")));
"#,
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_follow_up = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "loaded"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("load the stored value").await?;

    let second_request = second_follow_up.single_request();
    let (second_output, second_success) =
        custom_tool_output_body_and_success(&second_request, "call-2");
    assert_ne!(
        second_success,
        Some(false),
        "exec load call failed unexpectedly: {second_output}"
    );
    let loaded: Value = serde_json::from_str(
        &custom_tool_output_last_non_empty_text(&second_request, "call-2")
            .expect("exec load call should emit JSON"),
    )?;
    assert_eq!(
        loaded,
        serde_json::json!({ "title": "Notebook", "items": [1, true, null] })
    );

    Ok(())
}
