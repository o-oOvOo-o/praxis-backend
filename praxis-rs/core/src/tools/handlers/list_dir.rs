use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::FileType;
use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use praxis_utils_string::take_bytes_at_char_boundary;
use serde::Deserialize;
use tokio::fs;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct ListDirHandler;

const MAX_ENTRY_LENGTH: usize = 500;
const INDENTATION_SPACES: usize = 2;
const HARD_MAX_SCANNED_ENTRIES: usize = 20_000;
const DEFAULT_MAX_SCAN_ENTRIES: usize = 2_000;
const DEFAULT_MAX_DEPTH: usize = 6;
const DEFAULT_IGNORED_RECURSIVE_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    ".next",
    ".nuxt",
    "dist",
    "build",
    "vendor",
    "__pycache__",
];

#[derive(Clone, Debug)]
struct DirectoryScanPolicy {
    max_depth: usize,
    ignored_recursive_dirs: Vec<String>,
}

impl DirectoryScanPolicy {
    fn from_env() -> Self {
        Self {
            max_depth: read_usize_env("PRAXIS_LIST_DIR_MAX_DEPTH", DEFAULT_MAX_DEPTH, 16),
            ignored_recursive_dirs: std::env::var("PRAXIS_LIST_DIR_SKIP_DIRS")
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|dirs| !dirs.is_empty())
                .unwrap_or_else(|| {
                    DEFAULT_IGNORED_RECURSIVE_DIRS
                        .iter()
                        .map(|value| value.to_string())
                        .collect()
                }),
        }
    }
}

fn read_usize_env(name: &str, default_value: usize, hard_max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(hard_max))
        .unwrap_or(default_value)
}

fn default_offset() -> usize {
    1
}

fn default_limit() -> usize {
    25
}

fn default_depth() -> usize {
    2
}

#[derive(Deserialize)]
struct ListDirArgs {
    dir_path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_depth")]
    depth: usize,
    /// Hard cap for entries scanned before pagination. This keeps a single
    /// list_dir call from walking giant target/node_modules/vendor trees.
    #[serde(default = "default_max_scan_entries")]
    max_entries: usize,
}

fn default_max_scan_entries() -> usize {
    DEFAULT_MAX_SCAN_ENTRIES
}

#[async_trait]
impl ToolHandler for ListDirHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "list_dir handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: ListDirArgs = parse_arguments(&arguments)?;

        let ListDirArgs {
            dir_path,
            offset,
            limit,
            depth,
            max_entries,
        } = args;

        if offset == 0 {
            return Err(FunctionCallError::RespondToModel(
                "offset must be a 1-indexed entry number".to_string(),
            ));
        }

        if limit == 0 {
            return Err(FunctionCallError::RespondToModel(
                "limit must be greater than zero".to_string(),
            ));
        }

        if depth == 0 {
            return Err(FunctionCallError::RespondToModel(
                "depth must be greater than zero".to_string(),
            ));
        }

        let scan_policy = DirectoryScanPolicy::from_env();
        if depth > scan_policy.max_depth {
            return Err(FunctionCallError::RespondToModel(format!(
                "depth must be at most {}; use a narrower path instead",
                scan_policy.max_depth
            )));
        }

        if max_entries == 0 {
            return Err(FunctionCallError::RespondToModel(
                "max_entries must be greater than zero".to_string(),
            ));
        }

        let path = PathBuf::from(&dir_path);
        if !path.is_absolute() {
            return Err(FunctionCallError::RespondToModel(
                "dir_path must be an absolute path".to_string(),
            ));
        }

        let entries =
            list_dir_slice_with_policy(&path, offset, limit, depth, max_entries, scan_policy)
                .await?;
        let mut output = Vec::with_capacity(entries.len() + 1);
        output.push(format!("Absolute path: {}", path.display()));
        output.extend(entries);
        Ok(FunctionToolOutput::from_text(output.join("\n"), Some(true)))
    }
}

async fn list_dir_slice(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
) -> Result<Vec<String>, FunctionCallError> {
    list_dir_slice_with_policy(
        path,
        offset,
        limit,
        depth,
        DEFAULT_MAX_SCAN_ENTRIES,
        DirectoryScanPolicy::from_env(),
    )
    .await
}

async fn list_dir_slice_with_policy(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
    max_scanned_entries: usize,
    scan_policy: DirectoryScanPolicy,
) -> Result<Vec<String>, FunctionCallError> {
    let max_scanned_entries = max_scanned_entries.clamp(limit.max(1), HARD_MAX_SCANNED_ENTRIES);
    let mut scan = collect_entries(
        path,
        Path::new(""),
        depth,
        max_scanned_entries,
        &scan_policy,
    )
    .await?;

    if scan.entries.is_empty() {
        if scan.truncated {
            return Ok(vec![format!(
                "Directory scan stopped after {max_scanned_entries} entries before any displayable entries were collected. Narrow dir_path/depth or raise max_scanned_entries."
            )]);
        }
        return Ok(Vec::new());
    }

    scan.entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let start_index = offset - 1;
    if start_index >= scan.entries.len() {
        if scan.truncated {
            return Ok(vec![format!(
                "offset exceeds the {count} entries scanned before the safety cap ({cap}) was reached; narrow dir_path/depth or raise max_scanned_entries.",
                count = scan.entries.len(),
                cap = max_scanned_entries,
            )]);
        }
        return Err(FunctionCallError::RespondToModel(
            "offset exceeds directory entry count".to_string(),
        ));
    }

    let remaining_entries = scan.entries.len() - start_index;
    let capped_limit = limit.min(remaining_entries);
    let end_index = start_index + capped_limit;
    let selected_entries = &scan.entries[start_index..end_index];
    let mut formatted = Vec::with_capacity(selected_entries.len() + 2);

    if scan.truncated {
        formatted.push(format!(
            "Directory scan truncated after {scanned} entries; results are a capped prefix, not a complete directory listing.",
            scanned = scan.scanned_entries,
        ));
    }
    if scan.skipped_recursive_dirs > 0 {
        formatted.push(format!(
            "Skipped recursion into {count} large/generated directories ({dirs}).",
            count = scan.skipped_recursive_dirs,
            dirs = scan_policy.ignored_recursive_dirs.join(", "),
        ));
    }

    for entry in selected_entries {
        formatted.push(format_entry_line(entry));
    }

    if end_index < scan.entries.len() {
        formatted.push(format!("More than {capped_limit} entries found"));
    } else if scan.truncated {
        formatted.push(format!(
            "Directory scan stopped after {max_scanned_entries} entries; use a narrower dir_path/depth or a larger max_entries"
        ));
    }

    Ok(formatted)
}

