use crate::agent::AgentStatus;
use crate::config::Config;
use crate::error::PraxisErr;
use crate::function_tool::FunctionCallError;
use crate::llm::registry::LlmProfileRegistry;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use crate::models_manager::manager::RefreshStrategy;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use praxis_features::Feature;
use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::protocol::CollabAgentRef;
use praxis_protocol::protocol::CollabAgentStatusEntry;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::user_input::UserInput;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Minimum wait timeout to prevent tight polling loops from burning CPU.
pub(crate) const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
pub(crate) const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
pub(crate) const MAX_WAIT_TIMEOUT_MS: i64 = 3600 * 1000;

pub(crate) fn function_arguments(payload: ToolPayload) -> Result<String, FunctionCallError> {
    match payload {
        ToolPayload::Function { arguments } => Ok(arguments),
        _ => Err(FunctionCallError::RespondToModel(
            "collab handler received unsupported payload".to_string(),
        )),
    }
}

pub(crate) fn tool_output_json_text<T>(value: &T, tool_name: &str) -> String
where
    T: Serialize,
{
    serde_json::to_string(value).unwrap_or_else(|err| {
        JsonValue::String(format!("failed to serialize {tool_name} result: {err}")).to_string()
    })
}

pub(crate) fn tool_output_response_item<T>(
    call_id: &str,
    payload: &ToolPayload,
    value: &T,
    success: Option<bool>,
    tool_name: &str,
) -> ResponseInputItem
where
    T: Serialize,
{
    FunctionToolOutput::from_text(tool_output_json_text(value, tool_name), success)
        .to_response_item(call_id, payload)
}

pub(crate) fn tool_output_code_mode_result<T>(value: &T, tool_name: &str) -> JsonValue
where
    T: Serialize,
{
    serde_json::to_value(value).unwrap_or_else(|err| {
        JsonValue::String(format!("failed to serialize {tool_name} result: {err}"))
    })
}

pub(crate) fn build_wait_agent_statuses(
    statuses: &HashMap<ThreadId, AgentStatus>,
    receiver_agents: &[CollabAgentRef],
) -> Vec<CollabAgentStatusEntry> {
    if statuses.is_empty() {
        return Vec::new();
    }

    let mut entries = Vec::with_capacity(statuses.len());
    let mut seen = HashMap::with_capacity(receiver_agents.len());
    for receiver_agent in receiver_agents {
        seen.insert(receiver_agent.thread_id, ());
        if let Some(status) = statuses.get(&receiver_agent.thread_id) {
            entries.push(CollabAgentStatusEntry {
                thread_id: receiver_agent.thread_id,
                agent_base_name: receiver_agent.agent_base_name.clone(),
                agent_title: receiver_agent.agent_title.clone(),
                agent_display_name: receiver_agent.agent_display_name.clone(),
                agent_role: receiver_agent.agent_role.clone(),
                status: status.clone(),
            });
        }
    }

    let mut extras = statuses
        .iter()
        .filter(|(thread_id, _)| !seen.contains_key(thread_id))
        .map(|(thread_id, status)| CollabAgentStatusEntry {
            thread_id: *thread_id,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            status: status.clone(),
        })
        .collect::<Vec<_>>();
    extras.sort_by(|left, right| left.thread_id.to_string().cmp(&right.thread_id.to_string()));
    entries.extend(extras);
    entries
}

pub(crate) fn collab_spawn_error(err: PraxisErr) -> FunctionCallError {
    match err {
        PraxisErr::UnsupportedOperation(message) if message == "thread manager dropped" => {
            FunctionCallError::RespondToModel("collab manager unavailable".to_string())
        }
        PraxisErr::UnsupportedOperation(message) => FunctionCallError::RespondToModel(message),
        err => FunctionCallError::RespondToModel(format!("collab spawn failed: {err}")),
    }
}

pub(crate) fn collab_agent_error(agent_id: ThreadId, err: PraxisErr) -> FunctionCallError {
    match err {
        PraxisErr::ThreadNotFound(id) => {
            FunctionCallError::RespondToModel(format!("agent with id {id} not found"))
        }
        PraxisErr::InternalAgentDied => {
            FunctionCallError::RespondToModel(format!("agent with id {agent_id} is closed"))
        }
        PraxisErr::UnsupportedOperation(_) => {
            FunctionCallError::RespondToModel("collab manager unavailable".to_string())
        }
        err => FunctionCallError::RespondToModel(format!("collab tool failed: {err}")),
    }
}

