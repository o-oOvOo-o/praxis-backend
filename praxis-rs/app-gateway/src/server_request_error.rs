use praxis_app_gateway_protocol::JSONRPCErrorError;

pub(crate) const TURN_TRANSITION_PENDING_REQUEST_ERROR_REASON: &str = "turnTransition";
pub(crate) const PERMISSION_CHANGED_PENDING_REQUEST_ERROR_REASON: &str = "permissionChanged";

pub(crate) fn is_server_request_lifecycle_resolution_error(error: &JSONRPCErrorError) -> bool {
    matches!(
        error
            .data
            .as_ref()
            .and_then(|data| data.get("reason"))
            .and_then(serde_json::Value::as_str),
        Some(TURN_TRANSITION_PENDING_REQUEST_ERROR_REASON)
            | Some(PERMISSION_CHANGED_PENDING_REQUEST_ERROR_REASON)
    )
}

#[cfg(test)]
mod tests {
    use super::is_server_request_lifecycle_resolution_error;
    use praxis_app_gateway_protocol::JSONRPCErrorError;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn turn_transition_error_is_detected() {
        let error = JSONRPCErrorError {
            code: -1,
            message: "client request resolved because the turn state was changed".to_string(),
            data: Some(json!({ "reason": "turnTransition" })),
        };

        assert_eq!(is_server_request_lifecycle_resolution_error(&error), true);
    }

    #[test]
    fn unrelated_error_is_not_detected() {
        let error = JSONRPCErrorError {
            code: -1,
            message: "boom".to_string(),
            data: Some(json!({ "reason": "other" })),
        };

        assert_eq!(is_server_request_lifecycle_resolution_error(&error), false);
    }

    #[test]
    fn permission_change_resolution_is_detected() {
        let error = JSONRPCErrorError {
            code: -1,
            message: "approval resolved because permissions changed".to_string(),
            data: Some(json!({ "reason": "permissionChanged" })),
        };

        assert!(is_server_request_lifecycle_resolution_error(&error));
    }
}
