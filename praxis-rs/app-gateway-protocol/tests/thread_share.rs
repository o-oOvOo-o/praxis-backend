use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ThreadShareParams;
use praxis_app_gateway_protocol::ThreadShareResponse;
use serde_json::json;

#[test]
fn thread_share_request_and_response_use_the_public_json_rpc_contract() {
    let request = ClientRequest::ThreadShare {
        request_id: RequestId::Integer(73),
        params: ThreadShareParams {
            thread_id: "019fd67a-9190-7762-aa53-1097faf6b07f".to_owned(),
            team: "Cunning3D Core".to_owned(),
        },
    };
    assert_eq!(
        serde_json::to_value(request).expect("serialize thread/share request"),
        json!({
            "method": "thread/share",
            "id": 73,
            "params": {
                "threadId": "019fd67a-9190-7762-aa53-1097faf6b07f",
                "team": "Cunning3D Core"
            }
        }),
    );

    let response: ThreadShareResponse = serde_json::from_value(json!({
        "threadId": "019fd67a-9190-7762-aa53-1097faf6b07f",
        "project": "Cunning3D/Cunning3D-Dev",
        "team": "Cunning3D Core",
        "messageCount": 1928,
        "redactionCount": 27,
        "commit": "cffbc79",
        "webUrl": "https://github.com/o-oOvOo-o/praxis-threads/blob/main/thread.json"
    }))
    .expect("deserialize thread/share response");
    assert_eq!(response.team, "Cunning3D Core");
    assert_eq!(response.message_count, 1928);
    assert_eq!(response.redaction_count, 27);
}
