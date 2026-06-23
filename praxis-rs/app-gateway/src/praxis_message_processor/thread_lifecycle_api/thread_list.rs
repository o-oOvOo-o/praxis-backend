use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;
use crate::praxis_message_processor::PraxisMessageProcessor;
use crate::praxis_message_processor::thread_projection_api::summary_to_thread;
use crate::praxis_message_processor::thread_store_api::ThreadStore;
use crate::praxis_message_processor::thread_store_api::ThreadStoreListQuery;
use crate::praxis_message_processor::thread_store_api::ThreadStoreSummary;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::THREAD_LIST_DEFAULT_LIMIT;
use praxis_app_gateway_protocol::THREAD_LIST_MAX_LIMIT;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadLoadedListResponse;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind as ApiThreadSourceKind;
use praxis_core::Cursor as RolloutCursor;
use praxis_core::ThreadSortKey as CoreThreadSortKey;
use praxis_core::parse_cursor;
use praxis_protocol::ThreadId;
use std::path::Path;
use std::path::PathBuf;

use super::project_scope_root;

pub(super) struct ThreadListFilters {
    pub(super) model_providers: Option<Vec<String>>,
    pub(super) source_kinds: Option<Vec<ApiThreadSourceKind>>,
    pub(super) archived: bool,
    pub(super) cwd: Option<PathBuf>,
    pub(super) cwd_scope: Option<PathBuf>,
    pub(super) search_term: Option<String>,
}

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_list(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadListParams,
    ) {
        let ThreadListParams {
            cursor,
            limit,
            sort_key,
            model_providers,
            source_kinds,
            archived,
            cwd,
            cwd_scope,
            search_term,
        } = params;

        let requested_page_size = list_page_size(limit);
        let core_sort_key = match sort_key.unwrap_or(ThreadSortKey::CreatedAt) {
            ThreadSortKey::CreatedAt => CoreThreadSortKey::CreatedAt,
            ThreadSortKey::UpdatedAt => CoreThreadSortKey::UpdatedAt,
        };
        let (summaries, next_cursor) = match self
            .list_threads_common(
                requested_page_size,
                cursor,
                core_sort_key,
                ThreadListFilters {
                    model_providers,
                    source_kinds,
                    archived: archived.unwrap_or(false),
                    cwd: cwd.map(PathBuf::from),
                    cwd_scope: cwd_scope.as_deref().map(Path::new).map(project_scope_root),
                    search_term,
                },
            )
            .await
        {
            Ok(r) => r,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let threads: Vec<Thread> = summaries.into_iter().map(summary_to_thread).collect();
        let data = self.project_thread_runtime_states(threads).await;
        let response = ThreadListResponse { data, next_cursor };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(in crate::praxis_message_processor) async fn thread_loaded_list(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadLoadedListParams,
    ) {
        let ThreadLoadedListParams { cursor, limit } = params;
        let mut data = self
            .thread_manager
            .list_thread_ids()
            .await
            .into_iter()
            .map(|thread_id| thread_id.to_string())
            .collect::<Vec<_>>();

        if data.is_empty() {
            let response = ThreadLoadedListResponse {
                data,
                next_cursor: None,
            };
            self.outgoing.send_response(request_id, response).await;
            return;
        }

        data.sort();
        let total = data.len();
        let start = match loaded_threads_page_start(cursor, &data) {
            Ok(start) => start,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let effective_limit = list_page_size(limit);
        let end = start.saturating_add(effective_limit).min(total);
        let page = data[start..end].to_vec();
        let next_cursor = page.last().filter(|_| end < total).cloned();

        let response = ThreadLoadedListResponse {
            data: page,
            next_cursor,
        };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(super) async fn list_threads_common(
        &self,
        requested_page_size: usize,
        cursor: Option<String>,
        sort_key: CoreThreadSortKey,
        filters: ThreadListFilters,
    ) -> Result<(Vec<ThreadStoreSummary>, Option<String>), JSONRPCErrorError> {
        let ThreadListFilters {
            model_providers,
            source_kinds,
            archived,
            cwd,
            cwd_scope,
            search_term,
        } = filters;
        let page = ThreadStore::new(&self.config)
            .list_summaries(ThreadStoreListQuery {
                page_size: requested_page_size.min(THREAD_LIST_MAX_LIMIT as usize),
                cursor: parse_thread_list_cursor(cursor)?,
                sort_key,
                model_providers: non_empty_model_provider_filter(model_providers),
                source_kinds: thread_store_source_kinds(source_kinds),
                archived,
                cwd: cwd_scope.or(cwd),
                search_term,
                fallback_provider: self.config.model_provider_id.clone(),
            })
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to list threads: {err}"),
                data: None,
            })?;

        Ok((page.items, next_cursor_string(page.next_cursor.as_ref())))
    }
}

fn list_page_size(limit: Option<u32>) -> usize {
    limit
        .unwrap_or(THREAD_LIST_DEFAULT_LIMIT)
        .clamp(1, THREAD_LIST_MAX_LIMIT) as usize
}

fn loaded_threads_page_start(
    cursor: Option<String>,
    sorted_thread_ids: &[String],
) -> Result<usize, JSONRPCErrorError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let cursor = ThreadId::from_string(&cursor)
        .map(|id| id.to_string())
        .map_err(|_| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("invalid cursor: {cursor}"),
            data: None,
        })?;
    Ok(match sorted_thread_ids.binary_search(&cursor) {
        Ok(idx) => idx + 1,
        Err(idx) => idx,
    })
}

