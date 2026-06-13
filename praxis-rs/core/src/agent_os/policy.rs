use chrono::Duration;
use std::sync::OnceLock;

pub(super) const COORDINATOR_RANK: u8 = 0;
pub(super) const MAX_COORDINATORS: usize = 3;
const DEFAULT_TICKET_TTL_SECONDS: i64 = 30 * 60;
const DEFAULT_LEASE_TTL_SECONDS: i64 = 30 * 60;
pub(super) const LEASE_JANITOR_INTERVAL_SECONDS: u64 = 30;
const MAX_AGENT_OS_EVENTS_IN_MEMORY: usize = 1_000;
const DEFAULT_ARTIFACT_READ_MAX_BYTES: usize = 64 * 1024;
pub(super) const HARD_ARTIFACT_READ_MAX_BYTES: usize = 1024 * 1024;

static AGENT_OS_POLICY: OnceLock<AgentOsPolicy> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub(super) struct AgentOsPolicy {
    ticket_ttl_seconds: i64,
    lease_ttl_seconds: i64,
    pub(super) max_events_in_memory: usize,
    pub(super) default_artifact_read_max_bytes: usize,
}

impl AgentOsPolicy {
    pub(super) fn get() -> &'static Self {
        AGENT_OS_POLICY.get_or_init(|| Self {
            ticket_ttl_seconds: read_i64_env(
                "PRAXIS_AGENTOS_TICKET_TTL_SECONDS",
                DEFAULT_TICKET_TTL_SECONDS,
                60,
                24 * 60 * 60,
            ),
            lease_ttl_seconds: read_i64_env(
                "PRAXIS_AGENTOS_LEASE_TTL_SECONDS",
                DEFAULT_LEASE_TTL_SECONDS,
                60,
                24 * 60 * 60,
            ),
            max_events_in_memory: read_usize_env(
                "PRAXIS_AGENTOS_MAX_EVENTS_IN_MEMORY",
                MAX_AGENT_OS_EVENTS_IN_MEMORY,
                1,
                100_000,
            ),
            default_artifact_read_max_bytes: read_usize_env(
                "PRAXIS_AGENTOS_ARTIFACT_READ_MAX_BYTES",
                DEFAULT_ARTIFACT_READ_MAX_BYTES,
                1,
                HARD_ARTIFACT_READ_MAX_BYTES,
            ),
        })
    }

    pub(super) fn ticket_ttl(&self) -> Duration {
        Duration::seconds(self.ticket_ttl_seconds)
    }

    pub(super) fn lease_ttl(&self) -> Duration {
        Duration::seconds(self.lease_ttl_seconds)
    }
}

fn read_i64_env(name: &str, default_value: i64, hard_min: i64, hard_max: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|value| value.clamp(hard_min, hard_max))
        .unwrap_or(default_value)
}

fn read_usize_env(name: &str, default_value: usize, hard_min: usize, hard_max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(hard_min, hard_max))
        .unwrap_or(default_value)
}
