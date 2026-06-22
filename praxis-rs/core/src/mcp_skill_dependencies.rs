use std::collections::HashMap;

use praxis_config::ConfigEditsBuilder;
use praxis_config::McpServerConfig;
use praxis_config::load_global_mcp_servers;
use praxis_login::default_client::is_first_party_originator;
use praxis_login::default_client::originator;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SkillDependencies as ProtocolSkillDependencies;
use praxis_protocol::protocol::SkillInterface as ProtocolSkillInterface;
use praxis_protocol::protocol::SkillMetadata as ProtocolSkillMetadata;
use praxis_protocol::protocol::SkillToolDependency as ProtocolSkillToolDependency;
use praxis_protocol::request_user_input::RequestUserInputArgs;
use praxis_protocol::request_user_input::RequestUserInputQuestion;
use praxis_protocol::request_user_input::RequestUserInputQuestionOption;
use praxis_protocol::request_user_input::RequestUserInputResponse;
use praxis_rmcp_client::perform_oauth_login;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::SkillMetadata;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_mcp::mcp::auth::McpOAuthLoginSupport;
use praxis_mcp::mcp::auth::oauth_login_support;
use praxis_mcp::mcp::auth::resolve_oauth_scopes;
use praxis_mcp::mcp::auth::should_retry_without_scopes;
use praxis_mcp::mcp::canonical_mcp_server_key;
use praxis_mcp::mcp::collect_missing_mcp_dependencies;

const SKILL_MCP_DEPENDENCY_PROMPT_ID: &str = "skill_mcp_dependency_install";
const MCP_DEPENDENCY_OPTION_INSTALL: &str = "Install";
const MCP_DEPENDENCY_OPTION_SKIP: &str = "Continue anyway";

pub(crate) async fn maybe_prompt_and_install_mcp_dependencies(
    sess: &Session,
    turn_context: &TurnContext,
    cancellation_token: &CancellationToken,
    mentioned_skills: &[SkillMetadata],
) {
    let originator_value = originator().value;
    if !is_first_party_originator(originator_value.as_str()) {
        // Only support first-party clients for now.
        return;
    }

    let config = turn_context.config.clone();
    if mentioned_skills.is_empty()
        || !config
            .features
            .enabled(praxis_features::Feature::SkillMcpDependencyInstall)
    {
        return;
    }

    let installed = sess
        .services
        .mcp_manager
        .configured_servers(config.as_ref());
    let mentioned_skills_for_mcp = protocol_skill_metadata_for_mcp(mentioned_skills);
    let missing = collect_missing_mcp_dependencies(&mentioned_skills_for_mcp, &installed);
    if missing.is_empty() {
        return;
    }

    let unprompted_missing = filter_prompted_mcp_dependencies(sess, &missing).await;
    if unprompted_missing.is_empty() {
        return;
    }

    if should_install_mcp_dependencies(sess, turn_context, &unprompted_missing, cancellation_token)
        .await
    {
        maybe_install_mcp_dependencies(sess, turn_context, config.as_ref(), mentioned_skills).await;
    }
}

