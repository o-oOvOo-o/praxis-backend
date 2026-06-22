use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::Client;
use reqwest::multipart;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::config::TranscriptionConfig;
use crate::config::TranscriptionProviderConfig;
use crate::config::TranscriptionProviderKind;
use crate::config::TranscriptionSubmitMode;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const DEFAULT_DASHSCOPE_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_DASHSCOPE_API_KEY_ENV: &str = "DASHSCOPE_API_KEY";
const DEFAULT_QWEN_ASR_MODEL: &str = "qwen3-asr-flash";
const DEFAULT_LOCAL_ASR_MODEL: &str = "local-asr";
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const OPENAI_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const OPENAI_AUDIO_TRANSCRIPTIONS_PATH: &str = "/audio/transcriptions";
const LOCAL_PROCESS_PROTOCOL_JSON_STDIN: &str = "json_stdin";
const LOCAL_PROCESS_PROTOCOL_WHISPER_CPP_CLI: &str = "whisper_cpp_cli";
const LOCAL_PROCESS_PROTOCOL_LLAMA_MTMD_CLI: &str = "llama_mtmd_cli";
const DEFAULT_LLAMA_MTMD_ASR_PROMPT: &str = "Transcribe the audio.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptionRuntimeError {
    NoDefaultProvider,
    UnknownProvider(String),
    MissingBaseUrl(String),
    MissingCommand(String),
    MissingModel(String),
    MissingApiKey {
        provider_id: String,
        env: String,
    },
    HttpRequest {
        provider_id: String,
        url: String,
        message: String,
    },
    HttpStatus {
        provider_id: String,
        status: u16,
        body: String,
    },
    MalformedResponse {
        provider_id: String,
        message: String,
    },
    ProcessStart {
        provider_id: String,
        message: String,
    },
    ProcessIo {
        provider_id: String,
        message: String,
    },
    ProcessStatus {
        provider_id: String,
        code: Option<i32>,
        stderr: String,
    },
    ProcessTimeout(String),
    ProviderUnavailable {
        provider_id: String,
        reason: String,
    },
}

