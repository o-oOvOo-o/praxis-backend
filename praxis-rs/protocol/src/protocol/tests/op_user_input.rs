use super::*;

#[test]
fn conversation_op_serializes_as_unnested_variants() {
    let audio = Op::RealtimeConversationAudio(ConversationAudioParams {
        frame: RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 24_000,
            num_channels: 1,
            samples_per_channel: Some(480),
            item_id: None,
        },
    });
    let start = Op::RealtimeConversationStart(ConversationStartParams {
        prompt: "be helpful".to_string(),
        session_id: Some("conv_1".to_string()),
    });
    let text = Op::RealtimeConversationText(ConversationTextParams {
        text: "hello".to_string(),
    });
    let close = Op::RealtimeConversationClose;

    assert_eq!(
        serde_json::to_value(&start).unwrap(),
        json!({
            "type": "realtime_conversation_start",
            "prompt": "be helpful",
            "session_id": "conv_1"
        })
    );
    assert_eq!(
        serde_json::to_value(&audio).unwrap(),
        json!({
            "type": "realtime_conversation_audio",
            "frame": {
                "data": "AQID",
                "sample_rate": 24000,
                "num_channels": 1,
                "samples_per_channel": 480
            }
        })
    );
    assert_eq!(
        serde_json::from_value::<Op>(serde_json::to_value(&text).unwrap()).unwrap(),
        text
    );
    assert_eq!(
        serde_json::to_value(&close).unwrap(),
        json!({
            "type": "realtime_conversation_close"
        })
    );
    assert_eq!(
        serde_json::from_value::<Op>(serde_json::to_value(&close).unwrap()).unwrap(),
        close
    );
}

#[test]
fn user_input_serialization_omits_final_output_json_schema_when_none() -> Result<()> {
    let op = Op::UserInput {
        items: Vec::new(),
        final_output_json_schema: None,
    };

    let json_op = serde_json::to_value(op)?;
    assert_eq!(json_op, json!({ "type": "user_input", "items": [] }));

    Ok(())
}

#[test]
fn user_input_deserializes_without_final_output_json_schema_field() -> Result<()> {
    let op: Op = serde_json::from_value(json!({ "type": "user_input", "items": [] }))?;

    assert_eq!(
        op,
        Op::UserInput {
            items: Vec::new(),
            final_output_json_schema: None,
        }
    );

    Ok(())
}

#[test]
fn user_input_serialization_includes_final_output_json_schema_when_some() -> Result<()> {
    let schema = json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" }
        },
        "required": ["answer"],
        "additionalProperties": false
    });
    let op = Op::UserInput {
        items: Vec::new(),
        final_output_json_schema: Some(schema.clone()),
    };

    let json_op = serde_json::to_value(op)?;
    assert_eq!(
        json_op,
        json!({
            "type": "user_input",
            "items": [],
            "final_output_json_schema": schema,
        })
    );

    Ok(())
}

#[test]
fn user_input_text_serializes_empty_text_elements() -> Result<()> {
    let input = UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    };

    let json_input = serde_json::to_value(input)?;
    assert_eq!(
        json_input,
        json!({
            "type": "text",
            "text": "hello",
            "text_elements": [],
        })
    );

    Ok(())
}

#[test]
fn user_message_event_serializes_empty_metadata_vectors() -> Result<()> {
    let event = UserMessageEvent {
        message: "hello".to_string(),
        images: None,
        local_images: Vec::new(),
        text_elements: Vec::new(),
    };

    let json_event = serde_json::to_value(event)?;
    assert_eq!(
        json_event,
        json!({
            "message": "hello",
            "local_images": [],
            "text_elements": [],
        })
    );

    Ok(())
}

#[test]
fn turn_aborted_event_deserializes_without_turn_id() -> Result<()> {
    let event: EventMsg = serde_json::from_value(json!({
        "type": "turn_aborted",
        "reason": "interrupted",
    }))?;

    match event {
        EventMsg::TurnAborted(TurnAbortedEvent { turn_id, reason }) => {
            assert_eq!(turn_id, None);
            assert_eq!(reason, TurnAbortReason::Interrupted);
        }
        _ => panic!("expected turn_aborted event"),
    }

    Ok(())
}

#[test]
fn turn_context_item_deserializes_without_network() -> Result<()> {
    let item: TurnContextItem = serde_json::from_value(json!({
        "cwd": "/tmp",
        "approval_policy": "never",
        "sandbox_policy": { "type": "danger-full-access" },
        "model": "gpt-5",
        "summary": "auto",
    }))?;

    assert_eq!(item.trace_id, None);
    assert_eq!(item.network, None);
    Ok(())
}

#[test]
fn turn_context_item_serializes_network_when_present() -> Result<()> {
    let item = TurnContextItem {
        turn_id: None,
        trace_id: None,
        cwd: PathBuf::from("/tmp"),
        current_date: None,
        timezone: None,
        approval_policy: AskForApproval::Never,
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        network: Some(TurnContextNetworkItem {
            allowed_domains: vec!["api.example.com".to_string()],
            denied_domains: vec!["blocked.example.com".to_string()],
        }),
        model: "gpt-5".to_string(),
        personality: None,
        collaboration_mode: None,
        realtime_active: None,
        effort: None,
        summary: ReasoningSummaryConfig::Auto,
        user_instructions: None,
        developer_instructions: None,
        final_output_json_schema: None,
        truncation_policy: None,
    };

    let value = serde_json::to_value(item)?;
    assert_eq!(
        value["network"],
        json!({
            "allowed_domains": ["api.example.com"],
            "denied_domains": ["blocked.example.com"],
        })
    );
    Ok(())
}
