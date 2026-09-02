use super::history::ThreadHistoryPageReadError;
use super::history::ThreadHistorySource;
use super::history::ThreadTurnHydration;
use super::history::ThreadTurnPage;
use super::history::{self};
use super::list::ThreadStoreListPage;
use super::list::ThreadStoreListQuery;
use super::list::{self};
use super::paths;
use super::summary::ThreadStoreSummary;
use super::summary::{self};
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadHistoryCursor;
use praxis_app_gateway_protocol::Turn;
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_rollout::ThreadHistoryReader;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadata;
use std::path::Path;
use std::path::PathBuf;

pub(crate) struct ThreadProjection<'a> {
    config: &'a Config,
    store: tokio::sync::OnceCell<praxis_rollout::ThreadStore<'a, Config>>,
}

impl<'a> ThreadProjection<'a> {
    pub(in crate::praxis_message_processor) fn new(config: &'a Config) -> Self {
        Self {
            config,
            store: tokio::sync::OnceCell::new(),
        }
    }

    async fn store(&self) -> &praxis_rollout::ThreadStore<'a, Config> {
        self.store
            .get_or_init(|| async { praxis_rollout::ThreadStore::open(self.config).await })
            .await
    }

    pub(in crate::praxis_message_processor) async fn list_summaries(
        &self,
        query: ThreadStoreListQuery,
    ) -> std::io::Result<ThreadStoreListPage> {
        list::list_thread_summaries(self.store().await, query).await
    }

    pub(in crate::praxis_message_processor) async fn read_history_cwd(
        &self,
        thread_id: Option<ThreadId>,
        rollout_path: &Path,
    ) -> Option<PathBuf> {
        paths::read_thread_history_cwd(self.store().await, thread_id, rollout_path).await
    }

    pub(in crate::praxis_message_processor) async fn find_active_rollout_path(
        &self,
        thread_id: ThreadId,
    ) -> std::io::Result<Option<PathBuf>> {
        self.find_rollout_path(thread_id, Some(false)).await
    }

    pub(in crate::praxis_message_processor) async fn find_any_rollout_path(
        &self,
        thread_id: ThreadId,
    ) -> std::io::Result<Option<PathBuf>> {
        self.find_rollout_path(thread_id, None).await
    }

    pub(in crate::praxis_message_processor) async fn find_archived_rollout_path(
        &self,
        thread_id: ThreadId,
    ) -> std::io::Result<Option<PathBuf>> {
        self.find_rollout_path(thread_id, Some(true)).await
    }

    pub(in crate::praxis_message_processor) async fn thread_exists(
        &self,
        thread_id: ThreadId,
        archived_only: Option<bool>,
    ) -> std::io::Result<bool> {
        paths::thread_exists(self.store().await, thread_id, archived_only).await
    }

    pub(in crate::praxis_message_processor) async fn write_thread_name(
        &self,
        thread_id: ThreadId,
        name: &str,
    ) -> std::io::Result<()> {
        paths::write_thread_name(self.store().await, thread_id, name).await
    }

    pub(in crate::praxis_message_processor) async fn set_archived(
        &self,
        thread_id: ThreadId,
        archived: bool,
    ) -> std::io::Result<()> {
        self.store().await.set_archived(thread_id, archived).await
    }

    pub(in crate::praxis_message_processor) async fn delete_native_thread(
        &self,
        thread_id: ThreadId,
    ) -> std::io::Result<bool> {
        self.store().await.delete_native_thread(thread_id).await
    }

    pub(in crate::praxis_message_processor) async fn resolve_thread_name(
        &self,
        thread_id: ThreadId,
    ) -> Option<String> {
        paths::resolve_thread_name(self.store().await, thread_id).await
    }

    pub(in crate::praxis_message_processor) async fn read_directory_summary(
        &self,
        thread_id: ThreadId,
    ) -> Option<ThreadStoreSummary> {
        self.try_read_directory_summary(thread_id)
            .await
            .ok()
            .flatten()
    }

    pub(in crate::praxis_message_processor) async fn try_read_directory_summary(
        &self,
        thread_id: ThreadId,
    ) -> std::io::Result<Option<ThreadStoreSummary>> {
        summary::try_read_directory_summary(
            self.store().await,
            thread_id,
            self.config.model_provider_id.as_str(),
        )
        .await
    }

    pub(in crate::praxis_message_processor) async fn hydrate_turns(
        &self,
        thread: &mut Thread,
        source: ThreadHistorySource<'_>,
        hydration: ThreadTurnHydration,
        active_turn: Option<&Turn>,
    ) -> std::result::Result<(), String> {
        history::hydrate_thread_turns(
            self.store().await.history(),
            thread,
            source,
            hydration,
            active_turn,
        )
        .await
    }

    async fn find_rollout_path(
        &self,
        thread_id: ThreadId,
        archived_only: Option<bool>,
    ) -> std::io::Result<Option<PathBuf>> {
        paths::find_thread_rollout_path(self.store().await, thread_id, archived_only).await
    }
}