pub(crate) fn thread_spawn_source(
    parent_thread_id: ThreadId,
    parent_session_source: &SessionSource,
    depth: i32,
    agent_role: Option<&str>,
    task_name: Option<String>,
    agent_title: Option<String>,
) -> Result<SessionSource, FunctionCallError> {
    let agent_path = task_name
        .as_deref()
        .map(|task_name| {
            parent_session_source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root)
                .join(task_name)
                .map_err(FunctionCallError::RespondToModel)
        })
        .transpose()?;
    Ok(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id,
        depth,
        agent_path,
        agent_base_name: None,
        agent_title,
        agent_display_name: None,
        agent_role: agent_role.map(str::to_string),
    }))
}

pub(crate) fn parse_collab_input(
    message: Option<String>,
    items: Option<Vec<UserInput>>,
) -> Result<Op, FunctionCallError> {
    match (message, items) {
        (Some(_), Some(_)) => Err(FunctionCallError::RespondToModel(
            "Provide either message or items, but not both".to_string(),
        )),
        (None, None) => Err(FunctionCallError::RespondToModel(
            "Provide one of: message or items".to_string(),
        )),
        (Some(message), None) => {
            if message.trim().is_empty() {
                return Err(FunctionCallError::RespondToModel(
                    "Empty message can't be sent to an agent".to_string(),
                ));
            }
            Ok(vec![UserInput::Text {
                text: message,
                text_elements: Vec::new(),
            }]
            .into())
        }
        (None, Some(items)) => {
            if items.is_empty() {
                return Err(FunctionCallError::RespondToModel(
                    "Items can't be empty".to_string(),
                ));
            }
            Ok(items.into())
        }
    }
}

/// Builds the base config snapshot for a newly spawned sub-agent.
///
/// The returned config starts from the parent's effective config and then refreshes the
/// runtime-owned fields carried on `turn`, including model selection, reasoning settings,
/// approval policy, sandbox, and cwd. Role-specific overrides are layered after this step;
/// skipping this helper and cloning stale config state directly can send the child agent out with
/// the wrong provider or runtime policy.
pub(crate) fn build_agent_spawn_config(turn: &TurnContext) -> Result<Config, FunctionCallError> {
    build_agent_shared_config(turn)
}

fn build_agent_shared_config(turn: &TurnContext) -> Result<Config, FunctionCallError> {
    let base_config = turn.config.clone();
    let mut config = (*base_config).clone();
    config.model = Some(turn.model_info.slug.clone());
    config.model_provider_id = turn.config.model_provider_id.clone();
    config.model_provider = turn.provider.clone();
    config.model_reasoning_effort = turn.reasoning_effort;
    config.model_reasoning_summary = Some(turn.reasoning_summary);
    config.developer_instructions = turn.developer_instructions.clone();
    config.compact_prompt = turn.compact_prompt.clone();
    apply_spawn_agent_runtime_overrides(&mut config, turn)?;

    Ok(config)
}

/// Copies runtime-only turn state onto a child config before it is handed to `AgentControl`.
///
/// These values are chosen by the live turn rather than persisted config, so leaving them stale
/// can make a child agent disagree with its parent about approval policy, cwd, or sandboxing.
pub(crate) fn apply_spawn_agent_runtime_overrides(
    config: &mut Config,
    turn: &TurnContext,
) -> Result<(), FunctionCallError> {
    let permissions = turn.effective_permissions();
    config
        .permissions
        .approval_policy
        .set(permissions.approval_policy.value())
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!("approval_policy is invalid: {err}"))
        })?;
    config.permissions.shell_environment_policy = turn.shell_environment_policy.clone();
    config.praxis_linux_sandbox_exe = turn.praxis_linux_sandbox_exe.clone();
    config.cwd = turn.cwd.clone();
    config
        .permissions
        .sandbox_policy
        .set(permissions.sandbox_policy.get().clone())
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!("sandbox_policy is invalid: {err}"))
        })?;
    config.permissions.file_system_sandbox_policy = permissions.file_system_sandbox_policy;
    config.permissions.network_sandbox_policy = permissions.network_sandbox_policy;
    Ok(())
}

pub(crate) fn apply_spawn_agent_overrides(config: &mut Config, child_depth: i32) {
    if child_depth >= config.agent_max_depth {
        let _ = config.features.disable(Feature::SpawnCsv);
        let _ = config.features.disable(Feature::Collab);
    }
}

