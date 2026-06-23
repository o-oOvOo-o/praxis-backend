use crate::backend::init_backend;
use crate::command_support::resolve_environment_id;
use crate::util;
use crate::util::format_relative_time;
use anyhow::anyhow;
use chrono::Utc;
use owo_colors::OwoColorize;
use owo_colors::Stream;
use praxis_cloud_tasks_client::TaskStatus;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_utils_cli::CliConfigOverrides;
use std::cmp::Ordering;
use supports_color::Stream as SupportStream;

pub(crate) fn parse_task_id(raw: &str) -> anyhow::Result<praxis_cloud_tasks_client::TaskId> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("task id must not be empty");
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let id = without_query
        .rsplit('/')
        .next()
        .unwrap_or(without_query)
        .trim();
    if id.is_empty() {
        anyhow::bail!("task id must not be empty");
    }
    Ok(praxis_cloud_tasks_client::TaskId(id.to_string()))
}

#[derive(Clone, Debug)]
pub(crate) struct AttemptDiffData {
    pub(crate) placement: Option<i64>,
    pub(crate) created_at: Option<chrono::DateTime<Utc>>,
    pub(crate) diff: String,
}

fn cmp_attempt(lhs: &AttemptDiffData, rhs: &AttemptDiffData) -> Ordering {
    match (lhs.placement, rhs.placement) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => match (lhs.created_at, rhs.created_at) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
    }
}

pub(crate) async fn collect_attempt_diffs(
    backend: &dyn praxis_cloud_tasks_client::CloudBackend,
    task_id: &praxis_cloud_tasks_client::TaskId,
) -> anyhow::Result<Vec<AttemptDiffData>> {
    let text =
        praxis_cloud_tasks_client::CloudBackend::get_task_text(backend, task_id.clone()).await?;
    let mut attempts = Vec::new();
    if let Some(diff) =
        praxis_cloud_tasks_client::CloudBackend::get_task_diff(backend, task_id.clone()).await?
    {
        attempts.push(AttemptDiffData {
            placement: text.attempt_placement,
            created_at: None,
            diff,
        });
    }
    if let Some(turn_id) = text.turn_id {
        let siblings = praxis_cloud_tasks_client::CloudBackend::list_sibling_attempts(
            backend,
            task_id.clone(),
            turn_id,
        )
        .await?;
        for sibling in siblings {
            if let Some(diff) = sibling.diff {
                attempts.push(AttemptDiffData {
                    placement: sibling.attempt_placement,
                    created_at: sibling.created_at,
                    diff,
                });
            }
        }
    }
    attempts.sort_by(cmp_attempt);
    if attempts.is_empty() {
        anyhow::bail!(
            "No diff available for task {}; it may still be running.",
            task_id.0
        );
    }
    Ok(attempts)
}

pub(crate) fn select_attempt(
    attempts: &[AttemptDiffData],
    attempt: Option<usize>,
) -> anyhow::Result<&AttemptDiffData> {
    if attempts.is_empty() {
        anyhow::bail!("No attempts available");
    }
    let desired = attempt.unwrap_or(1);
    let idx = desired
        .checked_sub(1)
        .ok_or_else(|| anyhow!("attempt must be at least 1"))?;
    if idx >= attempts.len() {
        anyhow::bail!(
            "Attempt {desired} not available; only {} attempt(s) found",
            attempts.len()
        );
    }
    Ok(&attempts[idx])
}

fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "PENDING",
        TaskStatus::Ready => "READY",
        TaskStatus::Applied => "APPLIED",
        TaskStatus::Error => "ERROR",
    }
}

fn summary_line(summary: &praxis_cloud_tasks_client::DiffSummary, colorize: bool) -> String {
    if summary.files_changed == 0 && summary.lines_added == 0 && summary.lines_removed == 0 {
        let base = "no diff";
        return if colorize {
            base.if_supports_color(Stream::Stdout, |text| text.dimmed())
                .to_string()
        } else {
            base.to_string()
        };
    }
    let adds = summary.lines_added;
    let dels = summary.lines_removed;
    let files = summary.files_changed;
    if colorize {
        let adds_raw = format!("+{adds}");
        let adds_str = adds_raw
            .as_str()
            .if_supports_color(Stream::Stdout, |text| text.green())
            .to_string();
        let dels_raw = format!("-{dels}");
        let dels_str = dels_raw
            .as_str()
            .if_supports_color(Stream::Stdout, |text| text.red())
            .to_string();
        let bullet = "•"
            .if_supports_color(Stream::Stdout, |text| text.dimmed())
            .to_string();
        let file_label = format!("file{}", if files == 1 { "" } else { "s" })
            .if_supports_color(Stream::Stdout, |text| text.dimmed())
            .to_string();
        format!("{adds_str}/{dels_str}  {bullet}  {files} {file_label}")
    } else {
        format!(
            "+{adds}/-{dels} • {files} file{}",
            if files == 1 { "" } else { "s" }
        )
    }
}

