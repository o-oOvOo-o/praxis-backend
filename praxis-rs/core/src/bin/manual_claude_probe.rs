use anyhow::Context;
use anyhow::Result;
use futures::StreamExt;
use praxis_core::ModelClient;
use praxis_core::ModelProviderInfo;
use praxis_core::Prompt;
use praxis_core::ResponseEvent;
use praxis_login::AuthCredentialsStoreMode;
use praxis_login::AuthManager;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::protocol::SessionSource;
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    anyhow::ensure!(
        !prompt.trim().is_empty(),
        "usage: cargo run -p praxis-core --bin manual_claude_probe -- <prompt>"
    );

    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = std::env::var("ANTHROPIC_MODEL").context("ANTHROPIC_MODEL must be set")?;
    let provider_name = if base_url == praxis_core::ANTHROPIC_API_BASE_URL {
        "Anthropic"
    } else {
        "Manual Claude Probe"
    };

    let provider_toml = format!(
        "name = {provider_name:?}\nbase_url = {base_url:?}\nenv_key = \"ANTHROPIC_API_KEY\"\nwire_api = \"claude\"\n"
    );
    let provider: ModelProviderInfo =
        toml::from_str(&provider_toml).context("failed to build provider config")?;

    let praxis_home = praxis_utils_home_dir::find_praxis_home()
        .context("failed to resolve Praxis home for Claude OAuth")?;
    if std::env::var("PRAXIS_ANTHROPIC_OAUTH_LOGIN").as_deref() == Ok("1") {
        praxis_login::login_anthropic_oauth(&praxis_home)
            .await
            .context("Anthropic OAuth login failed")?;
    }
    let auth_manager = AuthManager::shared(praxis_home, false, AuthCredentialsStoreMode::Auto);

    let client = ModelClient::new(
        Some(auth_manager),
        ThreadId::new(),
        praxis_core::ANTHROPIC_PROVIDER_ID.to_string(),
        provider,
        SessionSource::Cli,
        None,
        false,
        false,
        None,
    );
    let mut session = client.new_session();
    let session_telemetry = SessionTelemetry::new(
        ThreadId::new(),
        model.as_str(),
        model.as_str(),
        None,
        None,
        None,
        "manual_claude_probe".to_string(),
        false,
        "powershell".to_string(),
        SessionSource::Cli,
    );

    let mut prompt_request = Prompt::default();
    prompt_request.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: prompt }],
        end_turn: None,
        phase: None,
    }];
    prompt_request.base_instructions = BaseInstructions {
        text: String::new(),
    };

    let mut stream = session
        .stream(
            &prompt_request,
            &model_info(&model)?,
            &session_telemetry,
            None,
            ReasoningSummary::Auto,
            None,
            None,
        )
        .await
        .context("provider stream failed")?;

    let mut delta_text = String::new();
    let mut done_text = String::new();
    let mut response_id = None;
    let mut token_usage = None;

    while let Some(event) = stream.next().await {
        match event.context("stream event failed")? {
            ResponseEvent::OutputTextDelta(text) => delta_text.push_str(&text),
            ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => {
                done_text.push_str(&extract_output_text(&content));
            }
            ResponseEvent::Completed {
                response_id: id,
                token_usage: usage,
            } => {
                response_id = Some(id);
                token_usage = usage;
                break;
            }
            _ => {}
        }
    }

    let text = if delta_text.is_empty() {
        done_text
    } else {
        delta_text
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "model": model,
            "response_id": response_id,
            "text": text,
            "token_usage": token_usage,
        }))?
    );

    Ok(())
}

fn model_info(model: &str) -> Result<ModelInfo> {
    serde_json::from_value(json!({
        "slug": model,
        "display_name": model,
        "description": null,
        "default_reasoning_level": null,
        "supported_reasoning_levels": [],
        "shell_type": "local",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 0,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": "",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "default_reasoning_summary": "auto",
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "web_search_tool_type": "text",
        "truncation_policy": {
            "mode": "tokens",
            "limit": 100000
        },
        "supports_parallel_tool_calls": true,
        "supports_image_detail_original": false,
        "context_window": null,
        "auto_compact_token_limit": null,
        "effective_context_window_percent": 100,
        "experimental_supported_tools": [],
        "input_modalities": ["text"],
        "supports_search_tool": false
    }))
    .context("failed to construct model info")
}

fn extract_output_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::OutputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