pub(crate) async fn apply_requested_spawn_agent_model_overrides(
    session: &Session,
    turn: &TurnContext,
    config: &mut Config,
    requested_model_provider: Option<&str>,
    requested_model: Option<&str>,
    requested_reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), FunctionCallError> {
    let requested_model_provider = requested_model_provider
        .map(str::trim)
        .filter(|provider| !provider.is_empty());
    let requested_model = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty());
    let requested_reasoning_effort = requested_reasoning_effort
        .or_else(|| requested_model.and_then(spawn_agent_embedded_reasoning_effort));

    if requested_model_provider.is_none()
        && requested_model.is_none()
        && requested_reasoning_effort.is_none()
    {
        return Ok(());
    }

    if let Some(provider_selector) = requested_model_provider {
        apply_spawn_agent_model_provider_override(config, provider_selector)?;
    }

    let model_candidates = requested_model
        .map(|model| spawn_agent_model_candidates(model, &config.notices.model_migrations))
        .unwrap_or_default();
    if requested_model_provider.is_none() {
        infer_spawn_agent_model_provider(config, &model_candidates);
    }

    if let Some(requested_model) = requested_model {
        let available_models = session
            .services
            .models_manager
            .list_models_for_config(config, RefreshStrategy::Offline)
            .await;
        let selected_model_name = resolve_spawn_agent_model_name(
            &available_models,
            requested_model,
            &config.notices.model_migrations,
            config,
        )?;
        let selected_model_info = session
            .services
            .models_manager
            .get_model_info(&selected_model_name, config)
            .await;

        config.model = Some(selected_model_name.clone());
        if let Some(reasoning_effort) = requested_reasoning_effort {
            validate_spawn_agent_reasoning_effort(
                &selected_model_name,
                &selected_model_info.supported_reasoning_levels,
                reasoning_effort,
            )?;
            config.model_reasoning_effort = Some(reasoning_effort);
        } else {
            config.model_reasoning_effort = selected_model_info.default_reasoning_level;
        }

        return Ok(());
    }

    if requested_model_provider.is_some() {
        let available_models = session
            .services
            .models_manager
            .list_models_for_config(config, RefreshStrategy::Offline)
            .await;
        let selected_model_name =
            select_spawn_agent_provider_default_model(&available_models, config)?;
        let selected_model_info = session
            .services
            .models_manager
            .get_model_info(&selected_model_name, config)
            .await;
        config.model = Some(selected_model_name.clone());
        if let Some(reasoning_effort) = requested_reasoning_effort {
            validate_spawn_agent_reasoning_effort(
                &selected_model_name,
                &selected_model_info.supported_reasoning_levels,
                reasoning_effort,
            )?;
            config.model_reasoning_effort = Some(reasoning_effort);
        } else {
            config.model_reasoning_effort = selected_model_info.default_reasoning_level;
        }

        return Ok(());
    }

    if let Some(reasoning_effort) = requested_reasoning_effort {
        validate_spawn_agent_reasoning_effort(
            &turn.model_info.slug,
            &turn.model_info.supported_reasoning_levels,
            reasoning_effort,
        )?;
        config.model_reasoning_effort = Some(reasoning_effort);
    }

    Ok(())
}

fn apply_spawn_agent_model_provider_override(
    config: &mut Config,
    requested_model_provider: &str,
) -> Result<(), FunctionCallError> {
    let (provider_id, provider) =
        resolve_spawn_agent_model_provider(config, requested_model_provider)?;
    config.model_provider_id = provider_id;
    config.model_provider = provider;
    Ok(())
}

fn resolve_spawn_agent_model_provider(
    config: &Config,
    requested_model_provider: &str,
) -> Result<(String, ModelProviderInfo), FunctionCallError> {
    for selector in spawn_agent_model_provider_candidates(requested_model_provider) {
        if let Some((provider_id, provider)) = config
            .model_providers
            .iter()
            .find(|(provider_id, _)| provider_id.eq_ignore_ascii_case(selector.as_str()))
        {
            return Ok((provider_id.clone(), provider.clone()));
        }

        if let Some((provider_id, provider)) = config
            .model_providers
            .iter()
            .find(|(_, provider)| provider.name.eq_ignore_ascii_case(selector.as_str()))
        {
            return Ok((provider_id.clone(), provider.clone()));
        }
    }

    let mut available = config
        .model_providers
        .iter()
        .map(|(provider_id, provider)| format!("{provider_id} ({})", provider.name))
        .collect::<Vec<_>>();
    available.sort();
    Err(FunctionCallError::RespondToModel(format!(
        "Unknown model_provider `{requested_model_provider}` for spawn_agent. Available providers: {}",
        available.join(", ")
    )))
}

