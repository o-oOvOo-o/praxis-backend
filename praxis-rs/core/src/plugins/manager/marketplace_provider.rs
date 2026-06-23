use super::*;

pub(super) fn marketplace_provider_source(
    provider: &PluginMarketplaceProviderConfig,
) -> PluginMarketplaceProviderSource {
    match provider {
        PluginMarketplaceProviderConfig::Local { path } => PluginMarketplaceProviderSource::Local {
            path: path.as_path().to_path_buf(),
        },
        PluginMarketplaceProviderConfig::Git {
            repo,
            reference,
            path,
        } => PluginMarketplaceProviderSource::Git {
            repo: repo.clone(),
            reference: reference.clone(),
            path: path.clone(),
        },
        PluginMarketplaceProviderConfig::Http { url } => {
            PluginMarketplaceProviderSource::Http { url: url.clone() }
        }
    }
}

pub(super) fn sync_git_marketplace_provider(
    praxis_home: &Path,
    marketplace_name: &str,
    repo: &str,
    reference: Option<&str>,
    path: Option<&Path>,
) -> Result<PluginMarketplaceSyncOutcome, String> {
    let relative_path = validate_marketplace_provider_relative_path(path)?;
    let repo_path = marketplace_provider_cache_root(praxis_home, marketplace_name);
    let old_version = if repo_path.join(".git").is_dir() {
        git_head_sha(repo_path.as_path(), "git").ok()
    } else {
        None
    };

    let staged_repo_dir = prepare_marketplace_provider_temp_dir(repo_path.as_path())?;
    clone_git_marketplace_repo(repo, reference, staged_repo_dir.path(), marketplace_name)?;
    ensure_marketplace_provider_manifest_exists(staged_repo_dir.path(), relative_path.as_deref())?;
    let new_version = git_head_sha(staged_repo_dir.path(), "git").ok();

    let changed = old_version != new_version;
    if changed || !repo_path.is_dir() {
        activate_marketplace_provider_repo(repo_path.as_path(), staged_repo_dir)?;
    }

    let local_root = relative_path
        .as_deref()
        .map(|path| repo_path.join(path))
        .unwrap_or(repo_path);

    Ok(PluginMarketplaceSyncOutcome {
        marketplace_name: marketplace_name.to_string(),
        changed,
        local_root: Some(local_root),
        version: new_version,
        diagnostics: Vec::new(),
    })
}

pub(super) fn marketplace_provider_cache_root(
    praxis_home: &Path,
    marketplace_name: &str,
) -> PathBuf {
    praxis_home
        .join(MARKETPLACE_PROVIDER_CACHE_DIR)
        .join(marketplace_name)
}

pub(super) fn validate_marketplace_provider_relative_path(
    path: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if path.as_os_str().is_empty() || path == Path::new(".") {
        return Ok(None);
    }
    if path.is_absolute() {
        return Err(format!(
            "git marketplace subpath must be relative, got {}",
            path.display()
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "git marketplace subpath must stay within the repository root, got {}",
            path.display()
        ));
    }
    Ok(Some(path.to_path_buf()))
}

pub(super) fn prepare_marketplace_provider_temp_dir(repo_path: &Path) -> Result<TempDir, String> {
    let Some(parent) = repo_path.parent() else {
        return Err(format!(
            "failed to determine marketplace cache parent directory for {}",
            repo_path.display()
        ));
    };
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create marketplace cache parent directory {}: {err}",
            parent.display()
        )
    })?;
    remove_stale_marketplace_provider_temp_dirs(parent);
    tempfile::Builder::new()
        .prefix("marketplace-clone-")
        .tempdir_in(parent)
        .map_err(|err| {
            format!(
                "failed to create temporary marketplace clone directory in {}: {err}",
                parent.display()
            )
        })
}

