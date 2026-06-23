use crate::util;
use crate::util::append_error_log;
use crate::util::set_user_agent_suffix;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_login::default_client::get_praxis_user_agent;
use praxis_utils_cli::CliConfigOverrides;
use std::sync::Arc;

pub(crate) struct BackendContext {
    pub(crate) backend: Arc<dyn praxis_cloud_tasks_client::CloudBackend>,
    pub(crate) base_url: String,
    pub(crate) config_overrides: CliConfigOverrides,
}

pub(crate) async fn init_backend(
    user_agent_suffix: &str,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<BackendContext> {
    #[cfg(debug_assertions)]
    let use_mock = matches!(
        std::env::var("PRAXIS_CLOUD_TASKS_MODE").ok().as_deref(),
        Some("mock") | Some("MOCK")
    );
    let base_url = std::env::var("PRAXIS_CLOUD_TASKS_BASE_URL")
        .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string());

    set_user_agent_suffix(user_agent_suffix);

    #[cfg(debug_assertions)]
    if use_mock {
        return Ok(BackendContext {
            backend: Arc::new(praxis_cloud_tasks_mock_client::MockClient),
            base_url,
            config_overrides: config_overrides.clone(),
        });
    }

    let ua = get_praxis_user_agent();
    let mut http =
        praxis_cloud_tasks_client::HttpClient::new(base_url.clone())?.with_user_agent(ua);
    let style = if base_url.contains("/backend-api") {
        "wham"
    } else {
        "praxis-api"
    };
    append_error_log(format!("startup: base_url={base_url} path_style={style}"));

    let auth_manager = util::load_auth_manager(config_overrides).await?;
    let auth = auth_manager.auth().await;
    let auth = match auth {
        Some(auth) => auth,
        None => {
            eprintln!(
                "Not signed in. Please run '{PRIMARY_CLI_COMMAND} login' to sign in with ChatGPT, then re-run '{PRIMARY_CLI_COMMAND} cloud'."
            );
            std::process::exit(1);
        }
    };

    if let Some(acc) = auth.get_account_id() {
        append_error_log(format!("auth: mode=ChatGPT account_id={acc}"));
    }

    let token = match auth.get_token() {
        Ok(token) if !token.is_empty() => token,
        _ => {
            eprintln!(
                "Not signed in. Please run '{PRIMARY_CLI_COMMAND} login' to sign in with ChatGPT, then re-run '{PRIMARY_CLI_COMMAND} cloud'."
            );
            std::process::exit(1);
        }
    };

    http = http.with_bearer_token(token.clone());
    if let Some(acc) = auth
        .get_account_id()
        .or_else(|| util::extract_chatgpt_account_id(&token))
    {
        append_error_log(format!("auth: set ChatGPT-Account-Id header: {acc}"));
        http = http.with_chatgpt_account_id(acc);
    }

    Ok(BackendContext {
        backend: Arc::new(http),
        base_url,
        config_overrides: config_overrides.clone(),
    })
}