fn spawn_agent_model_provider_candidates(requested_model_provider: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let trimmed = requested_model_provider.trim();
    push_spawn_agent_model_candidate(&mut candidates, trimmed.to_string());
    let normalized = trimmed
        .chars()
        .filter(|ch| !matches!(ch, '-' | '_' | '/' | ' '))
        .collect::<String>()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "responses" | "openairesponses" | "openai" | "gpt" | "chatgpt" => {
            push_spawn_agent_model_candidate(&mut candidates, OPENAI_PROVIDER_ID.to_string());
            push_spawn_agent_model_candidate(&mut candidates, "OpenAI".to_string());
        }
        _ => {}
    }
    candidates
}

fn infer_spawn_agent_model_provider(config: &mut Config, model_candidates: &[String]) {
    if model_candidates.is_empty() {
        return;
    }

    let registry = LlmProfileRegistry::builtin_static();
    for candidate in model_candidates {
        if let Some(provider_switch) = registry.provider_switch_for_selected_model(
            config.model_provider_id.as_str(),
            &config.model_provider,
            candidate,
            &config.model_providers,
        ) {
            config.model_provider_id = provider_switch.provider_id;
            config.model_provider = provider_switch.provider;
            return;
        }
    }
}

fn select_spawn_agent_provider_default_model(
    available_models: &[ModelPreset],
    config: &Config,
) -> Result<String, FunctionCallError> {
    if let Some(current_model) = config.model.as_deref()
        && let Some(model) = available_models
            .iter()
            .find(|model| model.model == current_model)
    {
        return Ok(model.model.clone());
    }

    if let Some(model) = available_models
        .iter()
        .find(|model| model.is_default)
        .or_else(|| available_models.first())
    {
        return Ok(model.model.clone());
    }

    Err(FunctionCallError::RespondToModel(format!(
        "No available models for spawn_agent provider `{}`.",
        config.model_provider_id
    )))
}

fn resolve_spawn_agent_model_name(
    available_models: &[ModelPreset],
    requested_model: &str,
    model_migrations: &BTreeMap<String, String>,
    config: &Config,
) -> Result<String, FunctionCallError> {
    let candidates = spawn_agent_model_candidates(requested_model, model_migrations);
    for candidate in &candidates {
        if let Some(model) = available_models
            .iter()
            .find(|model| model.model == *candidate)
        {
            return Ok(model.model.clone());
        }
    }

    let registry = LlmProfileRegistry::builtin_static();
    for candidate in &candidates {
        if known_openai_compatible_model_info(candidate).is_some()
            && registry.provider_accepts_known_first_party_model(
                config.model_provider_id.as_str(),
                &config.model_provider,
                candidate,
            )
        {
            return Ok(candidate.clone());
        }
    }

    let available = available_models
        .iter()
        .map(|model| model.model.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let candidate_note = if candidates.len() > 1 {
        format!(" Resolved candidates: {}.", candidates.join(", "))
    } else {
        String::new()
    };
    let provider_id = config.model_provider_id.as_str();
    Err(FunctionCallError::RespondToModel(format!(
        "Unknown model `{requested_model}` for spawn_agent provider `{provider_id}`.{candidate_note} Available models: {available}"
    )))
}

fn spawn_agent_model_candidates(
    requested_model: &str,
    model_migrations: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut candidates = Vec::new();
    push_spawn_agent_model_candidate(&mut candidates, requested_model.trim().to_string());

    for normalized in normalize_spawn_agent_model_aliases(requested_model) {
        push_spawn_agent_model_candidate(&mut candidates, normalized);
    }

    let pre_migration_candidates = candidates.clone();
    for candidate in pre_migration_candidates {
        if let Some(migrated) = model_migrations.get(&candidate) {
            push_spawn_agent_model_candidate(&mut candidates, migrated.clone());
        }
    }

    candidates
}

fn normalize_spawn_agent_model_aliases(requested_model: &str) -> Vec<String> {
    let compact = requested_model.split_whitespace().collect::<String>();
    if compact.is_empty() {
        return Vec::new();
    }

    let lower = compact.replace('_', "-").to_ascii_lowercase();
    let normalized = if lower
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        format!("gpt-{lower}")
    } else if let Some(rest) = lower.strip_prefix("gpt-") {
        format!("gpt-{rest}")
    } else if let Some(rest) = lower.strip_prefix("gpt") {
        if rest
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
        {
            format!("gpt-{rest}")
        } else {
            lower
        }
    } else {
        lower
    };

    let mut aliases = Vec::new();
    if normalized != requested_model.trim() {
        aliases.push(normalized.clone());
    }

    if let Some(stripped) = strip_embedded_reasoning_suffix(normalized.as_str())
        && stripped != normalized
        && stripped != requested_model.trim()
    {
        aliases.push(stripped);
    }

    aliases
}