fn parse_thread_list_cursor(
    cursor: Option<String>,
) -> Result<Option<RolloutCursor>, JSONRPCErrorError> {
    let Some(cursor_str) = cursor.as_ref() else {
        return Ok(None);
    };
    parse_cursor(cursor_str)
        .map(Some)
        .ok_or_else(|| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("invalid cursor: {cursor_str}"),
            data: None,
        })
}

fn non_empty_model_provider_filter(model_providers: Option<Vec<String>>) -> Option<Vec<String>> {
    model_providers.and_then(|providers| (!providers.is_empty()).then_some(providers))
}

fn thread_store_source_kinds(
    source_kinds: Option<Vec<ApiThreadSourceKind>>,
) -> Option<Vec<praxis_rollout::ThreadSourceKind>> {
    source_kinds.map(|kinds| kinds.into_iter().map(map_thread_source_kind).collect())
}

fn map_thread_source_kind(kind: ApiThreadSourceKind) -> praxis_rollout::ThreadSourceKind {
    match kind {
        ApiThreadSourceKind::Cli => praxis_rollout::ThreadSourceKind::Cli,
        ApiThreadSourceKind::VsCode => praxis_rollout::ThreadSourceKind::VsCode,
        ApiThreadSourceKind::Exec => praxis_rollout::ThreadSourceKind::Exec,
        ApiThreadSourceKind::AppGateway => praxis_rollout::ThreadSourceKind::AppGateway,
        ApiThreadSourceKind::SubAgent => praxis_rollout::ThreadSourceKind::SubAgent,
        ApiThreadSourceKind::SubAgentReview => praxis_rollout::ThreadSourceKind::SubAgentReview,
        ApiThreadSourceKind::SubAgentCompact => praxis_rollout::ThreadSourceKind::SubAgentCompact,
        ApiThreadSourceKind::SubAgentThreadSpawn => {
            praxis_rollout::ThreadSourceKind::SubAgentThreadSpawn
        }
        ApiThreadSourceKind::SubAgentOther => praxis_rollout::ThreadSourceKind::SubAgentOther,
        ApiThreadSourceKind::Unknown => praxis_rollout::ThreadSourceKind::Unknown,
    }
}

fn next_cursor_string(cursor: Option<&RolloutCursor>) -> Option<String> {
    cursor
        .and_then(|cursor| serde_json::to_value(cursor).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
}
