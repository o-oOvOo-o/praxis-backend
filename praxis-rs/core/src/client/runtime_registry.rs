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

#[derive(Clone, PartialEq, Eq, Hash)]
struct ModelRuntimeKey {
    provider_id: String,
    configuration_digest: [u8; 32],
}

impl ModelRuntimeKey {
    fn from_provider(provider_id: &str, provider: &ModelProviderInfo) -> Self {
        use sha2::Digest;
        use sha2::Sha256;

        let mut hasher = Sha256::new();
        if let Err(err) = serde_json::to_writer(&mut hasher, provider) {
            hasher.update(b"praxis-provider-serialization-error\0");
            hasher.update(err.to_string().as_bytes());
        }
        Self {
            provider_id: provider_id.to_string(),
            configuration_digest: hasher.finalize().into(),
        }
    }
}

impl std::fmt::Debug for ModelRuntimeKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelRuntimeKey")
            .field("provider_id", &self.provider_id)
            .field("configuration_digest", &"[SHA-256]")
            .finish()
    }
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
            provider_id.to_string(),
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
