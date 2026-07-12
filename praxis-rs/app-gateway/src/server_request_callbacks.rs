use crate::error_code::INTERNAL_ERROR_CODE;
use crate::outgoing_message::ClientRequestResult;
use crate::outgoing_message::ConnectionId;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::Result;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_protocol::ThreadId;
use std::collections::HashMap;
use std::collections::HashSet;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResponseConnectionScope {
    Any,
    Connections(HashSet<ConnectionId>),
}

impl ResponseConnectionScope {
    pub(crate) fn connections(connection_ids: impl IntoIterator<Item = ConnectionId>) -> Self {
        Self::Connections(connection_ids.into_iter().collect())
    }

    fn allows(&self, connection_id: ConnectionId) -> bool {
        match self {
            Self::Any => true,
            Self::Connections(connection_ids) => connection_ids.contains(&connection_id),
        }
    }

    fn remove_connection(&mut self, connection_id: ConnectionId) -> bool {
        match self {
            Self::Any => false,
            Self::Connections(connection_ids) => {
                connection_ids.remove(&connection_id) && connection_ids.is_empty()
            }
        }
    }

    fn remove_closed_connections(&mut self, closed_connections: &HashSet<ConnectionId>) -> bool {
        match self {
            Self::Any => false,
            Self::Connections(connection_ids) => {
                let had_connections = !connection_ids.is_empty();
                connection_ids.retain(|connection_id| !closed_connections.contains(connection_id));
                had_connections && connection_ids.is_empty()
            }
        }
    }
}

struct PendingCallbackEntry {
    callback: oneshot::Sender<ClientRequestResult>,
    thread_id: Option<ThreadId>,
    #[cfg(test)]
    request: ServerRequest,
    response_scope: ResponseConnectionScope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientResponseDisposition {
    Delivered,
    UnknownRequest,
    WrongConnection,
    WaiterDropped,
}

#[derive(Default)]
struct ServerRequestCallbackState {
    entries: HashMap<RequestId, PendingCallbackEntry>,
    closed_connections: HashSet<ConnectionId>,
}

#[derive(Default)]
pub(crate) struct ServerRequestCallbackRegistry {
    state: Mutex<ServerRequestCallbackState>,
}

pub(crate) fn controller_connection_closed_error() -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: "server request cancelled because its controlling connection closed".to_string(),
        data: Some(serde_json::json!({
            "reason": "controllerConnectionClosed",
        })),
    }
}

impl ServerRequestCallbackRegistry {
    pub(crate) async fn insert(
        &self,
        request: ServerRequest,
        callback: oneshot::Sender<ClientRequestResult>,
        thread_id: Option<ThreadId>,
        response_scope: ResponseConnectionScope,
    ) {
        let request_id = request.id().clone();
        let mut entry = PendingCallbackEntry {
            callback,
            thread_id,
            #[cfg(test)]
            request,
            response_scope,
        };
        let (replaced, rejected) = {
            let mut state = self.state.lock().await;
            if entry
                .response_scope
                .remove_closed_connections(&state.closed_connections)
            {
                (None, Some(entry))
            } else {
                (state.entries.insert(request_id, entry), None)
            }
        };
        debug_assert!(replaced.is_none(), "server request ids must be unique");
        if let Some(entry) = rejected {
            let _ = entry
                .callback
                .send(Err(controller_connection_closed_error()));
        }
    }

    pub(crate) async fn notify_response(
        &self,
        connection_id: ConnectionId,
        request_id: &RequestId,
        result: Result,
    ) -> ClientResponseDisposition {
        let Some(entry) = self.take_for_connection(connection_id, request_id).await else {
            return self
                .disposition_for_missing_or_wrong_connection(connection_id, request_id)
                .await;
        };
        if entry.callback.send(Ok(result)).is_ok() {
            ClientResponseDisposition::Delivered
        } else {
            ClientResponseDisposition::WaiterDropped
        }
    }

    pub(crate) async fn notify_error(
        &self,
        connection_id: ConnectionId,
        request_id: &RequestId,
        error: JSONRPCErrorError,
    ) -> ClientResponseDisposition {
        let Some(entry) = self.take_for_connection(connection_id, request_id).await else {
            return self
                .disposition_for_missing_or_wrong_connection(connection_id, request_id)
                .await;
        };
        if entry.callback.send(Err(error)).is_ok() {
            ClientResponseDisposition::Delivered
        } else {
            ClientResponseDisposition::WaiterDropped
        }
    }

