use std::collections::HashMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use praxis_config::types::McpServerConfig;
use praxis_config::types::McpServerTransportConfig;
use praxis_core::models_manager::manager::RefreshStrategy;
use praxis_login::OpenAiAccountAuth;

use core_test_support::responses;
use core_test_support::responses::mount_models_once;
use core_test_support::responses::mount_sse_once;
use core_test_support::skip_if_no_network;
use core_test_support::stdio_server_bin;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_with_timeout;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use praxis_utils_cargo_bin::cargo_bin;
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;
use serde_json::json;
use serial_test::serial;
use tempfile::tempdir;
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::Instant;
use tokio::time::sleep;

static OPENAI_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAD0AAAA9CAYAAAAeYmHpAAAE6klEQVR4Aeyau44UVxCGx1fZsmRLlm3Zoe0XcGQ5cUiCCIgJeS9CHgAhMkISQnIuGQgJEkBcxLW+nqnZ6uqqc+nuWRC7q/P3qetf9e+MtOwyX25O4Nep6JPyop++0qev9HrfgZ+F6r2DuB/vHOrt/UIkqdDHYvujOW6fO7h/CNEI+a5jc+pBR8uy0jVFsziYu5HtfSUk+Io34q921hLNctFSX0gwww+S8wce8K1LfCU+cYW4888aov8NxqvQILUPPReLOrm6zyLxa4i+6VZuFbJo8d1MOHZm+7VUtB/aIvhPWc/3SWg49JcwFLlHxuXKjtyloo+YNhuW3VS+WPBuUEMvCFKjEDVgFBQHXrnazpqiSxNZCkQ1kYiozsbm9Oz7l4i2Il7vGccGNWAc3XosDrZe/9P3ZnMmzHNEQw4smf8RQ87XEAMsC7Az0Au+dgXerfH4+sHvEc0SYGic8WBBUGqFH2gN7yDrazy7m2pbRTeRmU3+MjZmr1h6LJgPbGy23SI6GlYT0brQ71IY8Us4PNQCm+zepSbaD2BY9xCaAsD9IIj/IzFmKMSdHHonwdZATbTnYREf6/VZGER98N9yCWIvXQwXDoDdhZJoT8jwLnJXDB9w4Sb3e6nK5ndzlkTLnP3JBu4LKkbrYrU69gCVceV0JvpyuW1xlsUVngzhwMetn/XamtTORF9IO5YnWNiyeF9zCAfqR3fUW+vZZKLtgP+ts8BmQRBREAdRDhH3o8QuRh/YucNFz2BEjxbRN6LGzphfKmvP6v6QhqIQyZ8XNJ0W0X83MR1PEcJBNO2KC2Z1TW/v244scp9FwRViZxIOBF0Lctk7ZVSavdLvRlV1hz/ysUi9sr8CIcB3nvWBwA93ykTz18eAYxQ6N/K2DkPA1lv3iXCwmDUT7YkjIby9siXueIJj9H+pzSqJ9oIuJWTUgSSt4WO7o/9GGg0viR4VinNRUDoIj34xoCd6pxD3aK3zfdbnx5v1J3ZNNEJsE0sBG7N27ReDrJc4sFxz7dI/ZAbOmmiKvHBitQXpAdR6+F7v+/ol/tOouUV01EeMZQF2BoQDn6dP4XNr+j9GZEtEK1/L8pFw7bd3a53tsTa7WD+054jOFmPg1XBKPQgnqFfmFcy32ZRvjmiIIQTYFvyDxQ8nH8WIwwGwlyDjDznnilYyFr6njrlZwsKkBpO59A7OwgdzPEWRm+G+oeb7IfyNuzjEEVLrOVxJsxvxwF8kmCM6I2QYmJunz4u4TrADpfl7mlbRTWQ7VmrBzh3+C9f6Grc3YoGN9dg/SXFthpRsT6vobfXRs2VBlgBHXVMLHjDNbIZv1sZ9+X3hB09cXdH1JKViyG0+W9bWZDa/r2f9zAFR71sTzGpMSWz2iI4YssWjWo3REy1MDGjdwe5e0dFSiAC1JakBvu4/CUS8Eh6dqHdU0Or0ioY3W5ClSqDXAy7/6SRfgw8vt4I+tbvvNtFT2kVDhY5+IGb1rCqYaXNF08vSALsXCPmt0kQNqJT1p5eI1mkIV/BxCY1z85lOzeFbPBQHURkkPTlwTYK9gTVE25l84IbFFN+YJDHjdpn0gq6mrHht0dkcjbM4UL9283O5p77GN+SPW/QwVB4IUYg7Or+Kp7naR6qktP98LNF2UxWo9yObPIT9KYg+hK4i56no4rfnM0qeyFf6AwAAAP//trwR3wAAAAZJREFUAwBZ0sR75itw5gAAAABJRU5ErkJggg==";

async fn wait_for_streamable_http_server(
    server_child: &mut Child,
    address: &str,
    timeout: Duration,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    let metadata_url = format!("http://{address}/.well-known/oauth-authorization-server/mcp");
    let client = Client::builder().no_proxy().build()?;
    loop {
        if let Some(status) = server_child.try_wait()? {
            return Err(anyhow::anyhow!(
                "streamable HTTP server exited early with status {status}"
            ));
        }

        let remaining = deadline.saturating_duration_since(Instant::now());

        if remaining.is_zero() {
            return Err(anyhow::anyhow!(
                "timed out waiting for streamable HTTP server metadata at {metadata_url}: deadline reached"
            ));
        }

        match tokio::time::timeout(remaining, client.get(&metadata_url).send()).await {
            Ok(Ok(response)) if response.status() == StatusCode::OK => return Ok(()),
            Ok(Ok(response)) => {
                if Instant::now() >= deadline {
                    return Err(anyhow::anyhow!(
                        "timed out waiting for streamable HTTP server metadata at {metadata_url}: HTTP {}",
                        response.status()
                    ));
                }
            }
            Ok(Err(error)) => {
                if Instant::now() >= deadline {
                    return Err(anyhow::anyhow!(
                        "timed out waiting for streamable HTTP server metadata at {metadata_url}: {error}"
                    ));
                }
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "timed out waiting for streamable HTTP server metadata at {metadata_url}: request timed out"
                ));
            }
        }

        sleep(Duration::from_millis(50)).await;
    }
}

fn write_fallback_oauth_tokens(
    home: &Path,
    server_name: &str,
    server_url: &str,
    client_id: &str,
    access_token: &str,
    refresh_token: &str,
) -> anyhow::Result<()> {
    let expires_at = SystemTime::now()
        .checked_add(Duration::from_secs(3600))
        .ok_or_else(|| anyhow::anyhow!("failed to compute expiry time"))?
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;

    let store = serde_json::json!({
        "stub": {
            "server_name": server_name,
            "server_url": server_url,
            "client_id": client_id,
            "access_token": access_token,
            "expires_at": expires_at,
            "refresh_token": refresh_token,
            "scopes": ["profile"],
        }
    });

    let file_path = home.join(".credentials.json");
    fs::write(&file_path, serde_json::to_vec(&store)?)?;
    Ok(())
}

struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[path = "rmcp_client/stdio_round_trips.rs"]
mod stdio_round_trips;
#[path = "rmcp_client/streamable_http_round_trips.rs"]
mod streamable_http_round_trips;
