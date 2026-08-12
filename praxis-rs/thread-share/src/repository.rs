use crate::model::ExportIdentity;
use crate::model::ExportStats;
use crate::model::ParsedThread;
use crate::model::PraxisMetadata;
use crate::model::Provenance;
use crate::model::PublishOutcome;
use crate::model::SourceMetadata;
use crate::model::SubmittedBy;
use crate::model::ThreadExport;
use crate::model::ThreadIndex;
use crate::model::ThreadIndexEntry;
use crate::model::WorkspaceMetadata;
use crate::model::WriteOutcome;
use crate::parse_rollout;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;

pub struct PublishRequest<'a> {
    pub rollout_path: &'a Path,
    pub thread_id: &'a str,
    pub repository_path: &'a Path,
    pub team: &'a str,
    pub mode: PublishMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishMode {
    CommitOnly,
    Push,
}

pub fn discover_repository(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .map(|root| root.join(".local").join("praxis-threads"))
        .find(|candidate| candidate.join(".git").is_dir())
}

pub fn publish_thread(request: PublishRequest<'_>) -> Result<PublishOutcome> {
    ensure_repository(request.repository_path)?;
    let _lock = RepositoryLock::acquire(request.repository_path)?;
    ensure_clean(request.repository_path)?;
    if request.mode == PublishMode::Push {
        run_git(request.repository_path, &["pull", "--ff-only"])?;
    }

    let bytes = fs::read(request.rollout_path)
        .with_context(|| format!("failed to read rollout {}", request.rollout_path.display()))?;
    let parsed = parse_rollout(&bytes)?;
    if parsed.thread_id != request.thread_id {
        bail!(
            "thread id mismatch: command supplied `{}`, rollout contains `{}`",
            request.thread_id,
            parsed.thread_id
        );
    }
    let identity = repository_identity(request.repository_path)?;
    let published_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let write = write_export(
        request.repository_path,
        &parsed,
        &identity,
        request.team,
        &published_at,
    )?;

    run_git(
        request.repository_path,
        &["add", "--", "index.json", write.relative_path.as_str()],
    )?;
    let commit_message = format!("Share Praxis thread {}", request.thread_id);
    run_git(
        request.repository_path,
        &["commit", "-m", commit_message.as_str()],
    )?;
    if request.mode == PublishMode::Push {
        run_git(request.repository_path, &["push", "origin", "HEAD"])?;
    }

    let commit = git_stdout(request.repository_path, &["rev-parse", "HEAD"])?;
    let web_url = repository_web_url(request.repository_path, &write.relative_path).ok();
    Ok(PublishOutcome {
        thread_id: parsed.thread_id,
        relative_path: write.relative_path,
        commit,
        web_url,
        pushed: request.mode == PublishMode::Push,
        project: write.project,
        team: write.team,
        message_count: write.message_count,
        redaction_count: write.redaction_count,
    })
}

pub fn write_export(
    repository_path: &Path,
    parsed: &ParsedThread,
    identity: &ExportIdentity,
    team: &str,
    published_at: &str,
) -> Result<WriteOutcome> {
    let created_at = DateTime::parse_from_rfc3339(&parsed.created_at)
        .context("rollout creation timestamp is not RFC3339")?;
    let relative_path = format!(
        "threads/{:04}/{:02}/{}.json",
        created_at.year(),
        created_at.month(),
        parsed.thread_id
    );
    let output_path = repository_path.join(Path::new(&relative_path));
    let workspace = workspace_metadata(parsed.repository.as_deref(), team)?;
    let export = ThreadExport {
        schema: "../../../schema/thread.v2.schema.json".to_string(),
        schema_version: 2,
        thread_id: parsed.thread_id.clone(),
        title: parsed.title.clone(),
        submitted_by: SubmittedBy {
            github_login: identity.github_login.clone(),
            git_name: identity.git_name.clone(),
        },
        created_at: parsed.created_at.clone(),
        published_at: published_at.to_string(),
        workspace: Some(workspace.clone()),
        source: SourceMetadata {
            repository: parsed.repository.clone(),
            branch: parsed.branch.clone(),
            commit: parsed.commit.clone(),
        },
        praxis: PraxisMetadata {
            model: parsed.model.clone(),
            model_provider: parsed.model_provider.clone(),
            cli_version: parsed.cli_version.clone(),
            originator: parsed.originator.clone(),
        },
        stats: ExportStats {
            message_count: parsed.conversation.len(),
            redaction_count: parsed.redaction_count,
        },
        conversation: parsed.conversation.clone(),
        provenance: Provenance {
            rollout_sha256: parsed.rollout_sha256.clone(),
            redactions: parsed.redactions.clone(),
        },
    };
    write_json_atomic(&output_path, &export)?;
    rebuild_index(repository_path, Some(published_at.to_string()))?;
    Ok(WriteOutcome {
        relative_path,
        project: workspace.project,
        team: workspace.team,
        message_count: parsed.conversation.len(),
        redaction_count: parsed.redaction_count,
    })
}

