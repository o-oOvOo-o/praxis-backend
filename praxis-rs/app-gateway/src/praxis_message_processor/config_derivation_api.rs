use super::*;

impl PraxisMessageProcessor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_thread_config_overrides(
        &self,
        model: Option<String>,
        model_provider: Option<String>,
        service_tier: Option<Option<praxis_protocol::config_types::ServiceTier>>,
        cwd: Option<String>,
        approval_policy: Option<praxis_app_gateway_protocol::AskForApproval>,
        approvals_reviewer: Option<praxis_app_gateway_protocol::ApprovalsReviewer>,
        sandbox: Option<SandboxMode>,
        base_instructions: Option<String>,
        developer_instructions: Option<String>,
        personality: Option<Personality>,
    ) -> ConfigOverrides {
        ConfigOverrides {
            model,
            model_provider,
            service_tier,
            cwd: cwd.map(PathBuf::from),
            approval_policy: approval_policy
                .map(praxis_app_gateway_protocol::AskForApproval::to_core),
            approvals_reviewer: approvals_reviewer
                .map(praxis_app_gateway_protocol::ApprovalsReviewer::to_core),
            sandbox_mode: sandbox.map(SandboxMode::to_core),
            praxis_linux_sandbox_exe: self.arg0_paths.praxis_linux_sandbox_exe.clone(),
            main_execve_wrapper_exe: self.arg0_paths.main_execve_wrapper_exe.clone(),
            base_instructions,
            developer_instructions,
            personality,
            ..Default::default()
        }
    }
}

pub(crate) fn collect_resume_override_mismatches(
    request: &ThreadResumeParams,
    config_snapshot: &ThreadConfigSnapshot,
) -> Vec<String> {
    let mut mismatch_details = Vec::new();

    if let Some(requested_model) = request.model.as_deref()
        && requested_model != config_snapshot.model
    {
        mismatch_details.push(format!(
            "model requested={requested_model} active={}",
            config_snapshot.model
        ));
    }
    if let Some(requested_provider) = request.model_provider.as_deref()
        && requested_provider != config_snapshot.model_provider_id
    {
        mismatch_details.push(format!(
            "model_provider requested={requested_provider} active={}",
            config_snapshot.model_provider_id
        ));
    }
    if let Some(requested_service_tier) = request.service_tier.as_ref()
        && requested_service_tier != &config_snapshot.service_tier
    {
        mismatch_details.push(format!(
            "service_tier requested={requested_service_tier:?} active={:?}",
            config_snapshot.service_tier
        ));
    }
    if let Some(requested_cwd) = request.cwd.as_deref() {
        let requested_cwd_path = std::path::PathBuf::from(requested_cwd);
        if requested_cwd_path != config_snapshot.cwd {
            mismatch_details.push(format!(
                "cwd requested={} active={}",
                requested_cwd_path.display(),
                config_snapshot.cwd.display()
            ));
        }
    }
    if let Some(requested_approval) = request.approval_policy.as_ref() {
        let active_approval: AskForApproval = config_snapshot.approval_policy.into();
        if requested_approval != &active_approval {
            mismatch_details.push(format!(
                "approval_policy requested={requested_approval:?} active={active_approval:?}"
            ));
        }
    }
    if let Some(requested_review_policy) = request.approvals_reviewer.as_ref() {
        let active_review_policy: praxis_app_gateway_protocol::ApprovalsReviewer =
            config_snapshot.approvals_reviewer.into();
        if requested_review_policy != &active_review_policy {
            mismatch_details.push(format!(
                "approvals_reviewer requested={requested_review_policy:?} active={active_review_policy:?}"
            ));
        }
    }
    if let Some(requested_sandbox) = request.sandbox.as_ref() {
        let sandbox_matches = matches!(
            (requested_sandbox, &config_snapshot.sandbox_policy),
            (
                SandboxMode::ReadOnly,
                praxis_protocol::protocol::SandboxPolicy::ReadOnly { .. }
            ) | (
                SandboxMode::WorkspaceWrite,
                praxis_protocol::protocol::SandboxPolicy::WorkspaceWrite { .. }
            ) | (
                SandboxMode::DangerFullAccess,
                praxis_protocol::protocol::SandboxPolicy::DangerFullAccess
            ) | (
                SandboxMode::DangerFullAccess,
                praxis_protocol::protocol::SandboxPolicy::ExternalSandbox { .. }
            )
        );
        if !sandbox_matches {
            mismatch_details.push(format!(
                "sandbox requested={requested_sandbox:?} active={:?}",
                config_snapshot.sandbox_policy
            ));
        }
    }
    if let Some(requested_personality) = request.personality.as_ref()
        && config_snapshot.personality.as_ref() != Some(requested_personality)
    {
        mismatch_details.push(format!(
            "personality requested={requested_personality:?} active={:?}",
            config_snapshot.personality
        ));
    }

    mismatch_details
}