pub(crate) async fn maybe_install_mcp_dependencies(
    sess: &Session,
    turn_context: &TurnContext,
    config: &crate::config::Config,
    mentioned_skills: &[SkillMetadata],
) {
    if mentioned_skills.is_empty()
        || !config
            .features
            .enabled(praxis_features::Feature::SkillMcpDependencyInstall)
    {
        return;
    }

    let praxis_home = config.praxis_home.clone();
    let installed = sess.services.mcp_manager.configured_servers(config);
    let mentioned_skills_for_mcp = protocol_skill_metadata_for_mcp(mentioned_skills);
    let missing = collect_missing_mcp_dependencies(&mentioned_skills_for_mcp, &installed);
    if missing.is_empty() {
        return;
    }

    let mut servers = match load_global_mcp_servers(&praxis_home).await {
        Ok(servers) => servers,
        Err(err) => {
            warn!("failed to load MCP servers while installing skill dependencies: {err}");
            return;
        }
    };

    let mut updated = false;
    let mut added = Vec::new();
    for (name, config) in missing {
        if servers.contains_key(&name) {
            continue;
        }
        servers.insert(name.clone(), config.clone());
        added.push((name, config));
        updated = true;
    }

    if !updated {
        return;
    }

    if let Err(err) = ConfigEditsBuilder::new(&praxis_home)
        .replace_mcp_servers(&servers)
        .apply()
        .await
    {
        warn!("failed to persist MCP dependencies for mentioned skills: {err}");
        return;
    }

    for (name, server_config) in added {
        let oauth_config = match oauth_login_support(&server_config.transport).await {
            McpOAuthLoginSupport::Supported(config) => config,
            McpOAuthLoginSupport::Unsupported => continue,
            McpOAuthLoginSupport::Unknown(err) => {
                warn!("MCP server may or may not require login for dependency {name}: {err}");
                continue;
            }
        };

        sess.notify_background_event(
            turn_context,
            format!(
                "Authenticating MCP {name}... Follow instructions in your browser if prompted."
            ),
        )
        .await;

        let resolved_scopes = resolve_oauth_scopes(
            /*explicit_scopes*/ None,
            server_config.scopes.clone(),
            oauth_config.discovered_scopes.clone(),
        );
        let first_attempt = perform_oauth_login(
            &name,
            &oauth_config.url,
            config.mcp_oauth_credentials_store_mode,
            oauth_config.http_headers.clone(),
            oauth_config.env_http_headers.clone(),
            &resolved_scopes.scopes,
            server_config.oauth_resource.as_deref(),
            config.mcp_oauth_callback_port,
            config.mcp_oauth_callback_url.as_deref(),
        )
        .await;

        if let Err(err) = first_attempt {
            if should_retry_without_scopes(&resolved_scopes, &err) {
                sess.notify_background_event(
                    turn_context,
                    format!(
                        "Retrying MCP {name} authentication without scopes after provider rejection."
                    ),
                )
                .await;

                if let Err(err) = perform_oauth_login(
                    &name,
                    &oauth_config.url,
                    config.mcp_oauth_credentials_store_mode,
                    oauth_config.http_headers,
                    oauth_config.env_http_headers,
                    &[],
                    server_config.oauth_resource.as_deref(),
                    config.mcp_oauth_callback_port,
                    config.mcp_oauth_callback_url.as_deref(),
                )
                .await
                {
                    warn!("failed to login to MCP dependency {name}: {err}");
                }
            } else {
                warn!("failed to login to MCP dependency {name}: {err}");
            }
        }
    }

    // Refresh from the effective merged MCP map (global + repo + managed) and
    // overlay the updated global servers so we don't drop repo-scoped servers.
    let auth = sess.services.auth_manager.auth().await;
    let mut refresh_servers = sess
        .services
        .mcp_manager
        .effective_servers(config, auth.as_ref());
    for (name, server_config) in &servers {
        refresh_servers
            .entry(name.clone())
            .or_insert_with(|| server_config.clone());
    }
    sess.refresh_mcp_servers_now(
        turn_context,
        refresh_servers,
        config.mcp_oauth_credentials_store_mode,
    )
    .await;
}

