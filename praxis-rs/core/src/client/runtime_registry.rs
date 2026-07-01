use super::*;

pub(crate) struct ModelRuntimeRegistry {
    state: Arc<ModelRuntimeRegistryState>,
}

#[derive(Debug)]
struct ModelRuntimeRegistryState {
    auth_manager: Option<Arc<AuthManager>>,
    conversation_id: ThreadId,
    session_source: SessionSource,
    model_verbosity: Option<VerbosityConfig>,
    enable_request_compression: bool,
    include_timing_metrics: bool,
    beta_features_header: Option<String>,
    native_local_config: NativeLocalModelConfig,
    clients: StdMutex<HashMap<ModelRuntimeKey, ModelClient>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ModelRuntimeKey {
    provider_id: String,
    name: String,
    base_url: Option<String>,
    env_key: Option<String>,
    experimental_bearer_token: Option<String>,
    auth: Option<String>,
    wire_api: String,
    compat: Option<String>,
    query_params: Vec<(String, String)>,
    http_headers: Vec<(String, String)>,
    env_http_headers: Vec<(String, String)>,
    request_max_retries: Option<u64>,
    stream_max_retries: Option<u64>,
    stream_idle_timeout_ms: Option<u64>,
    websocket_connect_timeout_ms: Option<u64>,
    requires_openai_auth: bool,
    supports_websockets: bool,
}

impl ModelRuntimeKey {
    fn from_provider(provider_id: &str, provider: &ModelProviderInfo) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            env_key: provider.env_key.clone(),
            experimental_bearer_token: provider.experimental_bearer_token.clone(),
            auth: provider
                .auth
                .as_ref()
                .map(|auth| serde_json::to_string(auth).unwrap_or_else(|_| format!("{auth:?}"))),
            wire_api: provider.wire_api.to_string(),
            compat: provider.compat.as_ref().map(|compat| {
                serde_json::to_string(compat).unwrap_or_else(|_| format!("{compat:?}"))
            }),
            query_params: sorted_string_map(provider.query_params.as_ref()),
            http_headers: sorted_string_map(provider.http_headers.as_ref()),
            env_http_headers: sorted_string_map(provider.env_http_headers.as_ref()),
            request_max_retries: provider.request_max_retries,
            stream_max_retries: provider.stream_max_retries,
            stream_idle_timeout_ms: provider.stream_idle_timeout_ms,
            websocket_connect_timeout_ms: provider.websocket_connect_timeout_ms,
            requires_openai_auth: provider.requires_openai_auth,
            supports_websockets: provider.supports_websockets,
        }
    }
}

fn sorted_string_map(map: Option<&HashMap<String, String>>) -> Vec<(String, String)> {
    let mut entries = map.map_or_else(Vec::new, |map| {
        map.iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>()
    });
    entries.sort();
    entries
}

impl ModelRuntimeRegistry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        auth_manager: Option<Arc<AuthManager>>,
        conversation_id: ThreadId,
        session_source: SessionSource,
        model_verbosity: Option<VerbosityConfig>,
        enable_request_compression: bool,
        include_timing_metrics: bool,
        beta_features_header: Option<String>,
        native_local_config: NativeLocalModelConfig,
    ) -> Self {
        Self {
            state: Arc::new(ModelRuntimeRegistryState {
                auth_manager,
                conversation_id,
                session_source,
                model_verbosity,
                enable_request_compression,
                include_timing_metrics,
                beta_features_header,
                native_local_config,
                clients: StdMutex::new(HashMap::new()),
            }),
        }
    }

    pub(crate) fn client_for(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> ModelClient {
        let key = ModelRuntimeKey::from_provider(provider_id, provider);
        let mut clients = self
            .state
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(client) = clients.get(&key) {
            return client.clone();
        }

        let client = ModelClient::new_with_native_local_config(
            self.state.auth_manager.clone(),
            self.state.conversation_id.clone(),
            provider.clone(),
            self.state.session_source.clone(),
            self.state.model_verbosity,
            self.state.enable_request_compression,
            self.state.include_timing_metrics,
            self.state.beta_features_header.clone(),
            self.state.native_local_config.clone(),
        );
        clients.insert(key, client.clone());
        client
    }

    pub(crate) fn new_session_for(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> ModelClientSession {
        self.client_for(provider_id, provider).new_session()
    }

    pub(crate) fn responses_websocket_enabled_for(
        &self,
        provider_id: &str,
        provider: &ModelProviderInfo,
    ) -> bool {
        self.client_for(provider_id, provider)
            .responses_websocket_enabled()
    }
}
