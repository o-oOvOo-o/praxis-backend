use super::*;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::ExecCall;
use crate::exec_cell::ExecCell;
use dirs::home_dir;
use praxis_config::types::McpServerConfig;
use praxis_config::types::McpServerDisabledReason;
use praxis_core::config::Config;
use praxis_core::config::ConfigBuilder;
use praxis_otel::RuntimeMetricTotals;
use praxis_otel::RuntimeMetricsSummary;
use praxis_protocol::ThreadId;
use praxis_protocol::account::PlanType;
use praxis_protocol::models::WebSearchAction;
use praxis_protocol::parse_command::ParsedCommand;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::McpAuthStatus;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionConfiguredEvent;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::mcp::Tool;
use praxis_protocol::protocol::ExecCommandSource;
use rmcp::model::Content;

const SMALL_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
async fn test_config() -> Config {
    let praxis_home = std::env::temp_dir();
    ConfigBuilder::default()
        .praxis_home(praxis_home.clone())
        .build()
        .await
        .expect("config")
}

fn test_tui_config() -> TuiRuntimeConfig {
    TuiRuntimeConfig {
        animations: false,
        ..Default::default()
    }
}

fn test_cwd() -> PathBuf {
    // These tests only need a stable absolute cwd; using temp_dir() avoids baking Unix- or
    // Windows-specific root semantics into the fixtures.
    std::env::temp_dir()
}

fn stdio_server_config(
    command: &str,
    args: Vec<&str>,
    env: Option<HashMap<String, String>>,
    env_vars: Vec<&str>,
) -> McpServerConfig {
    let mut table = toml::Table::new();
    table.insert(
        "command".to_string(),
        toml::Value::String(command.to_string()),
    );
    if !args.is_empty() {
        table.insert(
            "args".to_string(),
            toml::Value::Array(
                args.into_iter()
                    .map(|arg| toml::Value::String(arg.to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(env) = env {
        table.insert("env".to_string(), string_map_to_toml_value(env));
    }
    if !env_vars.is_empty() {
        table.insert(
            "env_vars".to_string(),
            toml::Value::Array(
                env_vars
                    .into_iter()
                    .map(|name| toml::Value::String(name.to_string()))
                    .collect(),
            ),
        );
    }

    toml::Value::Table(table)
        .try_into()
        .expect("test stdio MCP config should deserialize")
}

fn streamable_http_server_config(
    url: &str,
    bearer_token_env_var: Option<&str>,
    http_headers: Option<HashMap<String, String>>,
    env_http_headers: Option<HashMap<String, String>>,
) -> McpServerConfig {
    let mut table = toml::Table::new();
    table.insert("url".to_string(), toml::Value::String(url.to_string()));
    if let Some(bearer_token_env_var) = bearer_token_env_var {
        table.insert(
            "bearer_token_env_var".to_string(),
            toml::Value::String(bearer_token_env_var.to_string()),
        );
    }
    if let Some(http_headers) = http_headers {
        table.insert(
            "http_headers".to_string(),
            string_map_to_toml_value(http_headers),
        );
    }
    if let Some(env_http_headers) = env_http_headers {
        table.insert(
            "env_http_headers".to_string(),
            string_map_to_toml_value(env_http_headers),
        );
    }

    toml::Value::Table(table)
        .try_into()
        .expect("test streamable_http MCP config should deserialize")
}

fn string_map_to_toml_value(entries: HashMap<String, String>) -> toml::Value {
    toml::Value::Table(
        entries
            .into_iter()
            .map(|(key, value)| (key, toml::Value::String(value)))
            .collect(),
    )
}

fn render_lines(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn render_transcript(cell: &dyn HistoryCell) -> Vec<String> {
    render_lines(&cell.transcript_lines(u16::MAX))
}

fn image_block(data: &str) -> serde_json::Value {
    serde_json::to_value(Content::image(data.to_string(), "image/png"))
        .expect("image content should serialize")
}

fn text_block(text: &str) -> serde_json::Value {
    serde_json::to_value(Content::text(text)).expect("text content should serialize")
}

fn resource_link_block(
    uri: &str,
    name: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> serde_json::Value {
    serde_json::to_value(Content::resource_link(rmcp::model::RawResource {
        uri: uri.to_string(),
        name: name.to_string(),
        title: title.map(str::to_string),
        description: description.map(str::to_string),
        mime_type: None,
        size: None,
        icons: None,
        meta: None,
    }))
    .expect("resource link content should serialize")
}

mod basic_and_exec;
mod exec_commands;
mod mcp_and_web_search;
mod session_header;
mod user_plan_reasoning;
