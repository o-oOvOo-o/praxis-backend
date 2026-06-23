use super::*;

#[test]
fn parse_session_updated_event() {
    let payload = json!({
        "type": "session.updated",
        "session": {"id": "sess_123", "instructions": "backend prompt"}
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::SessionUpdated {
            session_id: "sess_123".to_string(),
            instructions: Some("backend prompt".to_string()),
        })
    );
}

#[test]
fn parse_audio_delta_event() {
    let payload = json!({
        "type": "conversation.output_audio.delta",
        "delta": "AAA=",
        "sample_rate": 48000,
        "channels": 1,
        "samples_per_channel": 960
    })
    .to_string();
    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::AudioOut(RealtimeAudioFrame {
            data: "AAA=".to_string(),
            sample_rate: 48000,
            num_channels: 1,
            samples_per_channel: Some(960),
            item_id: None,
        }))
    );
}

#[test]
fn parse_conversation_item_added_event() {
    let payload = json!({
        "type": "conversation.item.added",
        "item": {"type": "message", "seq": 7}
    })
    .to_string();
    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::ConversationItemAdded(
            json!({"type": "message", "seq": 7})
        ))
    );
}

#[test]
fn parse_conversation_item_done_event() {
    let payload = json!({
        "type": "conversation.item.done",
        "item": {"id": "item_123", "type": "message"}
    })
    .to_string();
    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::ConversationItemDone {
            item_id: "item_123".to_string(),
        })
    );
}

#[test]
fn parse_handoff_requested_event() {
    let payload = json!({
        "type": "conversation.handoff.requested",
        "handoff_id": "handoff_123",
        "item_id": "item_123",
        "input_transcript": "delegate this"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::HandoffRequested(RealtimeHandoffRequested {
            handoff_id: "handoff_123".to_string(),
            item_id: "item_123".to_string(),
            input_transcript: "delegate this".to_string(),
            active_transcript: Vec::new(),
        }))
    );
}

#[test]
fn parse_input_transcript_delta_event() {
    let payload = json!({
        "type": "conversation.input_transcript.delta",
        "delta": "hello "
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::InputTranscriptDelta(
            RealtimeTranscriptDelta {
                delta: "hello ".to_string(),
            }
        ))
    );
}

#[test]
fn parse_output_transcript_delta_event() {
    let payload = json!({
        "type": "conversation.output_transcript.delta",
        "delta": "hi"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::V1),
        Some(RealtimeEvent::OutputTranscriptDelta(
            RealtimeTranscriptDelta {
                delta: "hi".to_string(),
            }
        ))
    );
}

#[test]
fn parse_realtime_v2_handoff_tool_call_event() {
    let payload = json!({
        "type": "conversation.item.done",
        "item": {
            "id": "item_123",
            "type": "function_call",
            "name": "praxis",
            "call_id": "call_123",
            "arguments": "{\"prompt\":\"delegate this\"}"
        }
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::HandoffRequested(RealtimeHandoffRequested {
            handoff_id: "call_123".to_string(),
            item_id: "item_123".to_string(),
            input_transcript: "delegate this".to_string(),
            active_transcript: Vec::new(),
        }))
    );
}

#[test]
fn parse_realtime_v2_input_audio_transcription_delta_event() {
    let payload = json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "delta": "hello"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::InputTranscriptDelta(
            RealtimeTranscriptDelta {
                delta: "hello".to_string(),
            }
        ))
    );
}

#[test]
fn parse_realtime_v2_output_audio_delta_defaults_audio_shape() {
    let payload = json!({
        "type": "response.output_audio.delta",
        "delta": "AQID"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::AudioOut(RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 24_000,
            num_channels: 1,
            samples_per_channel: None,
            item_id: None,
        }))
    );
}

#[test]
fn parse_realtime_v2_response_audio_delta_with_item_id() {
    let payload = json!({
        "type": "response.audio.delta",
        "delta": "AQID",
        "item_id": "item_audio_1"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::AudioOut(RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 24_000,
            num_channels: 1,
            samples_per_channel: None,
            item_id: Some("item_audio_1".to_string()),
        }))
    );
}

#[test]
fn parse_realtime_v2_speech_started_event() {
    let payload = json!({
        "type": "input_audio_buffer.speech_started",
        "item_id": "item_input_1"
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::InputAudioSpeechStarted(
            RealtimeInputAudioSpeechStarted {
                item_id: Some("item_input_1".to_string()),
            }
        ))
    );
}

#[test]
fn parse_realtime_v2_response_cancelled_event() {
    let payload = json!({
        "type": "response.cancelled",
        "response": {"id": "resp_cancelled_1"}
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::ResponseCancelled(
            RealtimeResponseCancelled {
                response_id: Some("resp_cancelled_1".to_string()),
            }
        ))
    );
}

#[test]
fn parse_realtime_v2_response_done_handoff_event() {
    let payload = json!({
        "type": "response.done",
        "response": {
            "output": [{
                "id": "item_123",
                "type": "function_call",
                "name": "praxis",
                "call_id": "call_123",
                "arguments": "{\"prompt\":\"delegate from done\"}"
            }]
        }
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::HandoffRequested(RealtimeHandoffRequested {
            handoff_id: "call_123".to_string(),
            item_id: "item_123".to_string(),
            input_transcript: "delegate from done".to_string(),
            active_transcript: Vec::new(),
        }))
    );
}

