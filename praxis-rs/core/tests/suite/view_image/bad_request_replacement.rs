#![cfg(not(target_os = "windows"))]
use super::*;

#[cfg(not(debug_assertions))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replaces_invalid_local_image_after_bad_request() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    const INVALID_IMAGE_ERROR: &str =
        "The image data you provided does not represent a valid image";

    let invalid_image_mock = responses::mount_response_once_match(
        &server,
        body_string_contains("\"input_image\""),
        ResponseTemplate::new(400)
            .insert_header("content-type", "text/plain")
            .set_body_string(INVALID_IMAGE_ERROR),
    )
    .await;

    let success_response = sse(vec![
        ev_response_created("resp-2"),
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);

    let completion_mock = responses::mount_sse_once(&server, success_response).await;

    let mut builder = test_praxis();
    let test = builder.build_remote_aware(&server).await?;
    let TestPraxis {
        thread: praxis,
        config,
        session_configured,
        ..
    } = &test;

    let rel_path = "assets/poisoned.png";
    let abs_path = write_workspace_png(&test, rel_path, 1024, 512, [10u8, 20, 30, 255]).await?;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::LocalImage {
                path: abs_path.clone(),
            }],
            final_output_json_schema: None,
            cwd: config.cwd.to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let first_body = invalid_image_mock.single_request().body_json();
    assert!(
        find_image_message(&first_body).is_some(),
        "initial request should include the uploaded image"
    );

    let second_request = completion_mock.single_request();
    let second_body = second_request.body_json();
    assert!(
        find_image_message(&second_body).is_none(),
        "second request should replace the invalid image"
    );
    let user_texts = second_request.message_input_texts("user");
    assert!(user_texts.iter().any(|text| text == "Invalid image"));

    Ok(())
}
