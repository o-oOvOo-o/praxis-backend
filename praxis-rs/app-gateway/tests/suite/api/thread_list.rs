use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::create_fake_rollout_with_source;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence;
use app_test_support::rollout_path;
use app_test_support::to_response;
use chrono::DateTime;
use chrono::Utc;
use core_test_support::responses;
use praxis_app_gateway_protocol::GitInfo as ApiGitInfo;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::SessionSource;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::UserInput;
use praxis_core::ARCHIVED_SESSIONS_SUBDIR;
use praxis_git_utils::GitSha;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::GitInfo as CoreGitInfo;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SessionSource as CoreSessionSource;
use praxis_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use std::cmp::Reverse;
use std::fs;
use std::fs::FileTimes;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::timeout;
use uuid::Uuid;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

async fn init_mcp(praxis_home: &Path) -> Result<McpProcess> {
    let mut mcp = McpProcess::new(praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    Ok(mcp)
}

async fn list_threads(
    mcp: &mut McpProcess,
    cursor: Option<String>,
    limit: Option<u32>,
    providers: Option<Vec<String>>,
    source_kinds: Option<Vec<ThreadSourceKind>>,
    archived: Option<bool>,
) -> Result<ThreadListResponse> {
    list_threads_with_sort(
        mcp,
        cursor,
        limit,
        providers,
        source_kinds,
        /*sort_key*/ None,
        archived,
    )
    .await
}

async fn list_threads_with_sort(
    mcp: &mut McpProcess,
    cursor: Option<String>,
    limit: Option<u32>,
    providers: Option<Vec<String>>,
    source_kinds: Option<Vec<ThreadSourceKind>>,
    sort_key: Option<ThreadSortKey>,
    archived: Option<bool>,
) -> Result<ThreadListResponse> {
    let request_id = mcp
        .send_thread_list_request(praxis_app_gateway_protocol::ThreadListParams {
            cursor,
            limit,
            sort_key,
            model_providers: providers,
            source_kinds,
            archived,
            cwd: None,
            cwd_scope: None,
            search_term: None,
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    to_response::<ThreadListResponse>(resp)
}

fn create_fake_rollouts<F, G>(
    praxis_home: &Path,
    count: usize,
    provider_for_index: F,
    timestamp_for_index: G,
    preview: &str,
) -> Result<Vec<String>>
where
    F: Fn(usize) -> &'static str,
    G: Fn(usize) -> (String, String),
{
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let (ts_file, ts_rfc) = timestamp_for_index(i);
        ids.push(create_fake_rollout(
            praxis_home,
            &ts_file,
            &ts_rfc,
            preview,
            Some(provider_for_index(i)),
            /*git_info*/ None,
        )?);
    }
    Ok(ids)
}

fn timestamp_at(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> (String, String) {
    (
        format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}"),
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"),
    )
}

#[allow(dead_code)]
fn set_rollout_mtime(path: &Path, updated_at_rfc3339: &str) -> Result<()> {
    let parsed = DateTime::parse_from_rfc3339(updated_at_rfc3339)?.with_timezone(&Utc);
    let times = FileTimes::new().set_modified(parsed.into());
    OpenOptions::new()
        .append(true)
        .open(path)?
        .set_times(times)?;
    Ok(())
}

fn set_rollout_cwd(path: &Path, cwd: &Path) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let first_line = lines
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("rollout at {} is empty", path.display()))?;
    let mut rollout_line: RolloutLine = serde_json::from_str(first_line)?;
    let RolloutItem::SessionMeta(mut session_meta_line) = rollout_line.item else {
        return Err(anyhow::anyhow!(
            "rollout at {} does not start with session metadata",
            path.display()
        ));
    };
    session_meta_line.meta.cwd = cwd.to_path_buf();
    rollout_line.item = RolloutItem::SessionMeta(session_meta_line);
    *first_line = serde_json::to_string(&rollout_line)?;
    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

mod basic;
mod fetch_limits;
mod filters;
mod sorting_archive;
mod source_kinds;

fn create_minimal_config(praxis_home: &std::path::Path) -> std::io::Result<()> {
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        r#"
model = "mock-model"
approval_policy = "never"
"#,
    )
}

fn create_runtime_config(praxis_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
