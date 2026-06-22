use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::LocalModelHostConfig;
use crate::config::LocalModelHostKind;

const DEFAULT_LOCAL_MODEL: &str = "local-model";
const DEFAULT_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_MANAGED_HEALTH_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MANAGED_HEALTH_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_TEMPERATURE: f32 = 0.1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalModelHostRegistryError {
    UnknownHost(String),
    MissingBaseUrl(String),
    MissingCommand(String),
    MissingNativeModelPath(String),
    EmptyMessages,
    ManagedServerStart {
        host_id: String,
        message: String,
    },
    ManagedServerHealth {
        host_id: String,
        url: String,
        message: String,
    },
    HttpRequest {
        host_id: String,
        url: String,
        message: String,
    },
    HttpStatus {
        host_id: String,
        status: u16,
        body: String,
    },
    MalformedResponse {
        host_id: String,
        message: String,
    },
    NativeEngineUnavailable {
        host_id: String,
        message: String,
    },
    UnexpectedToolCalls(String),
}

impl std::fmt::Display for LocalModelHostRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownHost(id) => write!(f, "unknown local model host `{id}`"),
            Self::MissingBaseUrl(id) => write!(f, "local model host `{id}` is missing base_url"),
            Self::MissingCommand(id) => write!(f, "local model host `{id}` is missing command"),
            Self::MissingNativeModelPath(id) => {
                write!(f, "local model host `{id}` is missing model_path")
            }
            Self::EmptyMessages => write!(f, "local model chat request has no messages"),
            Self::ManagedServerStart { host_id, message } => {
                write!(
                    f,
                    "failed to start managed local model host `{host_id}`: {message}"
                )
            }
            Self::ManagedServerHealth {
                host_id,
                url,
                message,
            } => {
                write!(
                    f,
                    "managed local model host `{host_id}` failed health check at {url}: {message}"
                )
            }
            Self::HttpRequest {
                host_id,
                url,
                message,
            } => {
                write!(
                    f,
                    "local model host `{host_id}` request to {url} failed: {message}"
                )
            }
            Self::HttpStatus {
                host_id,
                status,
                body,
            } => {
                write!(
                    f,
                    "local model host `{host_id}` returned HTTP {status}: {body}"
                )
            }
            Self::MalformedResponse { host_id, message } => {
                write!(
                    f,
                    "local model host `{host_id}` returned a malformed response: {message}"
                )
            }
            Self::NativeEngineUnavailable { host_id, message } => {
                write!(
                    f,
                    "native local model host `{host_id}` is unavailable: {message}"
                )
            }
            Self::UnexpectedToolCalls(host_id) => {
                write!(
                    f,
                    "local model host `{host_id}` returned tool calls where text was expected"
                )
            }
        }
    }
}