fn rebuild_index(repository_path: &Path, generated_at: Option<String>) -> Result<()> {
    let threads_root = repository_path.join("threads");
    let mut files = Vec::new();
    collect_json_files(&threads_root, &mut files)?;
    let mut threads = Vec::new();
    for path in files {
        let export: ThreadExport = serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        let relative = path
            .strip_prefix(repository_path)
            .context("thread export escaped repository root")?
            .to_string_lossy()
            .replace('\\', "/");
        let workspace = export.workspace.unwrap_or(workspace_metadata(
            export.source.repository.as_deref(),
            "General",
        )?);
        threads.push(ThreadIndexEntry {
            thread_id: export.thread_id,
            title: export.title,
            submitted_by: export.submitted_by.github_login,
            published_at: export.published_at,
            path: relative,
            message_count: export.stats.message_count,
            project: workspace.project,
            project_key: workspace.project_key,
            team: workspace.team,
            team_key: workspace.team_key,
            model: export.praxis.model,
            repository: export.source.repository,
        });
    }
    threads.sort_by(|left, right| {
        right
            .published_at
            .cmp(&left.published_at)
            .then_with(|| left.thread_id.cmp(&right.thread_id))
    });
    write_json_atomic(
        &repository_path.join("index.json"),
        &ThreadIndex {
            schema: "./schema/index.v2.schema.json".to_string(),
            schema_version: 2,
            generated_at,
            threads,
        },
    )
}

fn workspace_metadata(repository: Option<&str>, team: &str) -> Result<WorkspaceMetadata> {
    let repository = repository.context(
        "thread source has no GitHub repository; Praxis projects derive from the current repository",
    )?;
    let project = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("git@github.com:"))
        .map(|value| value.trim_end_matches(".git"))
        .filter(|value| value.split('/').count() == 2 && !value.contains(char::is_whitespace))
        .context("thread source is not a canonical GitHub repository")?;
    let team = team.trim();
    if team.is_empty() || team.chars().count() > 64 || team.chars().any(char::is_control) {
        bail!("team must contain 1-64 visible characters");
    }
    let team_key = room_key(team);
    if team_key.is_empty() {
        bail!("team must contain at least one letter or number");
    }
    Ok(WorkspaceMetadata {
        project: project.to_string(),
        project_key: format!("github:{}", project.to_lowercase()),
        team: team.to_string(),
        team_key,
    })
}

fn room_key(value: &str) -> String {
    let mut key = String::new();
    for character in value.to_lowercase().chars() {
        if character.is_alphanumeric() {
            key.push(character);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    key.trim_end_matches('-').to_string()
}

fn collect_json_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read directory {}", directory.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_json_files(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
            output.push(path);
        }
    }
    Ok(())
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("output path {} has no parent", path.display()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, value)?;
    writeln!(temporary)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically write {}", path.display()))?;
    Ok(())
}

fn ensure_repository(repository_path: &Path) -> Result<()> {
    if !repository_path.join(".git").is_dir() {
        bail!(
            "thread share repository is not a Git checkout: {}",
            repository_path.display()
        );
    }
    Ok(())
}

fn ensure_clean(repository_path: &Path) -> Result<()> {
    let status = git_stdout(
        repository_path,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.is_empty() {
        bail!("thread share repository has uncommitted changes");
    }
    Ok(())
}

fn repository_identity(repository_path: &Path) -> Result<ExportIdentity> {
    let origin = git_stdout(repository_path, &["config", "--get", "remote.origin.url"])?;
    let github_login = github_owner(&origin)
        .with_context(|| format!("origin is not a supported GitHub URL: {origin}"))?;
    let git_name = git_stdout(repository_path, &["config", "--get", "user.name"])
        .ok()
        .filter(|value| !value.is_empty());
    Ok(ExportIdentity {
        github_login,
        git_name,
    })
}

fn github_owner(origin: &str) -> Option<String> {
    let path = origin
        .strip_prefix("https://github.com/")
        .or_else(|| origin.strip_prefix("git@github.com:"))?;
    path.split('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn repository_web_url(repository_path: &Path, relative_path: &str) -> Result<String> {
    let origin = git_stdout(repository_path, &["config", "--get", "remote.origin.url"])?;
    let branch = git_stdout(repository_path, &["branch", "--show-current"])?;
    let base = if let Some(value) = origin.strip_prefix("git@github.com:") {
        format!("https://github.com/{}", value.trim_end_matches(".git"))
    } else {
        origin.trim_end_matches(".git").to_string()
    };
    Ok(format!("{base}/blob/{branch}/{relative_path}"))
}

fn run_git(repository_path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository_path)
        .output()
        .context("failed to launch git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn git_stdout(repository_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository_path)
        .output()
        .context("failed to launch git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct RepositoryLock {
    path: PathBuf,
}

impl RepositoryLock {
    fn acquire(repository_path: &Path) -> Result<Self> {
        let path = repository_path
            .join(".git")
            .join("praxis-thread-share.lock");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| "another Praxis thread share is already in progress")?;
        Ok(Self { path })
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
