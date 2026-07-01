use std::path::Path;

use anyhow::Context;
use chrono::Utc;
use praxis_state::ThreadMetadataBuilder;

use crate::state_db_bridge::StateDbHandle;
use crate::state_db_bridge::{self as state_db};

use super::super::Session;

impl Session {
    pub(crate) async fn state_db_for_thread_feature(
        &self,
        feature_name: &str,
    ) -> anyhow::Result<Option<StateDbHandle>> {
        self.ensure_rollout_materialized().await;
        let state_db = match self.state_db() {
            Some(state_db) => state_db,
            None => {
                let config = self.original_config().await;
                if config.ephemeral {
                    return Ok(None);
                }
                state_db::try_get_state_db(&config).await.with_context(|| {
                    format!(
                        "{feature_name} requires state db at {}",
                        config.sqlite_home.display()
                    )
                })?
            }
        };
        self.ensure_thread_metadata_for_feature(&state_db, feature_name)
            .await?;
        Ok(Some(state_db))
    }

    pub(crate) async fn require_state_db_for_thread_feature(
        &self,
        feature_name: &str,
    ) -> anyhow::Result<StateDbHandle> {
        self.state_db_for_thread_feature(feature_name)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{feature_name} requires a persisted thread; this thread is ephemeral"
                )
            })
    }

    async fn ensure_thread_metadata_for_feature(
        &self,
        state_db: &StateDbHandle,
        feature_name: &str,
    ) -> anyhow::Result<()> {
        if state_db.get_thread(self.conversation_id).await?.is_some() {
            return Ok(());
        }

        let rollout_path = self.current_rollout_path().await;
        if let Some(rollout_path) = rollout_path.as_deref() {
            let config = self.original_config().await;
            state_db::reconcile_rollout(
                Some(state_db.as_ref()),
                rollout_path,
                config.model_provider_id.as_str(),
                None,
                &[],
                None,
                None,
            )
            .await;
        }
        if state_db.get_thread(self.conversation_id).await?.is_some() {
            return Ok(());
        }

        if let Some(rollout_path) = rollout_path.as_deref() {
            self.insert_live_thread_metadata(state_db, rollout_path)
                .await
                .with_context(|| {
                    format!(
                        "{feature_name} failed to materialize live thread metadata for {}",
                        self.conversation_id
                    )
                })?;
        }
        if state_db.get_thread(self.conversation_id).await?.is_some() {
            return Ok(());
        }

        let rollout_hint = rollout_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "no rollout path".to_string());
        anyhow::bail!(
            "{feature_name} requires materialized thread metadata for {} ({rollout_hint})",
            self.conversation_id
        );
    }

    async fn insert_live_thread_metadata(
        &self,
        state_db: &StateDbHandle,
        rollout_path: &Path,
    ) -> anyhow::Result<()> {
        let metadata = {
            let state = self.state.lock().await;
            let session_configuration = &state.session_configuration;
            let mut builder = ThreadMetadataBuilder::new(
                self.conversation_id,
                rollout_path.to_path_buf(),
                Utc::now(),
                session_configuration.session_source.clone(),
            );
            builder.model_provider = Some(
                session_configuration
                    .original_config_do_not_use
                    .model_provider_id
                    .clone(),
            );
            builder.cwd = session_configuration.cwd.to_path_buf();
            builder.cli_version = Some(env!("CARGO_PKG_VERSION").to_string());
            builder.sandbox_policy = session_configuration.sandbox_policy.get().clone();
            builder.approval_mode = session_configuration.approval_policy.value();
            builder.build(
                session_configuration
                    .original_config_do_not_use
                    .model_provider_id
                    .as_str(),
            )
        };
        state_db.insert_thread_if_absent(&metadata).await?;
        Ok(())
    }
}