impl ThreadProjection<'_> {
    pub(in crate::praxis_message_processor) fn preview_from_rollout_items(
        items: &[RolloutItem],
    ) -> String {
        history::preview_from_rollout_items(items)
    }

    pub(in crate::praxis_message_processor) async fn read_rollout_items(
        &self,
        path: &Path,
    ) -> std::io::Result<Vec<RolloutItem>> {
        history::read_thread_rollout_items(self.store().await.history(), path).await
    }

    pub(in crate::praxis_message_processor) async fn read_initial_history(
        &self,
        path: &Path,
    ) -> std::io::Result<InitialHistory> {
        history::read_thread_initial_history(self.store().await.history(), path).await
    }

    pub(in crate::praxis_message_processor) async fn read_turns_from_rollout(
        &self,
        path: &Path,
        hydration: ThreadTurnHydration,
    ) -> std::io::Result<Vec<Turn>> {
        history::read_thread_turns_from_rollout(self.store().await.history(), path, hydration).await
    }

    pub(in crate::praxis_message_processor) async fn read_turn_page_from_rollout(
        &self,
        path: &Path,
        cursor: Option<ThreadHistoryCursor>,
        limit: u32,
    ) -> Result<ThreadTurnPage, ThreadHistoryPageReadError> {
        history::read_thread_turn_page_from_rollout(
            self.store().await.history(),
            path,
            cursor,
            limit,
        )
        .await
    }

    pub(in crate::praxis_message_processor) async fn hydrate_turns_with_reader(
        history_reader: &ThreadHistoryReader,
        thread: &mut Thread,
        source: ThreadHistorySource<'_>,
        hydration: ThreadTurnHydration,
        active_turn: Option<&Turn>,
    ) -> std::result::Result<(), String> {
        history::hydrate_thread_turns(history_reader, thread, source, hydration, active_turn).await
    }

    pub(in crate::praxis_message_processor) async fn read_turns_with_reader(
        history_reader: &ThreadHistoryReader,
        path: &Path,
        hydration: ThreadTurnHydration,
    ) -> std::io::Result<Vec<Turn>> {
        history::read_thread_turns_from_rollout(history_reader, path, hydration).await
    }

    pub(in crate::praxis_message_processor) fn hydrate_turns_from_items(
        thread: &mut Thread,
        items: &[RolloutItem],
        hydration: ThreadTurnHydration,
        active_turn: Option<&Turn>,
    ) {
        history::hydrate_thread_turns_from_items(thread, items, hydration, active_turn);
    }

    pub(in crate::praxis_message_processor) async fn read_rollout_summary(
        path: &Path,
        fallback_provider: &str,
    ) -> std::io::Result<ThreadStoreSummary> {
        summary::read_summary_from_rollout(path, fallback_provider).await
    }

    pub(in crate::praxis_message_processor) async fn read_state_db_summary(
        state_db_ctx: Option<&StateDbHandle>,
        thread_id: ThreadId,
    ) -> Option<ThreadStoreSummary> {
        summary::read_state_db_summary(state_db_ctx, thread_id).await
    }

    pub(in crate::praxis_message_processor) fn summary_from_metadata(
        metadata: &ThreadMetadata,
    ) -> ThreadStoreSummary {
        summary::summary_from_metadata(metadata)
    }
}
