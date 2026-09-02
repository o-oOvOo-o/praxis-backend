use crate::BatchId;
use crate::CommandId;
use crate::Digest;
use crate::EventId;
use crate::ReceiptId;
use crate::THREAD_STORE_SCHEMA_VERSION;
use crate::ThreadRevision;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Applied,
    NoOp,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AchievedDurability {
    None,
    Volatile,
    Buffered,
    Durable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptDiagnostic {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedEventRef {
    pub event_id: EventId,
    pub revision: ThreadRevision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadCommandReceipt {
    pub schema_version: u32,
    pub receipt_id: ReceiptId,
    pub command_id: CommandId,
    pub command_digest: Digest,
    pub batch_id: Option<BatchId>,
    pub status: ReceiptStatus,
    pub expected_revision: ThreadRevision,
    pub revision_before: ThreadRevision,
    pub revision_after: ThreadRevision,
    pub events: Vec<CommittedEventRef>,
    pub batch_digest: Option<Digest>,
    pub durability: AchievedDurability,
    pub diagnostics: Vec<ReceiptDiagnostic>,
    pub recorded_at_unix_ms: i64,
}

impl ThreadCommandReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn applied(
        command_id: CommandId,
        command_digest: Digest,
        batch_id: BatchId,
        expected_revision: ThreadRevision,
        events: Vec<CommittedEventRef>,
        batch_digest: Digest,
        durability: AchievedDurability,
        recorded_at_unix_ms: i64,
    ) -> Self {
        let revision_after = events
            .last()
            .map_or(expected_revision, |event| event.revision);
        Self {
            schema_version: THREAD_STORE_SCHEMA_VERSION,
            receipt_id: ReceiptId::for_command(command_id),
            command_id,
            command_digest,
            batch_id: Some(batch_id),
            status: ReceiptStatus::Applied,
            expected_revision,
            revision_before: expected_revision,
            revision_after,
            events,
            batch_digest: Some(batch_digest),
            durability,
            diagnostics: Vec::new(),
            recorded_at_unix_ms,
        }
    }

    pub fn no_op(
        command_id: CommandId,
        command_digest: Digest,
        revision: ThreadRevision,
        recorded_at_unix_ms: i64,
    ) -> Self {
        Self {
            schema_version: THREAD_STORE_SCHEMA_VERSION,
            receipt_id: ReceiptId::for_command(command_id),
            command_id,
            command_digest,
            batch_id: None,
            status: ReceiptStatus::NoOp,
            expected_revision: revision,
            revision_before: revision,
            revision_after: revision,
            events: Vec::new(),
            batch_digest: None,
            durability: AchievedDurability::None,
            diagnostics: Vec::new(),
            recorded_at_unix_ms,
        }
    }

    pub fn rejected(
        command_id: CommandId,
        command_digest: Digest,
        expected_revision: ThreadRevision,
        current_revision: ThreadRevision,
        diagnostic: ReceiptDiagnostic,
        recorded_at_unix_ms: i64,
    ) -> Self {
        Self {
            schema_version: THREAD_STORE_SCHEMA_VERSION,
            receipt_id: ReceiptId::for_command(command_id),
            command_id,
            command_digest,
            batch_id: None,
            status: ReceiptStatus::Rejected,
            expected_revision,
            revision_before: current_revision,
            revision_after: current_revision,
            events: Vec::new(),
            batch_digest: None,
            durability: AchievedDurability::None,
            diagnostics: vec![diagnostic],
            recorded_at_unix_ms,
        }
    }

    pub fn validate_causality(
        &self,
        expected_command_digest: Digest,
    ) -> Result<(), ReceiptValidationError> {
        if self.schema_version != THREAD_STORE_SCHEMA_VERSION {
            return Err(ReceiptValidationError::SchemaVersion);
        }
        if self.receipt_id != ReceiptId::for_command(self.command_id) {
            return Err(ReceiptValidationError::ReceiptId);
        }
        if self.command_digest != expected_command_digest {
            return Err(ReceiptValidationError::CommandDigest);
        }
        match self.status {
            ReceiptStatus::Applied => self.validate_applied(),
            ReceiptStatus::NoOp => self.validate_no_op(),
            ReceiptStatus::Rejected => self.validate_rejected(),
        }
    }

    fn validate_applied(&self) -> Result<(), ReceiptValidationError> {
        if self.expected_revision != self.revision_before
            || self.revision_after <= self.revision_before
            || self.batch_id.is_none()
            || self.batch_digest.is_none()
            || self.durability == AchievedDurability::None
            || !self.diagnostics.is_empty()
            || self.events.is_empty()
        {
            return Err(ReceiptValidationError::StatusPayload);
        }
        let expected_count = self
            .revision_after
            .get()
            .checked_sub(self.revision_before.get())
            .ok_or(ReceiptValidationError::RevisionRange)?;
        if usize::try_from(expected_count).ok() != Some(self.events.len()) {
            return Err(ReceiptValidationError::RevisionRange);
        }
        for (index, event) in self.events.iter().enumerate() {
            let offset =
                u64::try_from(index).map_err(|_| ReceiptValidationError::RevisionRange)? + 1;
            let expected = self
                .revision_before
                .checked_advance(offset)
                .ok_or(ReceiptValidationError::RevisionRange)?;
            if event.revision != expected {
                return Err(ReceiptValidationError::EventSequence);
            }
        }
        Ok(())
    }

    fn validate_no_op(&self) -> Result<(), ReceiptValidationError> {
        if self.expected_revision != self.revision_before
            || self.revision_before != self.revision_after
            || self.batch_id.is_some()
            || self.batch_digest.is_some()
            || self.durability != AchievedDurability::None
            || !self.events.is_empty()
            || !self.diagnostics.is_empty()
        {
            return Err(ReceiptValidationError::StatusPayload);
        }
        Ok(())
    }

    fn validate_rejected(&self) -> Result<(), ReceiptValidationError> {
        if self.revision_before != self.revision_after
            || self.batch_id.is_some()
            || self.batch_digest.is_some()
            || self.durability != AchievedDurability::None
            || !self.events.is_empty()
            || self.diagnostics.is_empty()
        {
            return Err(ReceiptValidationError::StatusPayload);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReceiptValidationError {
    #[error("unsupported ThreadStore receipt schema version")]
    SchemaVersion,
    #[error("receipt id does not match command id")]
    ReceiptId,
    #[error("receipt command digest does not match the submitted command")]
    CommandDigest,
    #[error("receipt status does not match revision or payload fields")]
    StatusPayload,
    #[error("receipt revision range does not match committed event count")]
    RevisionRange,
    #[error("committed event revisions are not contiguous")]
    EventSequence,
}