impl std::error::Error for LocalModelHostRegistryError {}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocalModelHostEndpoint {
    ExternalHttp {
        id: String,
        base_url: String,
        api_key_env: Option<String>,
        models: Vec<String>,
    },
    ManagedServer {
        id: String,
        base_url: String,
        api_key_env: Option<String>,
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        health_path: Option<String>,
        idle_timeout_ms: Option<u64>,
        models: Vec<String>,
    },
    NativeEngine {
        id: String,
        model_path: String,
        tokenizer_path: Option<String>,
        models: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalModelChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

impl LocalModelChatMessage {
    pub(crate) fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocalModelChatRequest {
    pub(crate) host_id: String,
    pub(crate) model: Option<String>,
    pub(crate) messages: Vec<LocalModelChatMessage>,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) tools: Vec<Value>,
}

impl LocalModelChatRequest {
    pub(crate) fn prompt(host_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            host_id: host_id.into(),
            model: None,
            messages: vec![LocalModelChatMessage::user(prompt)],
            temperature: Some(DEFAULT_TEMPERATURE),
            top_p: None,
            max_tokens: None,
            tools: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocalModelChatOutput {
    Text(String),
    ToolCalls(Vec<LocalModelToolCall>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocalModelToolCall {
    pub(crate) name: String,
    pub(crate) arguments: Value,
}

struct ManagedServerLease {
    child: Child,
}

#[derive(Clone)]
pub(crate) struct LocalModelHostRegistry {
    hosts: BTreeMap<String, LocalModelHostConfig>,
    client: Client,
    managed_servers: Arc<Mutex<BTreeMap<String, ManagedServerLease>>>,
}

impl fmt::Debug for LocalModelHostRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalModelHostRegistry")
            .field("hosts", &self.hosts)
            .finish_non_exhaustive()
    }
}

impl Default for LocalModelHostRegistry {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl LocalModelHostRegistry {
    pub(crate) fn new(hosts: BTreeMap<String, LocalModelHostConfig>) -> Self {
        Self {
            hosts,
            client: Client::builder()
                .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
                .timeout(DEFAULT_HTTP_TIMEOUT)
                .build()
                .unwrap_or_else(|_| Client::new()),
            managed_servers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    pub(crate) fn host_ids(&self) -> impl Iterator<Item = &str> {
        self.hosts.keys().map(String::as_str)
    }

    pub(crate) fn get(&self, host_id: &str) -> Option<&LocalModelHostConfig> {
        self.hosts.get(host_id)
    }

    pub(crate) fn resolve(
        &self,
        host_id: &str,
    ) -> Result<LocalModelHostEndpoint, LocalModelHostRegistryError> {
        let host = self
            .hosts
            .get(host_id)
            .ok_or_else(|| LocalModelHostRegistryError::UnknownHost(host_id.to_owned()))?;

        match host.kind {
            LocalModelHostKind::ExternalHttp => {
                let base_url = host.base_url.clone().ok_or_else(|| {
                    LocalModelHostRegistryError::MissingBaseUrl(host_id.to_owned())
                })?;
                Ok(LocalModelHostEndpoint::ExternalHttp {
                    id: host_id.to_owned(),
                    base_url,
                    api_key_env: host.api_key_env.clone(),
                    models: host.models.clone(),
                })
            }
            LocalModelHostKind::ManagedServer => {
                let base_url = host.base_url.clone().ok_or_else(|| {
                    LocalModelHostRegistryError::MissingBaseUrl(host_id.to_owned())
                })?;
                let command = host.command.clone().ok_or_else(|| {
                    LocalModelHostRegistryError::MissingCommand(host_id.to_owned())
                })?;
                Ok(LocalModelHostEndpoint::ManagedServer {
                    id: host_id.to_owned(),
                    base_url,
                    api_key_env: host.api_key_env.clone(),
                    command,
                    args: host.args.clone(),
                    env: host.env.clone(),
                    health_path: host.health_path.clone(),
                    idle_timeout_ms: host.idle_timeout_ms,
                    models: host.models.clone(),
                })
            }
            LocalModelHostKind::NativeEngine => {
                let model_path = host
                    .model_path
                    .as_ref()
                    .ok_or_else(|| {
                        LocalModelHostRegistryError::MissingNativeModelPath(host_id.to_owned())
                    })?
                    .to_string_lossy()
                    .into_owned();
                Ok(LocalModelHostEndpoint::NativeEngine {
                    id: host_id.to_owned(),
                    model_path,
                    tokenizer_path: host
                        .tokenizer_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    models: host.models.clone(),
                })
            }
        }
    }

    pub(crate) async fn chat_once(
        &self,
        request: LocalModelChatRequest,
    ) -> Result<LocalModelChatOutput, LocalModelHostRegistryError> {
        if request.messages.is_empty() {
            return Err(LocalModelHostRegistryError::EmptyMessages);
        }
        let endpoint = self.resolve(&request.host_id)?;
        match endpoint {
            LocalModelHostEndpoint::ExternalHttp {
                id,
                base_url,
                api_key_env,
                models,
            } => {
                let model = resolve_model(request.model.as_deref(), &models);
                self.chat_completions(&id, &base_url, api_key_env.as_deref(), &model, &request)
                    .await
            }
            LocalModelHostEndpoint::ManagedServer {
                id,
                base_url,
                api_key_env,
                command,
                args,
                env,
                health_path,
                idle_timeout_ms: _,
                models,
            } => {
                self.ensure_managed_server(&id, &base_url, &command, &args, &env, health_path)
                    .await?;
                let model = resolve_model(request.model.as_deref(), &models);
                self.chat_completions(&id, &base_url, api_key_env.as_deref(), &model, &request)
                    .await
            }
            LocalModelHostEndpoint::NativeEngine { id, .. } => {
                Err(LocalModelHostRegistryError::NativeEngineUnavailable {
                    host_id: id,
                    message:
                        "native GGUF engine is registered but no runtime adapter is linked yet"
                            .to_string(),
                })
            }
        }
    }

    pub(crate) async fn chat_text_once(
        &self,
        request: LocalModelChatRequest,
    ) -> Result<String, LocalModelHostRegistryError> {
        let host_id = request.host_id.clone();
        match self.chat_once(request).await? {
            LocalModelChatOutput::Text(text) => Ok(text),
            LocalModelChatOutput::ToolCalls(_) => {
                Err(LocalModelHostRegistryError::UnexpectedToolCalls(host_id))
            }
        }
    }

    pub(crate) async fn shutdown_managed_server(&self, host_id: &str) -> bool {
        let mut servers = self.managed_servers.lock().await;
        let Some(mut lease) = servers.remove(host_id) else {
            return false;
        };
        let _ = lease.child.start_kill();
        true
    }

    async fn ensure_managed_server(
        &self,
        host_id: &str,
        base_url: &str,
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        health_path: Option<String>,
    ) -> Result<(), LocalModelHostRegistryError> {
        {
            let mut servers = self.managed_servers.lock().await;
            if let Some(lease) = servers.get_mut(host_id) {
                match lease.child.try_wait() {
                    Ok(None) => return Ok(()),
                    Ok(Some(_)) | Err(_) => {
                        servers.remove(host_id);
                    }
                }
            }

            let mut cmd = Command::new(command);
            cmd.args(args).envs(env);
            let child =
                cmd.spawn()
                    .map_err(|err| LocalModelHostRegistryError::ManagedServerStart {
                        host_id: host_id.to_string(),
                        message: err.to_string(),
                    })?;
            servers.insert(host_id.to_string(), ManagedServerLease { child });
        }

        if let Some(health_path) = health_path {
            self.wait_for_health(host_id, base_url, &health_path)
                .await?;
        }
        Ok(())
    }

    async fn wait_for_health(
        &self,
        host_id: &str,
        base_url: &str,
        health_path: &str,
    ) -> Result<(), LocalModelHostRegistryError> {
        let url = join_url(base_url, health_path);
        let start = Instant::now();
        let mut last_error = String::new();
        while start.elapsed() < DEFAULT_MANAGED_HEALTH_TIMEOUT {
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) => {
                    last_error = format!("HTTP {}", resp.status());
                }
                Err(err) => {
                    last_error = err.to_string();
                }
            }
            tokio::time::sleep(DEFAULT_MANAGED_HEALTH_INTERVAL).await;
        }
        Err(LocalModelHostRegistryError::ManagedServerHealth {
            host_id: host_id.to_string(),
            url,
            message: last_error,
        })
    }

    async fn chat_completions(
        &self,
        host_id: &str,
        base_url: &str,
        api_key_env: Option<&str>,
        model: &str,
        request: &LocalModelChatRequest,
    ) -> Result<LocalModelChatOutput, LocalModelHostRegistryError> {
        let url = join_url(base_url, DEFAULT_CHAT_COMPLETIONS_PATH);
        let messages = request
            .messages
            .iter()
            .map(|message| {
                json!({
                    "role": message.role,
                    "content": message.content,
                })
            })
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(DEFAULT_TEMPERATURE),
            "stream": false,
        });
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools.clone());
            body["tool_choice"] = json!("auto");
        }

        let mut http_request = self.client.post(&url);
        if let Some(api_key) = resolve_api_key(api_key_env) {
            http_request = http_request.bearer_auth(api_key);
        }
        let response = http_request.json(&body).send().await.map_err(|err| {
            LocalModelHostRegistryError::HttpRequest {
                host_id: host_id.to_string(),
                url: url.clone(),
                message: err.to_string(),
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LocalModelHostRegistryError::HttpStatus {
                host_id: host_id.to_string(),
                status: status.as_u16(),
                body,
            });
        }
        let value: Value =
            response
                .json()
                .await
                .map_err(|err| LocalModelHostRegistryError::HttpRequest {
                    host_id: host_id.to_string(),
                    url: url.clone(),
                    message: err.to_string(),
                })?;
        parse_chat_completions_response(host_id, &value)
    }
}

fn resolve_model(requested_model: Option<&str>, endpoint_models: &[String]) -> String {
    requested_model
        .and_then(non_empty)
        .map(str::to_string)
        .or_else(|| {
            endpoint_models
                .iter()
                .find_map(|model| non_empty(model).map(str::to_string))
        })
        .unwrap_or_else(|| DEFAULT_LOCAL_MODEL.to_string())
}

fn resolve_api_key(api_key_env: Option<&str>) -> Option<String> {
    let env_name = api_key_env.and_then(non_empty)?;
    std::env::var(env_name)
        .ok()
        .and_then(|value| non_empty(&value).map(str::to_string))
}

fn parse_chat_completions_response(
    host_id: &str,
    value: &Value,
) -> Result<LocalModelChatOutput, LocalModelHostRegistryError> {
    let message = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| LocalModelHostRegistryError::MalformedResponse {
            host_id: host_id.to_string(),
            message: "missing choices[0].message".to_string(),
        })?;

    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        let tool_calls = calls
            .iter()
            .filter_map(parse_new_tool_call)
            .collect::<Vec<_>>();
        if !tool_calls.is_empty() {
            return Ok(LocalModelChatOutput::ToolCalls(tool_calls));
        }
    }
    if let Some(call) = message.get("function_call").and_then(Value::as_object) {
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !name.is_empty() {
            let raw = call
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            return Ok(LocalModelChatOutput::ToolCalls(vec![LocalModelToolCall {
                name: name.to_string(),
                arguments: parse_tool_arguments(raw),
            }]));
        }
    }

