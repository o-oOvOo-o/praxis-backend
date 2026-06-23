use super::*;

#[test]
fn image_data_url_payload_does_not_dominate_message_estimate() {
    let payload = "A".repeat(100_000);
    let image_url = format!("data:image/png;base64,{payload}");
    let image_item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "Here is the screenshot".to_string(),
            },
            ContentItem::InputImage { image_url },
        ],
        end_turn: None,
        phase: None,
    };
    let text_only_item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Here is the screenshot".to_string(),
        }],
        end_turn: None,
        phase: None,
    };

    let raw_len = serde_json::to_string(&image_item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&image_item);
    let expected = raw_len - payload.len() as i64 + RESIZED_IMAGE_BYTES_ESTIMATE;
    let text_only_estimated = estimate_response_item_model_visible_bytes(&text_only_item);

    assert_eq!(estimated, expected);
    assert!(estimated < raw_len);
    assert!(estimated > text_only_estimated);
}

#[test]
fn image_data_url_payload_does_not_dominate_function_call_output_estimate() {
    let payload = "B".repeat(50_000);
    let image_url = format!("data:image/png;base64,{payload}");
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-abc".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputText {
                text: "Screenshot captured".to_string(),
            },
            FunctionCallOutputContentItem::InputImage {
                image_url,
                detail: None,
            },
        ]),
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload.len() as i64 + RESIZED_IMAGE_BYTES_ESTIMATE;

    assert_eq!(estimated, expected);
    assert!(estimated < raw_len);
}

#[test]
fn image_data_url_payload_does_not_dominate_custom_tool_call_output_estimate() {
    let payload = "C".repeat(50_000);
    let image_url = format!("data:image/png;base64,{payload}");
    let item = ResponseItem::CustomToolCallOutput {
        call_id: "call-js-repl".to_string(),
        name: None,
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputText {
                text: "Screenshot captured".to_string(),
            },
            FunctionCallOutputContentItem::InputImage {
                image_url,
                detail: None,
            },
        ]),
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload.len() as i64 + RESIZED_IMAGE_BYTES_ESTIMATE;

    assert_eq!(estimated, expected);
    assert!(estimated < raw_len);
}

#[test]
fn non_base64_image_urls_are_unchanged() {
    let message_item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "https://example.com/foo.png".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    let function_output_item = ResponseItem::FunctionCallOutput {
        call_id: "call-1".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url: "file:///tmp/foo.png".to_string(),
                detail: None,
            },
        ]),
    };

    assert_eq!(
        estimate_response_item_model_visible_bytes(&message_item),
        serde_json::to_string(&message_item).unwrap().len() as i64
    );
    assert_eq!(
        estimate_response_item_model_visible_bytes(&function_output_item),
        serde_json::to_string(&function_output_item).unwrap().len() as i64
    );
}

#[test]
fn data_url_without_base64_marker_is_unchanged() {
    let item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg'/>".to_string(),
        }],
        end_turn: None,
        phase: None,
    };

    assert_eq!(
        estimate_response_item_model_visible_bytes(&item),
        serde_json::to_string(&item).unwrap().len() as i64
    );
}

#[test]
fn non_image_base64_data_url_is_unchanged() {
    let payload = "C".repeat(4_096);
    let image_url = format!("data:application/octet-stream;base64,{payload}");
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-octet".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url,
                detail: None,
            },
        ]),
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);

    assert_eq!(estimated, raw_len);
}

#[test]
fn mixed_case_data_url_markers_are_adjusted() {
    let payload = "F".repeat(1_024);
    let image_url = format!("DATA:image/png;BASE64,{payload}");
    let item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage { image_url }],
        end_turn: None,
        phase: None,
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload.len() as i64 + RESIZED_IMAGE_BYTES_ESTIMATE;

    assert_eq!(estimated, expected);
}

#[test]
fn multiple_inline_images_apply_multiple_fixed_costs() {
    let payload_one = "D".repeat(100);
    let payload_two = "E".repeat(200);
    let image_url_one = format!("data:image/png;base64,{payload_one}");
    let image_url_two = format!("data:image/jpeg;base64,{payload_two}");
    let item = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "images".to_string(),
            },
            ContentItem::InputImage {
                image_url: image_url_one,
            },
            ContentItem::InputImage {
                image_url: image_url_two,
            },
        ],
        end_turn: None,
        phase: None,
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let payload_sum = (payload_one.len() + payload_two.len()) as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload_sum + (2 * RESIZED_IMAGE_BYTES_ESTIMATE);

    assert_eq!(estimated, expected);
}

#[test]
fn original_detail_images_scale_with_dimensions() {
    // 2304x864 at 32px patches yields 72 * 27 = 1,944 patches.
    // The byte heuristic uses 4 bytes per token, so the replacement cost is 7,776 bytes.
    const EXPECTED_ORIGINAL_DETAIL_IMAGE_BYTES: i64 = 7_776;

    let width = 2304;
    let height = 864;
    let image = ImageBuffer::from_pixel(width, height, Rgba([12u8, 34, 56, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode png");
    let payload = BASE64_STANDARD.encode(bytes.get_ref());
    let image_url = format!("data:image/png;base64,{payload}");
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-original".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url,
                detail: Some(ImageDetail::Original),
            },
        ]),
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload.len() as i64 + EXPECTED_ORIGINAL_DETAIL_IMAGE_BYTES;

    assert_eq!(estimated, expected);
}

#[test]
fn original_detail_webp_images_scale_with_dimensions() {
    // Same dimensions as the PNG case above, so the patch-based replacement cost is the same.
    const EXPECTED_ORIGINAL_DETAIL_IMAGE_BYTES: i64 = 7_776;

    let width = 2304;
    let height = 864;
    let image = ImageBuffer::from_pixel(width, height, Rgba([12u8, 34, 56, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ImageFormat::WebP)
        .expect("encode webp");
    let payload = BASE64_STANDARD.encode(bytes.get_ref());
    let image_url = format!("data:image/webp;base64,{payload}");
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-original-webp".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url,
                detail: Some(ImageDetail::Original),
            },
        ]),
    };

    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;
    let estimated = estimate_response_item_model_visible_bytes(&item);
    let expected = raw_len - payload.len() as i64 + EXPECTED_ORIGINAL_DETAIL_IMAGE_BYTES;

    assert_eq!(estimated, expected);
}

#[test]
fn text_only_items_unchanged() {
    let item = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "Hello world, this is a response.".to_string(),
        }],
        end_turn: None,
        phase: None,
    };

    let estimated = estimate_response_item_model_visible_bytes(&item);
    let raw_len = serde_json::to_string(&item).unwrap().len() as i64;

    assert_eq!(estimated, raw_len);
}
