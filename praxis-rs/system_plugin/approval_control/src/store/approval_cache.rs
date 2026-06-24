use super::ApprovalRecord;
use crate::state::ApprovalCacheScope;
use praxis_protocol::protocol::ReviewDecision;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ApprovalCache {
    records: HashMap<String, ApprovalRecord>,
}

impl ApprovalCache {
    pub fn get(&self, key: &str, now_millis: u64) -> Option<&ApprovalRecord> {
        self.records
            .get(key)
            .filter(|record| !record.is_expired_at(now_millis))
    }

    pub fn remember(
        &mut self,
        key: impl Into<String>,
        scope: ApprovalCacheScope,
        thread_id: Option<String>,
        turn_id: Option<String>,
        decision: ReviewDecision,
        now_millis: u64,
        ttl_millis: Option<u64>,
    ) {
        let key = key.into();
        let record = ApprovalRecord {
            key: key.clone(),
            scope,
            thread_id,
            turn_id,
            verdict: (&decision).into(),
            decision,
            created_at_millis: now_millis,
            expires_at_millis: ttl_millis.map(|ttl| now_millis.saturating_add(ttl)),
        };
        self.records.insert(key, record);
    }

    pub fn retain_live(&mut self, now_millis: u64) {
        self.records
            .retain(|_, record| !record.is_expired_at(now_millis));
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}