    let text = message
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| LocalModelHostRegistryError::MalformedResponse {
            host_id: host_id.to_string(),
            message: "missing choices[0].message.content".to_string(),
        })?;
    Ok(LocalModelChatOutput::Text(text.to_string()))
}

fn parse_new_tool_call(value: &Value) -> Option<LocalModelToolCall> {
    let function = value.get("function")?;
    let name = function.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let raw = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    Some(LocalModelToolCall {
        name: name.to_string(),
        arguments: parse_tool_arguments(raw),
    })
}

fn parse_tool_arguments(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!({ "_raw": raw }))
}

fn join_url(base_url: &str, path: &str) -> String {
    let path = path.trim();
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let base = base_url.trim_end_matches('/');
    if path.is_empty() {
        return base.to_string();
    }
    format!("{base}/{}", path.trim_start_matches('/'))
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_model_from_request_then_host_then_default() {
        let models = vec!["host-model".to_string()];

        assert_eq!(
            resolve_model(Some("request-model"), &models),
            "request-model"
        );
        assert_eq!(resolve_model(None, &models), "host-model");
        assert_eq!(resolve_model(None, &[]), DEFAULT_LOCAL_MODEL);
    }

    #[test]
    fn parses_openai_compatible_text_response() {
        let value = json!({
            "choices": [{
                "message": {
                    "content": "hello"
                }
            }]
        });

        let output = parse_chat_completions_response("local", &value).unwrap();

        assert_eq!(output, LocalModelChatOutput::Text("hello".to_string()));
    }

    #[test]
    fn parses_openai_compatible_tool_calls() {
        let value = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "create_node",
                            "arguments": "{\"kind\":\"noise\"}"
                        }
                    }]
                }
            }]
        });

        let output = parse_chat_completions_response("local", &value).unwrap();

        assert_eq!(
            output,
            LocalModelChatOutput::ToolCalls(vec![LocalModelToolCall {
                name: "create_node".to_string(),
                arguments: json!({ "kind": "noise" }),
            }])
        );
    }
}