struct DirScan {
    entries: Vec<DirEntry>,
    scanned_entries: usize,
    skipped_recursive_dirs: usize,
    truncated: bool,
}

async fn collect_entries(
    dir_path: &Path,
    relative_prefix: &Path,
    depth: usize,
    max_scanned_entries: usize,
    scan_policy: &DirectoryScanPolicy,
) -> Result<DirScan, FunctionCallError> {
    let mut queue = VecDeque::new();
    let mut entries = Vec::new();
    let mut scanned_entries = 0usize;
    let mut skipped_recursive_dirs = 0usize;
    let mut truncated = false;
    queue.push_back((dir_path.to_path_buf(), relative_prefix.to_path_buf(), depth));

    while let Some((current_dir, prefix, remaining_depth)) = queue.pop_front() {
        if scanned_entries >= max_scanned_entries {
            truncated = true;
            break;
        }

        let mut read_dir = fs::read_dir(&current_dir).await.map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to read directory: {err}"))
        })?;

        let mut dir_entries = Vec::new();

        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to read directory: {err}"))
        })? {
            if scanned_entries >= max_scanned_entries {
                truncated = true;
                break;
            }

            scanned_entries += 1;
            let file_type = entry.file_type().await.map_err(|err| {
                FunctionCallError::RespondToModel(format!("failed to inspect entry: {err}"))
            })?;

            let file_name = entry.file_name();
            let relative_path = if prefix.as_os_str().is_empty() {
                PathBuf::from(&file_name)
            } else {
                prefix.join(&file_name)
            };

            let display_name = format_entry_component(&file_name);
            let display_depth = prefix.components().count();
            let sort_key = format_entry_name(&relative_path);
            let kind = DirEntryKind::from(&file_type);
            dir_entries.push((
                entry.path(),
                relative_path,
                file_name,
                kind,
                DirEntry {
                    name: sort_key,
                    display_name,
                    depth: display_depth,
                    kind,
                },
            ));
        }

        dir_entries.sort_unstable_by(|a, b| a.4.name.cmp(&b.4.name));

        for (entry_path, relative_path, file_name, kind, dir_entry) in dir_entries {
            if kind == DirEntryKind::Directory && remaining_depth > 1 {
                if should_skip_recursive_dir(&file_name, scan_policy) {
                    skipped_recursive_dirs += 1;
                } else {
                    queue.push_back((entry_path, relative_path, remaining_depth - 1));
                }
            }
            entries.push(dir_entry);
        }
    }

    Ok(DirScan {
        entries,
        scanned_entries,
        skipped_recursive_dirs,
        truncated,
    })
}

fn should_skip_recursive_dir(name: &OsStr, scan_policy: &DirectoryScanPolicy) -> bool {
    let name = name.to_string_lossy();
    scan_policy
        .ignored_recursive_dirs
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

fn format_entry_name(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace("\\", "/");
    if normalized.len() > MAX_ENTRY_LENGTH {
        take_bytes_at_char_boundary(&normalized, MAX_ENTRY_LENGTH).to_string()
    } else {
        normalized
    }
}

fn format_entry_component(name: &OsStr) -> String {
    let normalized = name.to_string_lossy();
    if normalized.len() > MAX_ENTRY_LENGTH {
        take_bytes_at_char_boundary(&normalized, MAX_ENTRY_LENGTH).to_string()
    } else {
        normalized.to_string()
    }
}

fn format_entry_line(entry: &DirEntry) -> String {
    let indent = " ".repeat(entry.depth * INDENTATION_SPACES);
    let mut name = entry.display_name.clone();
    match entry.kind {
        DirEntryKind::Directory => name.push('/'),
        DirEntryKind::Symlink => name.push('@'),
        DirEntryKind::Other => name.push('?'),
        DirEntryKind::File => {}
    }
    format!("{indent}{name}")
}

#[derive(Clone)]
struct DirEntry {
    name: String,
    display_name: String,
    depth: usize,
    kind: DirEntryKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DirEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

impl From<&FileType> for DirEntryKind {
    fn from(file_type: &FileType) -> Self {
        if file_type.is_symlink() {
            DirEntryKind::Symlink
        } else if file_type.is_dir() {
            DirEntryKind::Directory
        } else if file_type.is_file() {
            DirEntryKind::File
        } else {
            DirEntryKind::Other
        }
    }
}

#[cfg(test)]
#[path = "list_dir_tests.rs"]
mod tests;