impl std::fmt::Display for TranscriptionRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDefaultProvider => write!(f, "no transcription.default_provider is configured"),
            Self::UnknownProvider(id) => write!(f, "unknown transcription provider `{id}`"),
            Self::MissingBaseUrl(id) => {
                write!(f, "transcription provider `{id}` is missing base_url")
            }
            Self::MissingCommand(id) => {
                write!(f, "transcription provider `{id}` is missing command")
            }
            Self::MissingModel(id) => write!(f, "transcription provider `{id}` is missing model"),
            Self::MissingApiKey { provider_id, env } => write!(
                f,
                "transcription provider `{provider_id}` requires API key env `{env}`"
            ),
            Self::HttpRequest {
                provider_id,
                url,
                message,
            } => write!(
                f,
                "transcription provider `{provider_id}` request to {url} failed: {message}"
            ),
            Self::HttpStatus {
                provider_id,
                status,
                body,
            } => write!(
                f,
                "transcription provider `{provider_id}` returned HTTP {status}: {body}"
            ),
            Self::MalformedResponse {
                provider_id,
                message,
            } => write!(
                f,
                "transcription provider `{provider_id}` returned a malformed response: {message}"
            ),
            Self::ProcessStart {
                provider_id,
                message,
            } => write!(
                f,
                "failed to start transcription provider `{provider_id}`: {message}"
            ),
            Self::ProcessIo {
                provider_id,
                message,
            } => write!(
                f,
                "transcription provider `{provider_id}` process IO failed: {message}"
            ),
            Self::ProcessStatus {
                provider_id,
                code,
                stderr,
            } => write!(
                f,
                "transcription provider `{provider_id}` exited with code {code:?}: {stderr}"
            ),
            Self::ProcessTimeout(provider_id) => {
                write!(f, "transcription provider `{provider_id}` timed out")
            }
            Self::ProviderUnavailable {
                provider_id,
                reason,
            } => {
                write!(
                    f,
                    "transcription provider `{provider_id}` is unavailable: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for TranscriptionRuntimeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionAudio {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionRequest {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub audio: TranscriptionAudio,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionResponse {
    pub text: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub submit_mode: TranscriptionSubmitMode,
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptionRuntime {
    config: TranscriptionConfig,
    client: Client,
}

impl TranscriptionRuntime {
    pub fn new(config: TranscriptionConfig) -> Self {
        Self {
            config,
            client: Client::builder()
                .timeout(DEFAULT_HTTP_TIMEOUT)
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub fn submit_mode(&self) -> TranscriptionSubmitMode {
        self.config.submit_mode
    }

    pub fn provider(
        &self,
        provider_id: Option<&str>,
    ) -> Result<(String, &TranscriptionProviderConfig), TranscriptionRuntimeError> {
        let id = provider_id
            .map(str::to_owned)
            .or_else(|| self.config.default_provider.clone())
            .ok_or(TranscriptionRuntimeError::NoDefaultProvider)?;
        let provider = self
            .config
            .providers
            .get(&id)
            .ok_or_else(|| TranscriptionRuntimeError::UnknownProvider(id.clone()))?;
        Ok((id, provider))
    }

    pub async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResponse, TranscriptionRuntimeError> {
        let (provider_id, provider) = self.provider(request.provider_id.as_deref())?;
        let model = request
            .model
            .clone()
            .or_else(|| provider.model.clone())
            .or_else(|| self.config.default_model.clone());

        let text = match provider.kind {
            TranscriptionProviderKind::OpenAi => {
                self.transcribe_openai(&provider_id, provider, model.as_deref(), &request)
                    .await?
            }
            TranscriptionProviderKind::DashScopeQwen => {
                self.transcribe_openai_chat_audio(
                    &provider_id,
                    provider,
                    model.as_deref().unwrap_or(DEFAULT_QWEN_ASR_MODEL),
                    provider
                        .base_url
                        .as_deref()
                        .unwrap_or(DEFAULT_DASHSCOPE_BASE_URL),
                    provider
                        .api_key_env
                        .as_deref()
                        .unwrap_or(DEFAULT_DASHSCOPE_API_KEY_ENV),
                    &request,
                    true,
                )
                .await?
            }
            TranscriptionProviderKind::LocalHttp => {
                self.transcribe_local_http(&provider_id, provider, model.as_deref(), &request)
                    .await?
            }
            TranscriptionProviderKind::LocalProcess => {
                self.transcribe_local_process(&provider_id, provider, model.as_deref(), &request)
                    .await?
            }
            TranscriptionProviderKind::NativeEngine => {
                return Err(TranscriptionRuntimeError::ProviderUnavailable {
                    provider_id,
                    reason: provider_unavailable_reason(provider.kind, model.as_deref()),
                });
            }
        };

        Ok(TranscriptionResponse {
            text,
            provider_id,
            model,
            submit_mode: self.submit_mode(),
        })
    }

    async fn transcribe_openai(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let model = model
            .ok_or_else(|| TranscriptionRuntimeError::MissingModel(provider_id.to_string()))?;
        let base_url = provider
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_OPENAI_BASE_URL);
        let api_key_env = provider
            .api_key_env
            .as_deref()
            .unwrap_or(DEFAULT_OPENAI_API_KEY_ENV);
        let api_key = resolve_api_key(provider_id, api_key_env)?;
        let url = join_url(base_url, OPENAI_AUDIO_TRANSCRIPTIONS_PATH);
        let filename = audio_filename(&request.audio.media_type);
        let part = multipart::Part::bytes(request.audio.bytes.clone())
            .file_name(filename)
            .mime_str(&request.audio.media_type)
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let mut form = multipart::Form::new()
            .text("model", model.to_string())
            .part("file", part);
        if let Some(language) = request.language.as_deref().and_then(non_empty) {
            form = form.text("language", language.to_string());
        }

        let mut http_request = self.client.post(&url).multipart(form);
        if let Some(api_key) = api_key {
            http_request = http_request.bearer_auth(api_key);
        }
        let response =
            http_request
                .send()
                .await
                .map_err(|err| TranscriptionRuntimeError::HttpRequest {
                    provider_id: provider_id.to_string(),
                    url: url.clone(),
                    message: err.to_string(),
                })?;
        parse_http_text_response(provider_id, &url, response, None).await
    }

    async fn transcribe_openai_chat_audio(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: &str,
        base_url: &str,
        api_key_env: &str,
        request: &TranscriptionRequest,
        dashscope_defaults: bool,
    ) -> Result<String, TranscriptionRuntimeError> {
        let api_key = resolve_api_key(provider_id, api_key_env)?;
        let url = join_url(base_url, OPENAI_CHAT_COMPLETIONS_PATH);
        let mut body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "input_audio",
                    "input_audio": {
                        "data": audio_data_uri(&request.audio)
                    }
                }]
            }],
            "stream": false
        });
        let mut asr_options = metadata_object(&provider.metadata, "asr_options")
            .cloned()
            .unwrap_or_default();
        if let Some(language) = request.language.as_deref().and_then(non_empty) {
            asr_options.insert("language".to_string(), json!(language));
        }
        if dashscope_defaults && !asr_options.contains_key("enable_itn") {
            asr_options.insert("enable_itn".to_string(), json!(false));
        }
        if !asr_options.is_empty() {
            body["asr_options"] = Value::Object(asr_options);
        }
        merge_extra_body(&mut body, metadata_object(&provider.metadata, "extra_body"));

        let mut http_request = self.client.post(&url).json(&body);
        if let Some(api_key) = api_key {
            http_request = http_request.bearer_auth(api_key);
        }
        let response =
            http_request
                .send()
                .await
                .map_err(|err| TranscriptionRuntimeError::HttpRequest {
                    provider_id: provider_id.to_string(),
                    url: url.clone(),
                    message: err.to_string(),
                })?;
        parse_http_text_response(
            provider_id,
            &url,
            response,
            Some("choices.0.message.content"),
        )
        .await
    }

    async fn transcribe_local_http(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let base_url = provider
            .base_url
            .as_deref()
            .ok_or_else(|| TranscriptionRuntimeError::MissingBaseUrl(provider_id.to_string()))?;
        let protocol = metadata_str(&provider.metadata, "protocol").unwrap_or("openai_chat_audio");
        match protocol {
            "plain_json" => {
                let url = join_url(
                    base_url,
                    metadata_str(&provider.metadata, "path").unwrap_or(""),
                );
                let body = plain_json_request(
                    model.unwrap_or(DEFAULT_LOCAL_ASR_MODEL),
                    request,
                    metadata_object(&provider.metadata, "extra_body"),
                );
                let response = self
                    .client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|err| TranscriptionRuntimeError::HttpRequest {
                        provider_id: provider_id.to_string(),
                        url: url.clone(),
                        message: err.to_string(),
                    })?;
                parse_http_text_response(
                    provider_id,
                    &url,
                    response,
                    metadata_str(&provider.metadata, "response_text_path"),
                )
                .await
            }
            _ => {
                self.transcribe_openai_chat_audio(
                    provider_id,
                    provider,
                    model.unwrap_or(DEFAULT_LOCAL_ASR_MODEL),
                    base_url,
                    provider.api_key_env.as_deref().unwrap_or(""),
                    request,
                    false,
                )
                .await
            }
        }
    }

    async fn transcribe_local_process(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let protocol = metadata_str(&provider.metadata, "protocol")
            .unwrap_or(LOCAL_PROCESS_PROTOCOL_JSON_STDIN);
        match protocol {
            LOCAL_PROCESS_PROTOCOL_JSON_STDIN => {
                self.transcribe_json_stdin_process(provider_id, provider, model, request)
                    .await
            }
            LOCAL_PROCESS_PROTOCOL_WHISPER_CPP_CLI => {
                self.transcribe_whisper_cpp_cli(provider_id, provider, model, request)
                    .await
            }
            LOCAL_PROCESS_PROTOCOL_LLAMA_MTMD_CLI => {
                self.transcribe_llama_mtmd_cli(provider_id, provider, model, request)
                    .await
            }
            other => Err(TranscriptionRuntimeError::ProviderUnavailable {
                provider_id: provider_id.to_string(),
                reason: format!("unsupported local_process protocol `{other}`"),
            }),
        }
    }

    async fn transcribe_json_stdin_process(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let command = provider
            .command
            .as_deref()
            .ok_or_else(|| TranscriptionRuntimeError::MissingCommand(provider_id.to_string()))?;
        let timeout_ms = provider
            .timeout_ms
            .unwrap_or(DEFAULT_PROCESS_TIMEOUT.as_millis() as u64);
        let body = plain_json_request(
            model.unwrap_or(DEFAULT_LOCAL_ASR_MODEL),
            request,
            metadata_object(&provider.metadata, "extra_body"),
        );
        let mut child = Command::new(command)
            .args(&provider.args)
            .envs(&provider.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| TranscriptionRuntimeError::ProcessStart {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: "child stdin is unavailable".to_string(),
            })?;
        let input =
            serde_json::to_vec(&body).map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        stdin
            .write_all(&input)
            .await
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        drop(stdin);

        let output = timeout(Duration::from_millis(timeout_ms), child.wait_with_output())
            .await
            .map_err(|_| TranscriptionRuntimeError::ProcessTimeout(provider_id.to_string()))?
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        if !output.status.success() {
            return Err(TranscriptionRuntimeError::ProcessStatus {
                provider_id: provider_id.to_string(),
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        parse_process_stdout(
            provider_id,
            &output.stdout,
            metadata_str(&provider.metadata, "response_text_path"),
        )
    }

    async fn transcribe_whisper_cpp_cli(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let command = provider
            .command
            .as_deref()
            .ok_or_else(|| TranscriptionRuntimeError::MissingCommand(provider_id.to_string()))?;
        let model_path = metadata_str(&provider.metadata, "model_path")
            .or(model)
            .ok_or_else(|| TranscriptionRuntimeError::MissingModel(provider_id.to_string()))?;
        let timeout_ms = provider
            .timeout_ms
            .unwrap_or(DEFAULT_PROCESS_TIMEOUT.as_millis() as u64);
        let temp_dir = tempfile::Builder::new()
            .prefix("praxis-asr-whisper-")
            .tempdir()
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let audio_path = temp_dir
            .path()
            .join(audio_filename(&request.audio.media_type));
        fs::write(&audio_path, &request.audio.bytes)
            .await
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let output_stem = temp_dir.path().join("transcript");
        let language = request
            .language
            .as_deref()
            .and_then(non_empty)
            .or_else(|| metadata_str(&provider.metadata, "language"))
            .unwrap_or("auto");
        let args = whisper_cpp_args(provider, model_path, &audio_path, &output_stem, language);
        let output = timeout(
            Duration::from_millis(timeout_ms),
            Command::new(command)
                .args(&args)
                .envs(&provider.env)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| TranscriptionRuntimeError::ProcessTimeout(provider_id.to_string()))?
        .map_err(|err| TranscriptionRuntimeError::ProcessStart {
            provider_id: provider_id.to_string(),
            message: err.to_string(),
        })?;
        if !output.status.success() {
            return Err(TranscriptionRuntimeError::ProcessStatus {
                provider_id: provider_id.to_string(),
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let transcript_path = output_stem.with_extension("txt");
        if transcript_path.exists() {
            let text = fs::read_to_string(&transcript_path).await.map_err(|err| {
                TranscriptionRuntimeError::ProcessIo {
                    provider_id: provider_id.to_string(),
                    message: err.to_string(),
                }
            })?;
            if !text.trim().is_empty() {
                return Ok(text.trim().to_string());
            }
        }
        parse_process_stdout(
            provider_id,
            &output.stdout,
            metadata_str(&provider.metadata, "response_text_path"),
        )
    }

    async fn transcribe_llama_mtmd_cli(
        &self,
        provider_id: &str,
        provider: &TranscriptionProviderConfig,
        model: Option<&str>,
        request: &TranscriptionRequest,
    ) -> Result<String, TranscriptionRuntimeError> {
        let command = provider
            .command
            .as_deref()
            .ok_or_else(|| TranscriptionRuntimeError::MissingCommand(provider_id.to_string()))?;
        let model_path = metadata_str(&provider.metadata, "model_path")
            .or(model)
            .ok_or_else(|| TranscriptionRuntimeError::MissingModel(provider_id.to_string()))?;
        let mmproj_path = metadata_str(&provider.metadata, "mmproj_path").ok_or_else(|| {
            TranscriptionRuntimeError::ProviderUnavailable {
                provider_id: provider_id.to_string(),
                reason: "llama_mtmd_cli requires metadata.mmproj_path".to_string(),
            }
        })?;
        let timeout_ms = provider
            .timeout_ms
            .unwrap_or(DEFAULT_PROCESS_TIMEOUT.as_millis() as u64);
        let temp_dir = tempfile::Builder::new()
            .prefix("praxis-asr-mtmd-")
            .tempdir()
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let audio_path = temp_dir
            .path()
            .join(audio_filename(&request.audio.media_type));
        fs::write(&audio_path, &request.audio.bytes)
            .await
            .map_err(|err| TranscriptionRuntimeError::ProcessIo {
                provider_id: provider_id.to_string(),
                message: err.to_string(),
            })?;
        let prompt =
            metadata_str(&provider.metadata, "prompt").unwrap_or(DEFAULT_LLAMA_MTMD_ASR_PROMPT);
        let args = llama_mtmd_args(provider, model_path, mmproj_path, &audio_path, prompt);
        let output = timeout(
            Duration::from_millis(timeout_ms),
            Command::new(command)
                .args(&args)
                .envs(&provider.env)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| TranscriptionRuntimeError::ProcessTimeout(provider_id.to_string()))?
        .map_err(|err| TranscriptionRuntimeError::ProcessStart {
            provider_id: provider_id.to_string(),
            message: err.to_string(),
        })?;
        if !output.status.success() {
            return Err(TranscriptionRuntimeError::ProcessStatus {
                provider_id: provider_id.to_string(),
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        parse_llama_mtmd_stdout(
            provider_id,
            &output.stdout,
            metadata_str(&provider.metadata, "response_text_path"),
        )
    }
}

fn provider_unavailable_reason(kind: TranscriptionProviderKind, model: Option<&str>) -> String {
    let model = model.unwrap_or("<default>");
    match kind {
        TranscriptionProviderKind::OpenAi => {
            format!("OpenAI transcription adapter is not wired yet for model `{model}`")
        }
        TranscriptionProviderKind::DashScopeQwen => {
            format!("DashScope/Qwen ASR adapter is not wired yet for model `{model}`")
        }
        TranscriptionProviderKind::LocalHttp => {
            format!("local HTTP ASR adapter is not wired yet for model `{model}`")
        }
        TranscriptionProviderKind::LocalProcess => {
            format!("local process ASR adapter is not wired yet for model `{model}`")
        }
        TranscriptionProviderKind::NativeEngine => {
            format!("native ASR engine adapter is not wired yet for model `{model}`")
        }
    }
}

async fn parse_http_text_response(
    provider_id: &str,
    url: &str,
    response: reqwest::Response,
    preferred_path: Option<&str>,
) -> Result<String, TranscriptionRuntimeError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| TranscriptionRuntimeError::HttpRequest {
            provider_id: provider_id.to_string(),
            url: url.to_string(),
            message: err.to_string(),
        })?;
    if !status.is_success() {
        return Err(TranscriptionRuntimeError::HttpStatus {
            provider_id: provider_id.to_string(),
            status: status.as_u16(),
            body,
        });
    }
    parse_text_response(provider_id, &body, preferred_path)
}

fn parse_process_stdout(
    provider_id: &str,
    stdout: &[u8],
    preferred_path: Option<&str>,
) -> Result<String, TranscriptionRuntimeError> {
    let body = String::from_utf8_lossy(stdout).trim().to_string();
    parse_text_response(provider_id, &body, preferred_path)
}

fn parse_llama_mtmd_stdout(
    provider_id: &str,
    stdout: &[u8],
    preferred_path: Option<&str>,
) -> Result<String, TranscriptionRuntimeError> {
    let body = String::from_utf8_lossy(stdout).trim().to_string();
    if let Some(text) = extract_llama_mtmd_asr_text(&body) {
        return Ok(text);
    }
    parse_text_response(provider_id, &body, preferred_path)
}

fn extract_llama_mtmd_asr_text(body: &str) -> Option<String> {
    let (_, after_marker) = body.split_once("<asr_text>")?;
    let text = after_marker
        .split(['<', '\r', '\n'])
        .next()
        .unwrap_or(after_marker)
        .trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn parse_text_response(
    provider_id: &str,
    body: &str,
    preferred_path: Option<&str>,
) -> Result<String, TranscriptionRuntimeError> {
    if body.trim().is_empty() {
        return Err(TranscriptionRuntimeError::MalformedResponse {
            provider_id: provider_id.to_string(),
            message: "empty response".to_string(),
        });
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Ok(body.trim().to_string());
    };
    let candidate_paths = preferred_path
        .into_iter()
        .chain([
            "text",
            "transcript",
            "output.text",
            "choices.0.message.content",
            "choices.0.delta.content",
        ])
        .collect::<Vec<_>>();
    for path in candidate_paths {
        if let Some(text) = value_at_path(&value, path).and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            return Ok(text.to_string());
        }
    }
    Err(TranscriptionRuntimeError::MalformedResponse {
        provider_id: provider_id.to_string(),
        message: "could not find transcription text".to_string(),
    })
}

fn plain_json_request(
    model: &str,
    request: &TranscriptionRequest,
    extra_body: Option<&Map<String, Value>>,
) -> Value {
    let mut body = json!({
        "model": model,
        "audio": {
            "media_type": request.audio.media_type,
            "data": BASE64.encode(&request.audio.bytes),
        },
    });
    if let Some(language) = request.language.as_deref().and_then(non_empty) {
        body["language"] = json!(language);
    }
    merge_extra_body(&mut body, extra_body);
    body
}

fn audio_data_uri(audio: &TranscriptionAudio) -> String {
    format!(
        "data:{};base64,{}",
        audio.media_type,
        BASE64.encode(&audio.bytes)
    )
}

fn audio_filename(media_type: &str) -> String {
    let extension = match media_type {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/mp4" | "audio/m4a" => "m4a",
        _ => "audio",
    };
    format!("input.{extension}")
}

fn whisper_cpp_args(
    provider: &TranscriptionProviderConfig,
    model_path: &str,
    audio_path: &Path,
    output_stem: &Path,
    language: &str,
) -> Vec<String> {
    if !provider.args.is_empty() {
        return provider
            .args
            .iter()
            .map(|arg| expand_process_arg(arg, model_path, audio_path, output_stem, language))
            .collect();
    }
    let mut args = vec![
        "-m".to_string(),
        model_path.to_string(),
        "-f".to_string(),
        process_path(audio_path),
        "-l".to_string(),
        language.to_string(),
        "-otxt".to_string(),
        "-of".to_string(),
        process_path(output_stem),
        "-np".to_string(),
        "-nt".to_string(),
    ];
    if let Some(extra_args) = metadata_string_array(&provider.metadata, "extra_args") {
        args.extend(extra_args);
    }
    args
}

fn llama_mtmd_args(
    provider: &TranscriptionProviderConfig,
    model_path: &str,
    mmproj_path: &str,
    audio_path: &Path,
    prompt: &str,
) -> Vec<String> {
    if !provider.args.is_empty() {
        return provider
            .args
            .iter()
            .map(|arg| expand_mtmd_arg(arg, model_path, mmproj_path, audio_path, prompt))
            .collect();
    }
    let mut args = vec![
        "-m".to_string(),
        model_path.to_string(),
        "--mmproj".to_string(),
        mmproj_path.to_string(),
        "--audio".to_string(),
        process_path(audio_path),
        "-p".to_string(),
        prompt.to_string(),
        "-n".to_string(),
        "192".to_string(),
        "--temp".to_string(),
        "0".to_string(),
        "--no-warmup".to_string(),
    ];
    if let Some(extra_args) = metadata_string_array(&provider.metadata, "extra_args") {
        args.extend(extra_args);
    }
    args
}

fn expand_mtmd_arg(
    arg: &str,
    model_path: &str,
    mmproj_path: &str,
    audio_path: &Path,
    prompt: &str,
) -> String {
    arg.replace("{model}", model_path)
        .replace("{mmproj}", mmproj_path)
        .replace("{audio}", &process_path(audio_path))
        .replace("{prompt}", prompt)
}

fn expand_process_arg(
    arg: &str,
    model_path: &str,
    audio_path: &Path,
    output_stem: &Path,
    language: &str,
) -> String {
    arg.replace("{model}", model_path)
        .replace("{audio}", &process_path(audio_path))
        .replace("{output}", &process_path(output_stem))
        .replace("{language}", language)
}

fn process_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn resolve_api_key(
    provider_id: &str,
    api_key_env: &str,
) -> Result<Option<String>, TranscriptionRuntimeError> {
    let Some(env_name) = non_empty(api_key_env) else {
        return Ok(None);
    };
    let api_key = std::env::var(env_name)
        .ok()
        .and_then(|value| non_empty(&value).map(str::to_string))
        .ok_or_else(|| TranscriptionRuntimeError::MissingApiKey {
            provider_id: provider_id.to_string(),
            env: env_name.to_string(),
        })?;
    Ok(Some(api_key))
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

fn metadata_str<'a>(metadata: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .and_then(non_empty)
}

fn metadata_object<'a>(
    metadata: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<&'a Map<String, Value>> {
    metadata.get(key).and_then(Value::as_object)
}

fn metadata_string_array(metadata: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    let values = metadata.get(key)?.as_array()?;
    let strings = values
        .iter()
        .filter_map(Value::as_str)
        .filter_map(non_empty)
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!strings.is_empty()).then_some(strings)
}

fn merge_extra_body(body: &mut Value, extra_body: Option<&Map<String, Value>>) {
    let Some(extra_body) = extra_body else {
        return;
    };
    let Some(body_object) = body.as_object_mut() else {
        return;
    };
    for (key, value) in extra_body {
        body_object.insert(key.clone(), value.clone());
    }
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cursor = value;
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        if let Ok(index) = part.parse::<usize>() {
            cursor = cursor.get(index)?;
        } else {
            cursor = cursor.get(part)?;
        }
    }
    Some(cursor)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio() -> TranscriptionAudio {
        TranscriptionAudio {
            media_type: "audio/wav".to_string(),
            bytes: b"RIFF".to_vec(),
        }
    }

    #[test]
    fn builds_audio_data_uri() {
        assert_eq!(audio_data_uri(&audio()), "data:audio/wav;base64,UklGRg==");
    }

    #[test]
    fn parses_openai_chat_audio_response() {
        let body = r#"{"choices":[{"message":{"content":"hello world"}}]}"#;

        let text = parse_text_response("qwen", body, Some("choices.0.message.content")).unwrap();

        assert_eq!(text, "hello world");
    }

    #[test]
    fn parses_plain_text_process_response() {
        let text = parse_process_stdout("local", b"hello from local\n", None).unwrap();

        assert_eq!(text, "hello from local");
    }

    #[test]
    fn parses_llama_mtmd_qwen_asr_marker() {
        let body = b"\nlanguage English<asr_text>Hello, Praxis Voice Input.\n";

        let text = parse_llama_mtmd_stdout("qwen3-asr", body, None).unwrap();

        assert_eq!(text, "Hello, Praxis Voice Input.");
    }

    #[test]
    fn builds_plain_json_request_with_language_and_extra_body() {
        let request = TranscriptionRequest {
            provider_id: None,
            model: None,
            audio: audio(),
            language: Some("zh".to_string()),
        };
        let extra = serde_json::from_value::<Map<String, Value>>(json!({
            "hotwords": ["Praxis", "Cunning3D"]
        }))
        .unwrap();

        let body = plain_json_request("qwen3-asr-flash", &request, Some(&extra));

        assert_eq!(body["model"], "qwen3-asr-flash");
        assert_eq!(body["language"], "zh");
        assert_eq!(body["audio"]["data"], "UklGRg==");
        assert_eq!(body["hotwords"][0], "Praxis");
    }

    #[test]
    fn builds_default_whisper_cpp_args() {
        let provider = TranscriptionProviderConfig {
            kind: TranscriptionProviderKind::LocalProcess,
            model: None,
            base_url: None,
            api_key_env: None,
            command: Some("whisper-cli".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout_ms: None,
            metadata: BTreeMap::new(),
        };

        let args = whisper_cpp_args(
            &provider,
            "F:/models/model.gguf",
            Path::new("F:/tmp/input.wav"),
            Path::new("F:/tmp/transcript"),
            "zh",
        );

        assert_eq!(args[0], "-m");
        assert!(args.contains(&"F:/models/model.gguf".to_string()));
        assert!(args.contains(&"F:/tmp/input.wav".to_string()));
        assert!(args.contains(&"F:/tmp/transcript".to_string()));
        assert!(args.contains(&"zh".to_string()));
        assert!(args.contains(&"-otxt".to_string()));
    }

    #[test]
    fn expands_custom_whisper_cpp_arg_placeholders() {
        let provider = TranscriptionProviderConfig {
            kind: TranscriptionProviderKind::LocalProcess,
            model: None,
            base_url: None,
            api_key_env: None,
            command: Some("whisper-cli".to_string()),
            args: vec![
                "--model".to_string(),
                "{model}".to_string(),
                "--file={audio}".to_string(),
                "--output={output}".to_string(),
                "--language={language}".to_string(),
            ],
            env: BTreeMap::new(),
            timeout_ms: None,
            metadata: BTreeMap::new(),
        };

        let args = whisper_cpp_args(
            &provider,
            "F:/models/model.gguf",
            Path::new("F:/tmp/input.wav"),
            Path::new("F:/tmp/transcript"),
            "auto",
        );

        assert_eq!(
            args,
            vec![
                "--model",
                "F:/models/model.gguf",
                "--file=F:/tmp/input.wav",
                "--output=F:/tmp/transcript",
                "--language=auto",
            ]
        );
    }

    #[test]
    fn builds_default_llama_mtmd_args() {
        let provider = TranscriptionProviderConfig {
            kind: TranscriptionProviderKind::LocalProcess,
            model: None,
            base_url: None,
            api_key_env: None,
            command: Some("llama-mtmd-cli".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout_ms: None,
            metadata: BTreeMap::new(),
        };

        let args = llama_mtmd_args(
            &provider,
            "F:/models/qwen.gguf",
            "F:/models/mmproj.gguf",
            Path::new("F:/tmp/input.wav"),
            "Transcribe the audio.",
        );

        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"F:/models/qwen.gguf".to_string()));
        assert!(args.contains(&"--mmproj".to_string()));
        assert!(args.contains(&"F:/models/mmproj.gguf".to_string()));
        assert!(args.contains(&"--audio".to_string()));
        assert!(args.contains(&"F:/tmp/input.wav".to_string()));
        assert!(args.contains(&"Transcribe the audio.".to_string()));
    }

    #[test]
    fn expands_custom_llama_mtmd_arg_placeholders() {
        let provider = TranscriptionProviderConfig {
            kind: TranscriptionProviderKind::LocalProcess,
            model: None,
            base_url: None,
            api_key_env: None,
            command: Some("llama-mtmd-cli".to_string()),
            args: vec![
                "-m".to_string(),
                "{model}".to_string(),
                "--mmproj={mmproj}".to_string(),
                "--audio={audio}".to_string(),
                "-p".to_string(),
                "{prompt}".to_string(),
            ],
            env: BTreeMap::new(),
            timeout_ms: None,
            metadata: BTreeMap::new(),
        };

        let args = llama_mtmd_args(
            &provider,
            "F:/models/qwen.gguf",
            "F:/models/mmproj.gguf",
            Path::new("F:/tmp/input.wav"),
            "Transcribe.",
        );

        assert_eq!(
            args,
            vec![
                "-m",
                "F:/models/qwen.gguf",
                "--mmproj=F:/models/mmproj.gguf",
                "--audio=F:/tmp/input.wav",
                "-p",
                "Transcribe.",
            ]
        );
    }
}