fn normalize_openai_model_alias(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches('-');
    if rest
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        Some(format!("gpt-{rest}"))
    } else {
        None
    }
}

fn strip_embedded_reasoning_suffix(model: &str) -> Option<String> {
    for suffix in ["-xhigh", "-x-high", "-high", "xhigh", "x-high"] {
        if let Some(base) = model.strip_suffix(suffix)
            && !base.is_empty()
        {
            return Some(base.trim_end_matches('-').to_string());
        }
    }
    None
}

fn spawn_agent_embedded_reasoning_effort(requested_model: &str) -> Option<ReasoningEffort> {
    let normalized = requested_model
        .split_whitespace()
        .collect::<String>()
        .replace('_', "-")
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if normalized.ends_with("-xhigh")
        || normalized.ends_with("-x-high")
        || normalized.ends_with("xhigh")
        || normalized.ends_with("x-high")
    {
        return Some(ReasoningEffort::XHigh);
    }
    if normalized.ends_with("-high") || normalized.ends_with("high") {
        return Some(ReasoningEffort::High);
    }
    None
}

fn push_spawn_agent_model_candidate(candidates: &mut Vec<String>, candidate: String) {
    if candidate.is_empty() || candidates.iter().any(|existing| existing == &candidate) {
        return;
    }
    candidates.push(candidate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_provider_info::WireApi;

    #[test]
    fn spawn_agent_model_candidates_accept_openai_aliases() {
        let migrations = BTreeMap::new();
        assert_eq!(
            spawn_agent_model_candidates("5.5", &migrations),
            vec!["5.5".to_string(), "gpt-5.5".to_string()]
        );
        assert!(
            spawn_agent_model_candidates("gpt5.5", &migrations).contains(&"gpt-5.5".to_string())
        );
        assert!(
            spawn_agent_model_candidates("gpt 5.5 xhigh", &migrations)
                .contains(&"gpt-5.5".to_string())
        );
        assert_eq!(
            spawn_agent_embedded_reasoning_effort("gpt 5.5 xhigh"),
            Some(ReasoningEffort::XHigh)
        );
        assert_eq!(
            spawn_agent_embedded_reasoning_effort("gpt5.5-high"),
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn spawn_agent_model_provider_candidates_accept_openai_aliases() {
        assert!(
            spawn_agent_model_provider_candidates("openai/responses")
                .contains(&OPENAI_PROVIDER_ID.to_string())
        );
        assert!(
            spawn_agent_model_provider_candidates("openai")
                .contains(&OPENAI_PROVIDER_ID.to_string())
        );
        assert!(
            spawn_agent_model_provider_candidates("responses")
                .contains(&OPENAI_PROVIDER_ID.to_string())
        );
    }

    #[test]
    fn spawn_agent_model_alias_can_switch_deepseek_thread_to_openai_gpt55() {
        let mut config = crate::config::test_config();
        let deepseek_provider = ModelProviderInfo {
            name: "DeepSeek".to_string(),
            base_url: Some("https://api.deepseek.com".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::OpenAiCompat,
            compat: None,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: false,
        };
        config
            .model_providers
            .insert("deepseek".to_string(), deepseek_provider.clone());
        config.model_provider_id = "deepseek".to_string();
        config.model_provider = deepseek_provider;

        let migrations = BTreeMap::new();
        let candidates = spawn_agent_model_candidates("gpt 5.5 xhigh", &migrations);
        infer_spawn_agent_model_provider(&mut config, &candidates);

        assert_eq!(config.model_provider_id, OPENAI_PROVIDER_ID);
        assert!(config.model_provider.is_openai());
        assert_eq!(
            resolve_spawn_agent_model_name(&[], "gpt 5.5 xhigh", &migrations, &config)
                .expect("known GPT-5.5 alias should resolve after provider inference"),
            "gpt-5.5"
        );
        assert_eq!(
            spawn_agent_embedded_reasoning_effort("gpt 5.5 xhigh"),
            Some(ReasoningEffort::XHigh)
        );
    }
}

fn validate_spawn_agent_reasoning_effort(
    model: &str,
    supported_reasoning_levels: &[ReasoningEffortPreset],
    requested_reasoning_effort: ReasoningEffort,
) -> Result<(), FunctionCallError> {
    if supported_reasoning_levels
        .iter()
        .any(|preset| preset.effort == requested_reasoning_effort)
    {
        return Ok(());
    }

    let supported = supported_reasoning_levels
        .iter()
        .map(|preset| preset.effort.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(FunctionCallError::RespondToModel(format!(
        "Reasoning effort `{requested_reasoning_effort}` is not supported for model `{model}`. Supported reasoning efforts: {supported}"
    )))
}
