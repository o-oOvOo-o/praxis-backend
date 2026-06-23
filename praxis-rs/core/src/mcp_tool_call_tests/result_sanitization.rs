use super::*;

#[test]
fn sanitize_mcp_tool_result_for_model_rewrites_image_content() {
    let result = Ok(CallToolResult {
        content: vec![
            serde_json::json!({
                "type": "image",
                "data": "Zm9v",
                "mimeType": "image/png",
            }),
            serde_json::json!({
                "type": "text",
                "text": "hello",
            }),
        ],
        structured_content: None,
        is_error: Some(false),
        meta: None,
    });

    let got = sanitize_mcp_tool_result_for_model(/*supports_image_input*/ false, result)
        .expect("sanitized result");

    assert_eq!(
        got.content,
        vec![
            serde_json::json!({
                "type": "text",
                "text": "<image content omitted because you do not support image input>",
            }),
            serde_json::json!({
                "type": "text",
                "text": "hello",
            }),
        ]
    );
}

#[test]
fn sanitize_mcp_tool_result_for_model_preserves_image_when_supported() {
    let original = CallToolResult {
        content: vec![serde_json::json!({
            "type": "image",
            "data": "Zm9v",
            "mimeType": "image/png",
        })],
        structured_content: Some(serde_json::json!({"x": 1})),
        is_error: Some(false),
        meta: Some(serde_json::json!({"k": "v"})),
    };

    let got = sanitize_mcp_tool_result_for_model(
        /*supports_image_input*/ true,
        Ok(original.clone()),
    )
    .expect("unsanitized result");

    assert_eq!(got, original);
}