pub(super) fn remove_stale_marketplace_provider_temp_dirs(parent: &Path) {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(err) => {
            warn!(
                error = %err,
                parent = %parent.display(),
                "failed to list marketplace cache parent for stale cleanup"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let is_temp_dir = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("marketplace-clone-"));
        if !is_temp_dir {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified
            .elapsed()
            .is_ok_and(|age| age >= MARKETPLACE_PROVIDER_STALE_TEMP_DIR_MAX_AGE)
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub(super) fn clone_git_marketplace_repo(
    repo: &str,
    reference: Option<&str>,
    destination: &Path,
    marketplace_name: &str,
) -> Result<(), String> {
    let repo_url = normalize_git_marketplace_repo(repo);
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("clone")
        .arg("--depth")
        .arg("1");
    if let Some(reference) = reference.filter(|reference| !reference.trim().is_empty()) {
        command.arg("--branch").arg(reference);
    }
    command.arg(repo_url).arg(destination);

    let output = run_git_command_with_timeout(
        &mut command,
        &format!("git clone plugin marketplace `{marketplace_name}`"),
        MARKETPLACE_PROVIDER_GIT_TIMEOUT,
    )?;
    ensure_git_success(&output, "git clone plugin marketplace")
}

pub(super) fn normalize_git_marketplace_repo(repo: &str) -> String {
    let trimmed = repo.trim();
    if trimmed.contains("://") || trimmed.starts_with("git@") || trimmed.ends_with(".git") {
        return trimmed.to_string();
    }
    let mut parts = trimmed.split('/');
    if let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next())
        && !owner.is_empty()
        && !name.is_empty()
    {
        return format!("https://github.com/{owner}/{name}.git");
    }
    trimmed.to_string()
}

pub(super) fn ensure_marketplace_provider_manifest_exists(
    repo_path: &Path,
    relative_path: Option<&Path>,
) -> Result<(), String> {
    let root = relative_path
        .map(|path| repo_path.join(path))
        .unwrap_or_else(|| repo_path.to_path_buf());
    let manifest = marketplace_manifest_path(&root);
    if manifest.is_file() {
        return Ok(());
    }
    Err(format!(
        "plugin marketplace repo missing manifest at {}",
        manifest.display()
    ))
}

pub(super) fn activate_marketplace_provider_repo(
    repo_path: &Path,
    staged_repo_dir: TempDir,
) -> Result<(), String> {
    let staged_repo_path = staged_repo_dir.path();
    if repo_path.exists() {
        let parent = repo_path.parent().ok_or_else(|| {
            format!(
                "failed to determine marketplace cache parent directory for {}",
                repo_path.display()
            )
        })?;
        let backup_dir = tempfile::Builder::new()
            .prefix("marketplace-backup-")
            .tempdir_in(parent)
            .map_err(|err| {
                format!(
                    "failed to create marketplace cache backup directory in {}: {err}",
                    parent.display()
                )
            })?;
        let backup_repo_path = backup_dir.path().join("repo");
        fs::rename(repo_path, &backup_repo_path).map_err(|err| {
            format!(
                "failed to move previous marketplace cache out of the way at {}: {err}",
                repo_path.display()
            )
        })?;
        if let Err(err) = fs::rename(staged_repo_path, repo_path) {
            let rollback_result = fs::rename(&backup_repo_path, repo_path);
            return match rollback_result {
                Ok(()) => Err(format!(
                    "failed to activate new marketplace cache at {}: {err}",
                    repo_path.display()
                )),
                Err(rollback_err) => {
                    let backup_path = backup_dir.keep().join("repo");
                    Err(format!(
                        "failed to activate new marketplace cache at {}: {err}; failed to restore previous cache (left at {}): {rollback_err}",
                        repo_path.display(),
                        backup_path.display()
                    ))
                }
            };
        }
    } else {
        fs::rename(staged_repo_path, repo_path).map_err(|err| {
            format!(
                "failed to activate marketplace cache at {}: {err}",
                repo_path.display()
            )
        })?;
    }
    Ok(())
}

pub(super) fn git_head_sha(repo_path: &Path, git_binary: &str) -> Result<String, String> {
    let output = Command::new(git_binary)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|err| {
            format!(
                "failed to run git rev-parse HEAD in {}: {err}",
                repo_path.display()
            )
        })?;
    ensure_git_success(&output, "git rev-parse HEAD")?;

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "git rev-parse HEAD returned empty output in {}",
            repo_path.display()
        ));
    }
    Ok(sha)
}

pub(super) fn run_git_command_with_timeout(
    command: &mut Command,
    context: &str,
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run {context}: {err}"))?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("failed to wait for {context}: {err}"));
            }
            Ok(None) => {}
            Err(err) => return Err(format!("failed to poll {context}: {err}")),
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|err| format!("failed to wait for {context} after timeout: {err}"))?;
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return if stderr.is_empty() {
                Err(format!("{context} timed out after {}s", timeout.as_secs()))
            } else {
                Err(format!(
                    "{context} timed out after {}s: {stderr}",
                    timeout.as_secs()
                ))
            };
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn ensure_git_success(output: &Output, context: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("{context} failed with status {}", output.status))
    } else {
        Err(format!(
            "{context} failed with status {}: {stderr}",
            output.status
        ))
    }
}
