use super::*;

pub(super) fn session_target_from_app_gateway_thread(
    thread: AppGatewayThread,
) -> Option<resume_picker::SessionTarget> {
    match ThreadId::from_string(&thread.id) {
        Ok(thread_id) => Some(resume_picker::SessionTarget {
            path: thread.path,
            thread_id,
            thread_name: thread.name,
            cwd: Some(thread.cwd),
        }),
        Err(err) => {
            warn!(
                thread_id = thread.id,
                %err,
                "Ignoring app-gateway thread with invalid thread id during TUI session lookup"
            );
            None
        }
    }
}

pub(super) async fn lookup_session_target_with_app_gateway(
    app_gateway: &mut AppGatewaySession,
    id_or_name: &str,
) -> color_eyre::Result<Option<resume_picker::SessionTarget>> {
    let params = session_lookup_params(
        ThreadLookupSelector::IdOrName {
            value: id_or_name.to_string(),
        },
        interactive_thread_source_kinds(/*include_non_interactive*/ false),
        None,
    );
    app_gateway
        .thread_lookup(params)
        .await
        .map(|thread| thread.and_then(session_target_from_app_gateway_thread))
}

pub(super) async fn lookup_latest_session_target_with_app_gateway(
    app_gateway: &mut AppGatewaySession,
    cwd_filter: Option<&Path>,
    include_non_interactive: bool,
) -> color_eyre::Result<Option<resume_picker::SessionTarget>> {
    let params = session_lookup_params(
        ThreadLookupSelector::Latest,
        interactive_thread_source_kinds(include_non_interactive),
        cwd_filter.map(|path| path.to_string_lossy().into_owned()),
    );
    app_gateway
        .thread_lookup(params)
        .await
        .map(|thread| thread.and_then(session_target_from_app_gateway_thread))
}

pub(super) fn session_lookup_params(
    selector: ThreadLookupSelector,
    source_kinds: Option<Vec<ThreadSourceKind>>,
    cwd_scope: Option<String>,
) -> ThreadLookupParams {
    ThreadLookupParams {
        selector,
        include_turns: false,
        turn_limit: None,
        source_kinds,
        cwd_scope,
        archived: Some(false),
    }
}

pub(super) fn session_lookup_command_hint(action: &str, source: SessionLookupSource) -> String {
    match source.command_keyword() {
        Some(keyword) => format!("praxis {action} {keyword}"),
        None => format!("praxis {action}"),
    }
}

pub(super) struct SessionLookupContext {
    pub(super) source: SessionLookupSource,
    pub(super) config: Config,
    pub(super) app_gateway: AppGatewaySession,
}

async fn build_external_bridge_lookup_config(
    source: praxis_core::external_agent_migration::ExternalAgentSource,
    primary_config: &Config,
) -> std::io::Result<Config> {
    let bridge_config = source.bridge_config(primary_config);
    praxis_core::external_agent_migration::sync_external_agent_sessions_to_praxis_home(
        source,
        &bridge_config,
    )
    .await?;
    Ok(bridge_config)
}

pub(crate) async fn build_session_lookup_config(
    source: SessionLookupSource,
    primary_config: &Config,
) -> std::io::Result<Config> {
    match source {
        SessionLookupSource::Praxis => Ok(primary_config.clone()),
        SessionLookupSource::Codex => {
            build_external_bridge_lookup_config(
                praxis_core::external_agent_migration::ExternalAgentSource::Codex,
                primary_config,
            )
            .await
        }
        SessionLookupSource::Cursor => {
            build_external_bridge_lookup_config(
                praxis_core::external_agent_migration::ExternalAgentSource::Cursor,
                primary_config,
            )
            .await
        }
    }
}

pub(crate) fn picker_source_switch_enabled(app_gateway_target: &AppGatewayTarget) -> bool {
    matches!(current_praxis_home_namespace(), PraxisHomeNamespace::Praxis)
        && matches!(app_gateway_target, AppGatewayTarget::Embedded)
}

pub(crate) fn session_lookup_app_gateway_target(
    source: SessionLookupSource,
    app_gateway_target: &AppGatewayTarget,
) -> AppGatewayTarget {
    if source.is_external() {
        AppGatewayTarget::Embedded
    } else {
        app_gateway_target.clone()
    }
}

pub(super) async fn start_session_lookup_context(
    source: SessionLookupSource,
    primary_config: &Config,
    app_gateway_target: &AppGatewayTarget,
    arg0_paths: Arg0DispatchPaths,
    loader_overrides: LoaderOverrides,
    feedback: praxis_feedback::PraxisFeedback,
) -> color_eyre::Result<SessionLookupContext> {
    let lookup_config = build_session_lookup_config(source, primary_config)
        .await
        .map_err(color_eyre::Report::new)?;
    let app_gateway = start_app_gateway(
        app_gateway_target,
        arg0_paths,
        lookup_config.clone(),
        Vec::new(),
        loader_overrides,
        CloudConfigBundleLoader::default(),
        feedback,
        None,
    )
    .await?;
    Ok(SessionLookupContext {
        source,
        config: lookup_config,
        app_gateway: AppGatewaySession::new(app_gateway),
    })
}
