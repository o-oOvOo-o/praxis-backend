use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use praxis_app_gateway_protocol::FuzzyFileSearchParams;
use praxis_app_gateway_protocol::FuzzyFileSearchResponse;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionStartParams;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionStartResponse;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionStopParams;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionStopResponse;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionUpdateParams;
use praxis_app_gateway_protocol::FuzzyFileSearchSessionUpdateResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;

use super::PraxisMessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::fuzzy_file_search::run_fuzzy_file_search;
use crate::fuzzy_file_search::start_fuzzy_file_search_session;
use crate::outgoing_message::ConnectionRequestId;

impl PraxisMessageProcessor {
    pub(super) async fn fuzzy_file_search(
        &mut self,
        request_id: ConnectionRequestId,
        params: FuzzyFileSearchParams,
    ) {
        let FuzzyFileSearchParams {
            query,
            roots,
            cancellation_token,
        } = params;

        let cancel_flag = match cancellation_token.clone() {
            Some(token) => {
                let mut pending_fuzzy_searches = self.pending_fuzzy_searches.lock().await;
                if let Some(existing) = pending_fuzzy_searches.get(&token) {
                    existing.store(true, Ordering::Relaxed);
                }
                let flag = Arc::new(AtomicBool::new(false));
                pending_fuzzy_searches.insert(token.clone(), flag.clone());
                flag
            }
            None => Arc::new(AtomicBool::new(false)),
        };

        let results = match query.as_str() {
            "" => vec![],
            _ => run_fuzzy_file_search(query, roots, cancel_flag.clone()).await,
        };

        if let Some(token) = cancellation_token {
            let mut pending_fuzzy_searches = self.pending_fuzzy_searches.lock().await;
            if let Some(current_flag) = pending_fuzzy_searches.get(&token)
                && Arc::ptr_eq(current_flag, &cancel_flag)
            {
                pending_fuzzy_searches.remove(&token);
            }
        }

        let response = FuzzyFileSearchResponse { files: results };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn fuzzy_file_search_session_start(
        &mut self,
        request_id: ConnectionRequestId,
        params: FuzzyFileSearchSessionStartParams,
    ) {
        let FuzzyFileSearchSessionStartParams { session_id, roots } = params;
        if session_id.is_empty() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "sessionId must not be empty".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let session =
            start_fuzzy_file_search_session(session_id.clone(), roots, self.outgoing.clone());
        match session {
            Ok(session) => {
                let mut sessions = self.fuzzy_search_sessions.lock().await;
                sessions.insert(session_id, session);
                self.outgoing
                    .send_response(request_id, FuzzyFileSearchSessionStartResponse {})
                    .await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to start fuzzy file search session: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    pub(super) async fn fuzzy_file_search_session_update(
        &mut self,
        request_id: ConnectionRequestId,
        params: FuzzyFileSearchSessionUpdateParams,
    ) {
        let FuzzyFileSearchSessionUpdateParams { session_id, query } = params;
        let found = {
            let sessions = self.fuzzy_search_sessions.lock().await;
            if let Some(session) = sessions.get(&session_id) {
                session.update_query(query);
                true
            } else {
                false
            }
        };
        if !found {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("fuzzy file search session not found: {session_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        self.outgoing
            .send_response(request_id, FuzzyFileSearchSessionUpdateResponse {})
            .await;
    }

    pub(super) async fn fuzzy_file_search_session_stop(
        &mut self,
        request_id: ConnectionRequestId,
        params: FuzzyFileSearchSessionStopParams,
    ) {
        let FuzzyFileSearchSessionStopParams { session_id } = params;
        {
            let mut sessions = self.fuzzy_search_sessions.lock().await;
            sessions.remove(&session_id);
        }

        self.outgoing
            .send_response(request_id, FuzzyFileSearchSessionStopResponse {})
            .await;
    }
}