pub(crate) fn merge_persisted_resume_metadata(
    request_overrides: &mut Option<HashMap<String, serde_json::Value>>,
    typesafe_overrides: &mut ConfigOverrides,
    persisted_metadata: &ThreadMetadata,
) {
    if has_model_resume_override(request_overrides.as_ref(), typesafe_overrides) {
        return;
    }

    typesafe_overrides.model = persisted_metadata.model.clone();

    if let Some(reasoning_effort) = &persisted_metadata.reasoning_effort {
        request_overrides.get_or_insert_with(HashMap::new).insert(
            "model_reasoning_effort".to_string(),
            serde_json::Value::String(reasoning_effort.to_string()),
        );
    }
}

fn has_model_resume_override(
    request_overrides: Option<&HashMap<String, serde_json::Value>>,
    typesafe_overrides: &ConfigOverrides,
) -> bool {
    typesafe_overrides.model.is_some()
        || typesafe_overrides.model_provider.is_some()
        || request_overrides.is_some_and(|overrides| overrides.contains_key("model"))
        || request_overrides
            .is_some_and(|overrides| overrides.contains_key("model_reasoning_effort"))
}

pub(crate) fn thread_initialized_fact(
    thread: &Thread,
    model: &str,
    initialization_mode: ThreadInitializationMode,
) -> ThreadInitializedFact {
    ThreadInitializedFact {
        thread_id: thread.id.clone(),
        model: model.to_string(),
        ephemeral: thread.ephemeral,
        thread_source: thread.source.clone().into(),
        initialization_mode,
        created_at: u64::try_from(thread.created_at).unwrap_or_default(),
    }
}

fn cloud_requirements_load_error(err: &std::io::Error) -> Option<&CloudConfigBundleLoadError> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = err
        .get_ref()
        .map(|source| source as &(dyn std::error::Error + 'static));
    while let Some(source) = current {
        if let Some(cloud_error) = source.downcast_ref::<CloudConfigBundleLoadError>() {
            return Some(cloud_error);
        }
        current = source.source();
    }
    None
}

pub(crate) fn config_load_error(err: &std::io::Error) -> JSONRPCErrorError {
    let data = cloud_requirements_load_error(err).map(|cloud_error| {
        let mut data = serde_json::json!({
            "reason": "cloudRequirements",
            "errorCode": format!("{:?}", cloud_error.code()),
            "detail": cloud_error.to_string(),
        });
        if let Some(status_code) = cloud_error.status_code() {
            data["statusCode"] = serde_json::json!(status_code);
        }
        if cloud_error.code() == CloudConfigBundleLoadErrorCode::Auth {
            data["action"] = serde_json::json!("relogin");
        }
        data
    });

    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: format!("failed to load configuration: {err}"),
        data,
    }
}