#[test]
fn parse_realtime_v2_response_created_event() {
    let payload = json!({
        "type": "response.created",
        "response": {"id": "resp_created_1"}
    })
    .to_string();

    assert_eq!(
        parse_realtime_event(payload.as_str(), RealtimeEventParser::RealtimeV2),
        Some(RealtimeEvent::ConversationItemAdded(json!({
            "type": "response.created",
            "response": {"id": "resp_created_1"}
        })))
    );
}

#[test]
fn merge_request_headers_matches_http_precedence() {
    let mut provider_headers = HeaderMap::new();
    provider_headers.insert(
        "originator",
        HeaderValue::from_static("provider-originator"),
    );
    provider_headers.insert("x-priority", HeaderValue::from_static("provider"));

    let mut extra_headers = HeaderMap::new();
    extra_headers.insert("x-priority", HeaderValue::from_static("extra"));

    let mut default_headers = HeaderMap::new();
    default_headers.insert("originator", HeaderValue::from_static("default-originator"));
    default_headers.insert("x-priority", HeaderValue::from_static("default"));
    default_headers.insert("x-default-only", HeaderValue::from_static("default-only"));

    let merged = merge_request_headers(&provider_headers, extra_headers, default_headers);

    assert_eq!(
        merged.get("originator"),
        Some(&HeaderValue::from_static("provider-originator"))
    );
    assert_eq!(
        merged.get("x-priority"),
        Some(&HeaderValue::from_static("extra"))
    );
    assert_eq!(
        merged.get("x-default-only"),
        Some(&HeaderValue::from_static("default-only"))
    );
}

#[test]
fn websocket_url_from_http_base_defaults_to_ws_path() {
    let url = websocket_url_from_api_url(
        "http://127.0.0.1:8011",
        /*query_params*/ None,
        /*model*/ None,
        RealtimeEventParser::V1,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "ws://127.0.0.1:8011/v1/realtime?intent=quicksilver"
    );
}

#[test]
fn websocket_url_from_ws_base_defaults_to_ws_path() {
    let url = websocket_url_from_api_url(
        "wss://example.com",
        /*query_params*/ None,
        Some("realtime-test-model"),
        RealtimeEventParser::V1,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://example.com/v1/realtime?intent=quicksilver&model=realtime-test-model"
    );
}

#[test]
fn websocket_url_from_v1_base_appends_realtime_path() {
    let url = websocket_url_from_api_url(
        "https://api.openai.com/v1",
        /*query_params*/ None,
        Some("snapshot"),
        RealtimeEventParser::V1,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://api.openai.com/v1/realtime?intent=quicksilver&model=snapshot"
    );
}

#[test]
fn websocket_url_from_nested_v1_base_appends_realtime_path() {
    let url = websocket_url_from_api_url(
        "https://example.com/openai/v1",
        /*query_params*/ None,
        Some("snapshot"),
        RealtimeEventParser::V1,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://example.com/openai/v1/realtime?intent=quicksilver&model=snapshot"
    );
}

#[test]
fn websocket_url_preserves_existing_realtime_path_and_extra_query_params() {
    let url = websocket_url_from_api_url(
        "https://example.com/v1/realtime?foo=bar",
        Some(&HashMap::from([
            ("trace".to_string(), "1".to_string()),
            ("intent".to_string(), "ignored".to_string()),
        ])),
        Some("snapshot"),
        RealtimeEventParser::V1,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://example.com/v1/realtime?foo=bar&intent=quicksilver&model=snapshot&trace=1"
    );
}

#[test]
fn websocket_url_v1_ignores_transcription_mode() {
    let url = websocket_url_from_api_url(
        "https://example.com",
        /*query_params*/ None,
        /*model*/ None,
        RealtimeEventParser::V1,
        RealtimeSessionMode::Transcription,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://example.com/v1/realtime?intent=quicksilver"
    );
}

#[test]
fn websocket_url_omits_intent_for_realtime_v2_conversational_mode() {
    let url = websocket_url_from_api_url(
        "https://example.com/v1/realtime?foo=bar",
        Some(&HashMap::from([
            ("trace".to_string(), "1".to_string()),
            ("intent".to_string(), "ignored".to_string()),
        ])),
        Some("snapshot"),
        RealtimeEventParser::RealtimeV2,
        RealtimeSessionMode::Conversational,
    )
    .expect("build ws url");
    assert_eq!(
        url.as_str(),
        "wss://example.com/v1/realtime?foo=bar&model=snapshot&trace=1"
    );
}

#[test]
fn websocket_url_omits_intent_for_realtime_v2_transcription_mode() {
    let url = websocket_url_from_api_url(
        "https://example.com",
        /*query_params*/ None,
        /*model*/ None,
        RealtimeEventParser::RealtimeV2,
        RealtimeSessionMode::Transcription,
    )
    .expect("build ws url");
    assert_eq!(url.as_str(), "wss://example.com/v1/realtime");
}
