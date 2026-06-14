use crate::outgoing_message::ClientRequestResult;
use crate::server_request_error::is_turn_transition_server_request_error;
use serde::de::DeserializeOwned;
use std::any::type_name;
use tokio::sync::oneshot;
use tracing::error;

pub(crate) type PendingClientResponse =
    std::result::Result<ClientRequestResult, oneshot::error::RecvError>;

pub(crate) fn try_decode_client_response_or_default<T>(
    response: PendingClientResponse,
    fallback: impl FnOnce() -> T,
) -> Option<T>
where
    T: DeserializeOwned,
{
    match response_value_or_cancel(response) {
        ClientResponseValue::Value(value) => {
            Some(decode_response_value_or_default(value, fallback))
        }
        ClientResponseValue::Fallback => Some(fallback()),
        ClientResponseValue::TurnTransition => None,
    }
}

pub(crate) fn response_value_or_cancel(response: PendingClientResponse) -> ClientResponseValue {
    match response {
        Ok(Ok(value)) => ClientResponseValue::Value(value),
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => {
            ClientResponseValue::TurnTransition
        }
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            ClientResponseValue::Fallback
        }
        Err(err) => {
            error!("request failed: {err:?}");
            ClientResponseValue::Fallback
        }
    }
}

pub(crate) enum ClientResponseValue {
    Value(serde_json::Value),
    Fallback,
    TurnTransition,
}

pub(crate) fn decode_response_value_or_default<T>(
    value: serde_json::Value,
    fallback: impl FnOnce() -> T,
) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value::<T>(value).unwrap_or_else(|err| {
        error!("failed to deserialize {}: {err}", type_name::<T>());
        fallback()
    })
}