pub(crate) fn validate_dynamic_tools(tools: &[ApiDynamicToolSpec]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for tool in tools {
        let name = tool.name.trim();
        if name.is_empty() {
            return Err("dynamic tool name must not be empty".to_string());
        }
        if name != tool.name {
            return Err(format!(
                "dynamic tool name has leading/trailing whitespace: {}",
                tool.name
            ));
        }
        if name == "mcp" || name.starts_with("mcp__") {
            return Err(format!("dynamic tool name is reserved: {name}"));
        }
        if !seen.insert(name.to_string()) {
            return Err(format!("duplicate dynamic tool name: {name}"));
        }

        if let Err(err) = praxis_tools::parse_tool_input_schema(&tool.input_schema) {
            return Err(format!(
                "dynamic tool input schema is not supported for {name}: {err}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn build_core_dynamic_tools(
    tools: Option<Vec<ApiDynamicToolSpec>>,
) -> Result<Vec<CoreDynamicToolSpec>, String> {
    let tools = tools.unwrap_or_default();
    if tools.is_empty() {
        return Ok(Vec::new());
    }

    validate_dynamic_tools(&tools)?;
    Ok(tools
        .into_iter()
        .map(|tool| CoreDynamicToolSpec {
            name: tool.name,
            description: tool.description,
            input_schema: tool.input_schema,
            defer_loading: tool.defer_loading,
        })
        .collect())
}

/// Derive the effective [`Config`] by layering three override sources.
///
/// Precedence (lowest to highest):
/// - `cli_overrides`: process-wide startup `--config` flags.
/// - `request_overrides`: per-request dotted-path overrides (`params.config`), converted JSON->TOML.
/// - `typesafe_overrides`: Request objects such as `ThreadSpawnResultParams` and
///   `ThreadStartParams` support a limited set of _explicit_ config overrides, so
///   `typesafe_overrides` is a `ConfigOverrides` derived from the respective request object.
///   Because the overrides are defined explicitly in the `*Params`, this takes priority over
///   the more general "bag of config options" provided by `cli_overrides` and `request_overrides`.
pub(crate) async fn derive_config_from_params(
    cli_overrides: &[(String, TomlValue)],
    request_overrides: Option<HashMap<String, serde_json::Value>>,
    typesafe_overrides: ConfigOverrides,
    cloud_requirements: &CloudConfigBundleLoader,
    praxis_home: &Path,
    runtime_feature_enablement: &BTreeMap<String, bool>,
) -> std::io::Result<Config> {
    let merged_cli_overrides = cli_overrides
        .iter()
        .cloned()
        .chain(
            request_overrides
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, json_to_toml(v))),
        )
        .collect::<Vec<_>>();

    let mut config = praxis_core::config::ConfigBuilder::default()
        .praxis_home(praxis_home.to_path_buf())
        .cli_overrides(merged_cli_overrides)
        .harness_overrides(typesafe_overrides)
        .cloud_config_bundle(cloud_requirements.clone())
        .build()
        .await?;
    apply_runtime_feature_enablement(&mut config, runtime_feature_enablement);
    Ok(config)
}

pub(crate) async fn derive_config_for_cwd(
    cli_overrides: &[(String, TomlValue)],
    request_overrides: Option<HashMap<String, serde_json::Value>>,
    typesafe_overrides: ConfigOverrides,
    cwd: Option<PathBuf>,
    cloud_requirements: &CloudConfigBundleLoader,
    praxis_home: &Path,
    runtime_feature_enablement: &BTreeMap<String, bool>,
) -> std::io::Result<Config> {
    let merged_cli_overrides = cli_overrides
        .iter()
        .cloned()
        .chain(
            request_overrides
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, json_to_toml(v))),
        )
        .collect::<Vec<_>>();

    let mut config = praxis_core::config::ConfigBuilder::default()
        .praxis_home(praxis_home.to_path_buf())
        .cli_overrides(merged_cli_overrides)
        .harness_overrides(typesafe_overrides)
        .fallback_cwd(cwd)
        .cloud_config_bundle(cloud_requirements.clone())
        .build()
        .await?;
    apply_runtime_feature_enablement(&mut config, runtime_feature_enablement);
    Ok(config)
}
