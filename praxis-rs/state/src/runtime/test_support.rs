#[cfg(test)]
use chrono::DateTime;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use praxis_protocol::ThreadId;
#[cfg(test)]
use praxis_protocol::openai_models::ReasoningEffort;
#[cfg(test)]
use praxis_protocol::protocol::AskForApproval;
#[cfg(test)]
use praxis_protocol::protocol::SandboxPolicy;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::time::SystemTime;
#[cfg(test)]
use std::time::UNIX_EPOCH;
#[cfg(test)]
use uuid::Uuid;

#[cfg(test)]
use crate::ThreadMetadata;

#[cfg(test)]
pub(super) fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "praxis-state-runtime-test-{nanos}-{}",
        Uuid::new_v4()
    ))
}

#[cfg(test)]
pub(super) fn test_thread_metadata(
    praxis_home: &Path,
    thread_id: ThreadId,
    cwd: PathBuf,
) -> ThreadMetadata {
    let now = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp");
    ThreadMetadata {
        id: thread_id,
        rollout_path: praxis_home.join(format!("rollout-{thread_id}.jsonl")),
        created_at: now,
        updated_at: now,
        source: "cli".to_string(),
        agent_nickname: None,
        agent_role: None,
        agent_path: None,
        model_provider: "test-provider".to_string(),
        model: Some("gpt-5".to_string()),
        reasoning_effort: Some(ReasoningEffort::Medium),
        cwd,
        cli_version: "0.0.0".to_string(),
        title: String::new(),
        sandbox_policy: crate::extract::enum_to_string(&SandboxPolicy::new_read_only_policy()),
        approval_mode: crate::extract::enum_to_string(&AskForApproval::OnRequest),
        tokens_used: 0,
        session_summary: None,
        total_cost_micros: None,
        last_cost_micros: None,
        selfwork_plan_path: None,
        first_user_message: Some("hello".to_string()),
        archived_at: None,
        git_sha: None,
        git_branch: None,
        git_origin_url: None,
    }
}