fn protocol_skill_metadata_for_mcp(skills: &[SkillMetadata]) -> Vec<ProtocolSkillMetadata> {
    skills
        .iter()
        .map(|skill| ProtocolSkillMetadata {
            name: skill.name.clone(),
            description: skill.description.clone(),
            short_description: skill.short_description.clone(),
            interface: skill
                .interface
                .clone()
                .map(|interface| ProtocolSkillInterface {
                    display_name: interface.display_name,
                    short_description: interface.short_description,
                    icon_small: interface.icon_small,
                    icon_large: interface.icon_large,
                    brand_color: interface.brand_color,
                    default_prompt: interface.default_prompt,
                }),
            dependencies: skill.dependencies.clone().map(|dependencies| {
                ProtocolSkillDependencies {
                    tools: dependencies
                        .tools
                        .into_iter()
                        .map(|tool| ProtocolSkillToolDependency {
                            r#type: tool.r#type,
                            value: tool.value,
                            description: tool.description,
                            transport: tool.transport,
                            command: tool.command,
                            url: tool.url,
                        })
                        .collect(),
                }
            }),
            path: skill.path_to_skills_md.clone(),
            scope: skill.scope,
            enabled: true,
        })
        .collect()
}

async fn should_install_mcp_dependencies(
    sess: &Session,
    turn_context: &TurnContext,
    missing: &HashMap<String, McpServerConfig>,
    cancellation_token: &CancellationToken,
) -> bool {
    if is_full_access_mode(turn_context) {
        return true;
    }

    let server_list = format_missing_mcp_dependencies(missing);
    let question = RequestUserInputQuestion {
        id: SKILL_MCP_DEPENDENCY_PROMPT_ID.to_string(),
        header: "Install MCP servers?".to_string(),
        question: format!(
            "The following MCP servers are required by the selected skills but are not installed yet: {server_list}. Install them now?"
        ),
        is_other: false,
        is_secret: false,
        options: Some(vec![
            RequestUserInputQuestionOption {
                label: MCP_DEPENDENCY_OPTION_INSTALL.to_string(),
                description:
                    "Install and enable the missing MCP servers in your global config."
                        .to_string(),
            },
            RequestUserInputQuestionOption {
                label: MCP_DEPENDENCY_OPTION_SKIP.to_string(),
                description: "Skip installation for now and do not show again for these MCP servers in this session."
                    .to_string(),
            },
        ]),
    };
    let args = RequestUserInputArgs {
        questions: vec![question],
    };
    let sub_id = &turn_context.sub_id;
    let call_id = format!("mcp-deps-{sub_id}");
    let response_fut = sess.request_user_input(turn_context, call_id, args);
    let response = tokio::select! {
        biased;
        _ = cancellation_token.cancelled() => {
            let empty = RequestUserInputResponse {
                answers: HashMap::new(),
            };
            sess.notify_user_input_response(sub_id, empty.clone()).await;
            empty
        }
        response = response_fut => response.unwrap_or_else(|| RequestUserInputResponse {
            answers: HashMap::new(),
        }),
    };

    let install = response
        .answers
        .get(SKILL_MCP_DEPENDENCY_PROMPT_ID)
        .is_some_and(|answer| {
            answer
                .answers
                .iter()
                .any(|entry| entry == MCP_DEPENDENCY_OPTION_INSTALL)
        });

    let prompted_keys = missing
        .iter()
        .map(|(name, config)| canonical_mcp_server_key(name, config));
    sess.record_mcp_dependency_prompted(prompted_keys).await;

    install
}

async fn filter_prompted_mcp_dependencies(
    sess: &Session,
    missing: &HashMap<String, McpServerConfig>,
) -> HashMap<String, McpServerConfig> {
    let prompted = sess.mcp_dependency_prompted().await;
    if prompted.is_empty() {
        return missing.clone();
    }

    missing
        .iter()
        .filter(|(name, config)| !prompted.contains(&canonical_mcp_server_key(name, config)))
        .map(|(name, config)| (name.clone(), config.clone()))
        .collect()
}

fn format_missing_mcp_dependencies(missing: &HashMap<String, McpServerConfig>) -> String {
    let mut names = missing.keys().cloned().collect::<Vec<_>>();
    names.sort();
    names.join(", ")
}

fn is_full_access_mode(turn_context: &TurnContext) -> bool {
    let permissions = turn_context.effective_permissions();
    matches!(permissions.approval_policy.value(), AskForApproval::Never)
        && matches!(
            permissions.sandbox_policy.get(),
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
}