pub(crate) fn format_task_status_lines(
    task: &praxis_cloud_tasks_client::TaskSummary,
    now: chrono::DateTime<Utc>,
    colorize: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let status = task_status_label(&task.status);
    let status = if colorize {
        match task.status {
            TaskStatus::Ready => status
                .if_supports_color(Stream::Stdout, |text| text.green())
                .to_string(),
            TaskStatus::Pending => status
                .if_supports_color(Stream::Stdout, |text| text.magenta())
                .to_string(),
            TaskStatus::Applied => status
                .if_supports_color(Stream::Stdout, |text| text.blue())
                .to_string(),
            TaskStatus::Error => status
                .if_supports_color(Stream::Stdout, |text| text.red())
                .to_string(),
        }
    } else {
        status.to_string()
    };
    lines.push(format!("[{status}] {}", task.title));
    let mut meta_parts = Vec::new();
    if let Some(label) = task
        .environment_label
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if colorize {
            meta_parts.push(
                label
                    .if_supports_color(Stream::Stdout, |text| text.dimmed())
                    .to_string(),
            );
        } else {
            meta_parts.push(label.to_string());
        }
    } else if let Some(id) = task.environment_id.as_deref() {
        if colorize {
            meta_parts.push(
                id.if_supports_color(Stream::Stdout, |text| text.dimmed())
                    .to_string(),
            );
        } else {
            meta_parts.push(id.to_string());
        }
    }
    let when = format_relative_time(now, task.updated_at);
    meta_parts.push(if colorize {
        when.as_str()
            .if_supports_color(Stream::Stdout, |text| text.dimmed())
            .to_string()
    } else {
        when
    });
    let sep = if colorize {
        "  •  "
            .if_supports_color(Stream::Stdout, |text| text.dimmed())
            .to_string()
    } else {
        "  •  ".to_string()
    };
    lines.push(meta_parts.join(&sep));
    lines.push(summary_line(&task.summary, colorize));
    lines
}

pub(crate) fn format_task_list_lines(
    tasks: &[praxis_cloud_tasks_client::TaskSummary],
    base_url: &str,
    now: chrono::DateTime<Utc>,
    colorize: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (idx, task) in tasks.iter().enumerate() {
        lines.push(util::task_url(base_url, &task.id.0));
        for line in format_task_status_lines(task, now, colorize) {
            lines.push(format!("  {line}"));
        }
        if idx + 1 < tasks.len() {
            lines.push(String::new());
        }
    }
    lines
}

pub(crate) async fn run_status_command(
    args: crate::cli::StatusCommand,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let ctx = init_backend("praxis_cloud_tasks_status", config_overrides).await?;
    let task_id = parse_task_id(&args.task_id)?;
    let summary =
        praxis_cloud_tasks_client::CloudBackend::get_task_summary(&*ctx.backend, task_id).await?;
    let now = Utc::now();
    let colorize = supports_color::on(SupportStream::Stdout).is_some();
    for line in format_task_status_lines(&summary, now, colorize) {
        println!("{line}");
    }
    if !matches!(summary.status, TaskStatus::Ready) {
        std::process::exit(1);
    }
    Ok(())
}

pub(crate) async fn run_list_command(
    args: crate::cli::ListCommand,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let ctx = init_backend("praxis_cloud_tasks_list", config_overrides).await?;
    let env_filter = if let Some(env) = args.environment {
        Some(resolve_environment_id(&ctx, &env).await?)
    } else {
        None
    };
    let page = praxis_cloud_tasks_client::CloudBackend::list_tasks(
        &*ctx.backend,
        env_filter.as_deref(),
        Some(args.limit),
        args.cursor.as_deref(),
    )
    .await?;
    if args.json {
        let tasks: Vec<_> = page
            .tasks
            .iter()
            .map(|task| {
                serde_json::json!({
                    "id": task.id.0,
                    "url": util::task_url(&ctx.base_url, &task.id.0),
                    "title": task.title,
                    "status": task.status,
                    "updated_at": task.updated_at,
                    "environment_id": task.environment_id,
                    "environment_label": task.environment_label,
                    "summary": {
                        "files_changed": task.summary.files_changed,
                        "lines_added": task.summary.lines_added,
                        "lines_removed": task.summary.lines_removed,
                    },
                    "is_review": task.is_review,
                    "attempt_total": task.attempt_total,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "tasks": tasks,
            "cursor": page.cursor,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if page.tasks.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }
    let now = Utc::now();
    let colorize = supports_color::on(SupportStream::Stdout).is_some();
    for line in format_task_list_lines(&page.tasks, &ctx.base_url, now, colorize) {
        println!("{line}");
    }
    if let Some(cursor) = page.cursor {
        let command = format!("{PRIMARY_CLI_COMMAND} cloud list --cursor='{cursor}'");
        if colorize {
            println!(
                "\nTo fetch the next page, run {}",
                command.if_supports_color(Stream::Stdout, |text| text.cyan())
            );
        } else {
            println!("\nTo fetch the next page, run {command}");
        }
    }
    Ok(())
}

pub(crate) async fn run_diff_command(
    args: crate::cli::DiffCommand,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let ctx = init_backend("praxis_cloud_tasks_diff", config_overrides).await?;
    let task_id = parse_task_id(&args.task_id)?;
    let attempts = collect_attempt_diffs(&*ctx.backend, &task_id).await?;
    let selected = select_attempt(&attempts, args.attempt)?;
    print!("{}", selected.diff);
    Ok(())
}

pub(crate) async fn run_apply_command(
    args: crate::cli::ApplyCommand,
    config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let ctx = init_backend("praxis_cloud_tasks_apply", config_overrides).await?;
    let task_id = parse_task_id(&args.task_id)?;
    let attempts = collect_attempt_diffs(&*ctx.backend, &task_id).await?;
    let selected = select_attempt(&attempts, args.attempt)?;
    let outcome = praxis_cloud_tasks_client::CloudBackend::apply_task(
        &*ctx.backend,
        task_id,
        Some(selected.diff.clone()),
    )
    .await?;
    println!("{}", outcome.message);
    if !matches!(
        outcome.status,
        praxis_cloud_tasks_client::ApplyStatus::Success
    ) {
        std::process::exit(1);
    }
    Ok(())
}
