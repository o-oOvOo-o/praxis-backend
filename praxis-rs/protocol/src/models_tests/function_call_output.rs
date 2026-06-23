use super::*;

#[test]
fn serializes_success_as_plain_string() -> Result<()> {
    let item = ResponseInputItem::FunctionCallOutput {
        call_id: "call1".into(),
        output: FunctionCallOutputPayload::from_text("ok".into()),
    };

    let json = serde_json::to_string(&item)?;
    let v: serde_json::Value = serde_json::from_str(&json)?;

    // Success case -> output should be a plain string
    assert_eq!(v.get("output").unwrap().as_str().unwrap(), "ok");
    Ok(())
}

#[test]
fn serializes_failure_as_string() -> Result<()> {
    let item = ResponseInputItem::FunctionCallOutput {
        call_id: "call1".into(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("bad".into()),
            success: Some(false),
        },
    };

    let json = serde_json::to_string(&item)?;
    let v: serde_json::Value = serde_json::from_str(&json)?;

    assert_eq!(v.get("output").unwrap().as_str().unwrap(), "bad");
    Ok(())
}

#[test]
fn serializes_image_outputs_as_array() -> Result<()> {
    let call_tool_result = CallToolResult {
        content: vec![
            serde_json::json!({"type":"text","text":"caption"}),
            serde_json::json!({"type":"image","data":"BASE64","mimeType":"image/png"}),
        ],
        structured_content: None,
        is_error: Some(false),
        meta: None,
    };

    let payload = call_tool_result.into_function_call_output_payload();
    assert_eq!(payload.success, Some(true));
    let Some(items) = payload.content_items() else {
        panic!("expected content items");
    };
    let items = items.to_vec();
    assert_eq!(
        items,
        vec![
            FunctionCallOutputContentItem::InputText {
                text: "caption".into(),
            },
            FunctionCallOutputContentItem::InputImage {
                image_url: "data:image/png;base64,BASE64".into(),
                detail: None,
            },
        ]
    );

    let item = ResponseInputItem::FunctionCallOutput {
        call_id: "call1".into(),
        output: payload,
    };

    let json = serde_json::to_string(&item)?;
    let v: serde_json::Value = serde_json::from_str(&json)?;

    let output = v.get("output").expect("output field");
    assert!(output.is_array(), "expected array output");

    Ok(())
}

#[test]
fn serializes_custom_tool_image_outputs_as_array() -> Result<()> {
    let item = ResponseInputItem::CustomToolCallOutput {
        call_id: "call1".into(),
        name: None,
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url: "data:image/png;base64,BASE64".into(),
                detail: None,
            },
        ]),
    };

    let json = serde_json::to_string(&item)?;
    let v: serde_json::Value = serde_json::from_str(&json)?;

    let output = v.get("output").expect("output field");
    assert!(output.is_array(), "expected array output");

    Ok(())
}

#[test]
fn preserves_existing_image_data_urls() -> Result<()> {
    let call_tool_result = CallToolResult {
        content: vec![serde_json::json!({
            "type": "image",
            "data": "data:image/png;base64,BASE64",
            "mimeType": "image/png"
        })],
        structured_content: None,
        is_error: Some(false),
        meta: None,
    };

    let payload = call_tool_result.into_function_call_output_payload();
    let Some(items) = payload.content_items() else {
        panic!("expected content items");
    };
    let items = items.to_vec();
    assert_eq!(
        items,
        vec![FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,BASE64".into(),
            detail: None,
        }]
    );

    Ok(())
}

#[test]
fn deserializes_array_payload_into_items() -> Result<()> {
    let json = r#"[
        {"type": "input_text", "text": "note"},
        {"type": "input_image", "image_url": "data:image/png;base64,XYZ"}
    ]"#;

    let payload: FunctionCallOutputPayload = serde_json::from_str(json)?;

    assert_eq!(payload.success, None);
    let expected_items = vec![
        FunctionCallOutputContentItem::InputText {
            text: "note".into(),
        },
        FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,XYZ".into(),
            detail: None,
        },
    ];
    assert_eq!(
        payload.body,
        FunctionCallOutputBody::ContentItems(expected_items.clone())
    );
    assert_eq!(
        serde_json::to_string(&payload)?,
        serde_json::to_string(&expected_items)?
    );

    Ok(())
}
