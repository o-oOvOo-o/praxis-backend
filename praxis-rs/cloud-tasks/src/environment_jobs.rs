use crate::app;
use crate::env_detect;
use crate::util;
use praxis_utils_cli::CliConfigOverrides;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) fn spawn_environment_list(
    tx: UnboundedSender<app::AppEvent>,
    config_overrides: CliConfigOverrides,
) {
    tokio::spawn(async move {
        let result = async {
            let base_url = cloud_base_url();
            let headers = util::build_chatgpt_headers(&config_overrides).await?;
            env_detect::list_environments(&base_url, &headers).await
        }
        .await;
        let _ = tx.send(app::AppEvent::EnvironmentsLoaded(result));
    });
}

pub(crate) fn spawn_environment_autodetect(
    tx: UnboundedSender<app::AppEvent>,
    config_overrides: CliConfigOverrides,
    desired_label: Option<String>,
) {
    tokio::spawn(async move {
        let result = async {
            let base_url = cloud_base_url();
            let headers = util::build_chatgpt_headers(&config_overrides).await?;
            env_detect::autodetect_environment_id(&base_url, &headers, desired_label).await
        }
        .await;
        let _ = tx.send(app::AppEvent::EnvironmentAutodetected(result));
    });
}

fn cloud_base_url() -> String {
    util::normalize_base_url(
        &std::env::var("PRAXIS_CLOUD_TASKS_BASE_URL")
            .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string()),
    )
}
