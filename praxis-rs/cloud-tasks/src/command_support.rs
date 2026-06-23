use crate::backend::BackendContext;
use crate::backend::init_backend;
use crate::util;
use anyhow::anyhow;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_git_utils::current_branch_name;
use praxis_git_utils::default_branch_name;
use praxis_utils_cli::CliConfigOverrides;
use std::io::IsTerminal;
use std::io::Read;

#[async_trait::async_trait]
pub(crate) trait GitInfoProvider {
    async fn default_branch_name(&self, path: &std::path::Path) -> Option<String>;

    async fn current_branch_name(&self, path: &std::path::Path) -> Option<String>;
}

struct RealGitInfo;

#[async_trait::async_trait]
impl GitInfoProvider for RealGitInfo {
    async fn default_branch_name(&self, path: &std::path::Path) -> Option<String> {
        default_branch_name(path).await
    }

    async fn current_branch_name(&self, path: &std::path::Path) -> Option<String> {
        current_branch_name(path).await
    }
}

pub(crate) async fn resolve_git_ref(branch_override: Option<&String>) -> String {
    resolve_git_ref_with_git_info(branch_override, &RealGitInfo).await
}

pub(crate) async fn resolve_git_ref_with_git_info(
    branch_override: Option<&String>,
    git_info: &impl GitInfoProvider,
) -> String {
    if let Some(branch) = branch_override {
        let branch = branch.trim();
        if !branch.is_empty() {
            return branch.to_string();
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(branch) = git_info.current_branch_name(&cwd).await {
            branch
        } else if let Some(branch) = git_info.default_branch_name(&cwd).await {
            branch
        } else {
            "main".to_string()
        }
    } else {
        "main".to_string()
    }
}

pub(crate) async fn run_exec_command(
    args: crate::cli::ExecCommand,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let crate::cli::ExecCommand {
        query,
        environment,
        branch,
        attempts,
    } = args;
    let ctx = init_backend("praxis_cloud_tasks_exec", config_overrides).await?;
    let prompt = resolve_query_input(query)?;
    let env_id = resolve_environment_id(&ctx, &environment).await?;
    let git_ref = resolve_git_ref(branch.as_ref()).await;
    let created = praxis_cloud_tasks_client::CloudBackend::create_task(
        &*ctx.backend,
        &env_id,
        &prompt,
        &git_ref,
        /*qa_mode*/ false,
        attempts,
    )
    .await?;
    let url = util::task_url(&ctx.base_url, &created.id.0);
    println!("{url}");
    Ok(())
}

pub(crate) async fn resolve_environment_id(
    ctx: &BackendContext,
    requested: &str,
) -> anyhow::Result<String> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("environment id must not be empty"));
    }
    let normalized = util::normalize_base_url(&ctx.base_url);
    let headers = util::build_chatgpt_headers(&ctx.config_overrides).await?;
    let environments = crate::env_detect::list_environments(&normalized, &headers).await?;
    if environments.is_empty() {
        return Err(anyhow!(
            "no cloud environments are available for this workspace"
        ));
    }

    if let Some(row) = environments.iter().find(|row| row.id == trimmed) {
        return Ok(row.id.clone());
    }

    let label_matches = environments
        .iter()
        .filter(|row| {
            row.label
                .as_deref()
                .map(|label| label.eq_ignore_ascii_case(trimmed))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    match label_matches.as_slice() {
        [] => Err(anyhow!(
            "environment '{trimmed}' not found; run `{PRIMARY_CLI_COMMAND} cloud` to list available environments"
        )),
        [single] => Ok(single.id.clone()),
        [first, rest @ ..] => {
            let first_id = &first.id;
            if rest.iter().all(|row| row.id == *first_id) {
                Ok(first_id.clone())
            } else {
                Err(anyhow!(
                    "environment label '{trimmed}' is ambiguous; run `{PRIMARY_CLI_COMMAND} cloud` to pick the desired environment id"
                ))
            }
        }
    }
}

fn resolve_query_input(query_arg: Option<String>) -> anyhow::Result<String> {
    match query_arg {
        Some(query) if query != "-" => Ok(query),
        maybe_dash => {
            let force_stdin = matches!(maybe_dash.as_deref(), Some("-"));
            if std::io::stdin().is_terminal() && !force_stdin {
                return Err(anyhow!(
                    "no query provided. Pass one as an argument or pipe it via stdin."
                ));
            }
            if !force_stdin {
                eprintln!("Reading query from stdin...");
            }
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| anyhow!("failed to read query from stdin: {error}"))?;
            if buffer.trim().is_empty() {
                return Err(anyhow!(
                    "no query provided via stdin (received empty input)."
                ));
            }
            Ok(buffer)
        }
    }
}