    async fn take_for_connection(
        &self,
        connection_id: ConnectionId,
        request_id: &RequestId,
    ) -> Option<PendingCallbackEntry> {
        let mut state = self.state.lock().await;
        if !state
            .entries
            .get(request_id)
            .is_some_and(|entry| entry.response_scope.allows(connection_id))
        {
            return None;
        }
        state.entries.remove(request_id)
    }

    async fn disposition_for_missing_or_wrong_connection(
        &self,
        connection_id: ConnectionId,
        request_id: &RequestId,
    ) -> ClientResponseDisposition {
        let state = self.state.lock().await;
        match state.entries.get(request_id) {
            Some(entry) if !entry.response_scope.allows(connection_id) => {
                ClientResponseDisposition::WrongConnection
            }
            Some(_) => unreachable!("authorized callback must have been removed"),
            None => ClientResponseDisposition::UnknownRequest,
        }
    }

    pub(crate) async fn cancel(&self, request_id: &RequestId) -> bool {
        self.state.lock().await.entries.remove(request_id).is_some()
    }

    pub(crate) async fn fail(&self, request_id: &RequestId, error: JSONRPCErrorError) -> bool {
        let entry = self.state.lock().await.entries.remove(request_id);
        let Some(entry) = entry else {
            return false;
        };
        let _ = entry.callback.send(Err(error));
        true
    }

    pub(crate) async fn fail_connection(
        &self,
        connection_id: ConnectionId,
        error: JSONRPCErrorError,
    ) -> Vec<RequestId> {
        let entries = {
            let mut state = self.state.lock().await;
            state.closed_connections.insert(connection_id);
            let request_ids = state
                .entries
                .iter_mut()
                .filter_map(|(request_id, entry)| {
                    entry
                        .response_scope
                        .remove_connection(connection_id)
                        .then_some(request_id.clone())
                })
                .collect::<Vec<_>>();
            request_ids
                .into_iter()
                .filter_map(|request_id| {
                    state
                        .entries
                        .remove(&request_id)
                        .map(|entry| (request_id, entry))
                })
                .collect::<Vec<_>>()
        };

        let mut failed_request_ids = Vec::with_capacity(entries.len());
        for (request_id, entry) in entries {
            let _ = entry.callback.send(Err(error.clone()));
            failed_request_ids.push(request_id);
        }
        failed_request_ids
    }

    pub(crate) async fn fail_all(&self, error: Option<JSONRPCErrorError>) {
        let entries = {
            let mut state = self.state.lock().await;
            state
                .entries
                .drain()
                .map(|(_, entry)| entry)
                .collect::<Vec<_>>()
        };
        if let Some(error) = error {
            for entry in entries {
                let _ = entry.callback.send(Err(error.clone()));
            }
        }
    }

    #[cfg(test)]
    pub(crate) async fn pending_requests_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Vec<ServerRequest> {
        let state = self.state.lock().await;
        let mut requests = state
            .entries
            .values()
            .filter_map(|entry| {
                (entry.thread_id == Some(thread_id)).then_some(entry.request.clone())
            })
            .collect::<Vec<_>>();
        requests.sort_by(|left, right| left.id().cmp(right.id()));
        requests
    }

    pub(crate) async fn is_response_allowed(
        &self,
        request_id: &RequestId,
        connection_id: ConnectionId,
    ) -> bool {
        self.state
            .lock()
            .await
            .entries
            .get(request_id)
            .is_some_and(|entry| entry.response_scope.allows(connection_id))
    }

    pub(crate) async fn fail_thread(&self, thread_id: ThreadId, error: Option<JSONRPCErrorError>) {
        let entries = {
            let mut state = self.state.lock().await;
            let request_ids = state
                .entries
                .iter()
                .filter_map(|(request_id, entry)| {
                    (entry.thread_id == Some(thread_id)).then_some(request_id.clone())
                })
                .collect::<Vec<_>>();
            request_ids
                .into_iter()
                .filter_map(|request_id| state.entries.remove(&request_id))
                .collect::<Vec<_>>()
        };
        if let Some(error) = error {
            for entry in entries {
                let _ = entry.callback.send(Err(error.clone()));
            }
        }
    }
}
