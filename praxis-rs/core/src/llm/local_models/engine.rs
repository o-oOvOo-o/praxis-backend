use super::catalog::LocalModelEntry;
use super::catalog::LocalModelArchitecture;
use super::catalog::LocalModelFormat;
use super::catalog::LocalModelWire;
use super::catalog::NativeLocalModelConfig;
use super::catalog::resolve_local_model_from_runtime_config;
use super::managed_server::ensure_managed_llama_gpu_server;
use super::managed_server::ensure_v1_base_url;
use super::managed_server::local_max_tokens;
use super::managed_server::local_stream_idle_timeout_ms;
use super::output_filter::filter_native_local_output;
use crate::client_common::Prompt;
use crate::client_common::ResponseStream;
use crate::config::LocalModelHostConfig;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::model_provider_info::ModelProviderCompatInfo;
use crate::model_provider_info::ModelProviderMaxTokensField;
use crate::model_provider_info::ModelProviderThinkingFormat;
use crate::model_provider_info::create_native_local_provider;
use crate::provider_decision_center::AuthRequestPurpose;
use crate::provider_decision_center::ProviderDecisionCenter;
use praxis_protocol::openai_models::ModelInfo;
use serde_json::Value;

pub(crate) async fn stream_native_local_model(
    config: NativeLocalModelConfig,
    prompt: &Prompt,
    model_info: &ModelInfo,
) -> PraxisResult<ResponseStream> {
    let entry = resolve_local_model_from_runtime_config(&config, model_info.slug.as_str())
        .ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "local GPU model `{}` is not configured or was not discovered",
                model_info.slug
            ))
        })?;
    validate_runtime_entry(&entry)?;

    let host = entry
        .host_id
        .as_ref()
        .and_then(|host_id| config.local_model_hosts.get(host_id));
    let api_base_url = match entry.wire {
        LocalModelWire::LlamaCppGpu => ensure_managed_llama_gpu_server(&entry, host).await?,
        LocalModelWire::ExternalOpenAiCompat => external_openai_compat_base_url(&entry, host)?,
        LocalModelWire::Unsupported => unreachable!("validated local model wire"),
    };

    let provider_info = local_gpu_provider_info(api_base_url, &entry, host);
    let setup = ProviderDecisionCenter::new(None)
        .setup_provider(&provider_info, AuthRequestPurpose::ModelTurn)
        .await?;
    let stream = crate::non_responses_transport::stream_common_unary(
        setup.api_provider,
        setup.api_auth,
        &provider_info,
        prompt,
        model_info,
        None,
    )
    .await?;
    Ok(filter_native_local_output(stream))
}

fn validate_runtime_entry(entry: &LocalModelEntry) -> PraxisResult<()> {
    if entry.format != LocalModelFormat::Gguf {
        return Err(PraxisErr::UnsupportedOperation(format!(
            "local GPU inference currently supports GGUF only: {}",
            entry.model_path.display()
        )));
    }
    if !entry.runtime_supported {
        return Err(PraxisErr::UnsupportedOperation(format!(
            "local model `{}` is cataloged but not runnable; `native_engine` CPU inference has been removed, use a GPU `managed_server` or `external_http` host",
            entry.model_path.display()
        )));
    }
    Ok(())
}

fn external_openai_compat_base_url(
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
) -> PraxisResult<String> {
    let host = host.ok_or_else(|| {
        PraxisErr::UnsupportedOperation(format!(
            "local model `{}` has no external HTTP host config",
            entry.model_path.display()
        ))
    })?;
    let base_url = host.base_url.as_ref().ok_or_else(|| {
        PraxisErr::UnsupportedOperation(format!(
            "external local model host for `{}` must set base_url",
            entry.model_path.display()
        ))
    })?;
    Ok(ensure_v1_base_url(base_url))
}

fn local_gpu_provider_info(
    base_url: String,
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
) -> crate::model_provider_info::ModelProviderInfo {
    let mut provider = create_native_local_provider();
    provider.name = "Praxis Local GPU".to_string();
    provider.base_url = Some(ensure_v1_base_url(&base_url));
    provider.env_key = host
        .and_then(|host| host.api_key_env.as_ref())
        .filter(|key| !key.trim().is_empty())
        .cloned();
    provider.stream_idle_timeout_ms = Some(local_stream_idle_timeout_ms(host));
    provider.compat = Some(ModelProviderCompatInfo {
        supports_developer_role: Some(metadata_bool(host, "supports_developer_role").unwrap_or(false)),
        supports_reasoning_effort: Some(metadata_bool(host, "supports_reasoning_effort").unwrap_or(false)),
        supports_parallel_tool_calls: Some(
            metadata_bool(host, "supports_parallel_tool_calls").unwrap_or(false),
        ),
        max_tokens_field: Some(ModelProviderMaxTokensField::MaxTokens),
        max_tokens: Some(local_max_tokens(host)),
        requires_tool_result_name: metadata_bool(host, "requires_tool_result_name"),
        requires_assistant_after_tool_result: metadata_bool(
            host,
            "requires_assistant_after_tool_result",
        ),
        thinking_format: Some(local_thinking_format(entry, host)),
        ..ModelProviderCompatInfo::default()
    });
    provider
}

fn local_thinking_format(
    entry: &LocalModelEntry,
    host: Option<&LocalModelHostConfig>,
) -> ModelProviderThinkingFormat {
    metadata_string(host, "thinking_format")
        .and_then(|value| parse_thinking_format(&value))
        .unwrap_or_else(|| inferred_local_thinking_format(entry))
}

fn inferred_local_thinking_format(entry: &LocalModelEntry) -> ModelProviderThinkingFormat {
    match entry.wire {
        LocalModelWire::LlamaCppGpu => ModelProviderThinkingFormat::ChatTemplateKwargs,
        LocalModelWire::ExternalOpenAiCompat => match entry.architecture {
            LocalModelArchitecture::Qwen2 | LocalModelArchitecture::Qwen3 => {
                ModelProviderThinkingFormat::QwenChatTemplate
            }
            _ => ModelProviderThinkingFormat::Openai,
        },
        LocalModelWire::Unsupported => ModelProviderThinkingFormat::Openai,
    }
}

fn parse_thinking_format(value: &str) -> Option<ModelProviderThinkingFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Some(ModelProviderThinkingFormat::Openai),
        "openrouter" => Some(ModelProviderThinkingFormat::Openrouter),
        "deepseek" => Some(ModelProviderThinkingFormat::Deepseek),
        "gemini" => Some(ModelProviderThinkingFormat::Gemini),
        "zai" => Some(ModelProviderThinkingFormat::Zai),
        "qwen" => Some(ModelProviderThinkingFormat::Qwen),
        "qwen_chat_template" => Some(ModelProviderThinkingFormat::QwenChatTemplate),
        "chat_template_kwargs" | "llama_cpp_chat_template" => {
            Some(ModelProviderThinkingFormat::ChatTemplateKwargs)
        }
        _ => None,
    }
}

fn metadata_bool(host: Option<&LocalModelHostConfig>, key: &str) -> Option<bool> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => value.trim().parse::<bool>().ok(),
            _ => None,
        })
}

fn metadata_string(host: Option<&LocalModelHostConfig>, key: &str) -> Option<String> {
    host.and_then(|host| host.metadata.get(key))
        .and_then(|value| match value {
            Value::String(value) => Some(value.trim().to_owned()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}
