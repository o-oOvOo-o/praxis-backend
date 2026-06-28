use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use ignore::WalkBuilder;
use praxis_utils_string::take_bytes_at_char_boundary;
use serde::Deserialize;
use tokio::task;

use crate::function_tool::FunctionCallError;

const MAX_ENTRY_LENGTH: usize = 500;
const INDENTATION_SPACES: usize = 2;
const HARD_MAX_SCANNED_ENTRIES: usize = 20_000;
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DirectoryEntryFilter {
    All,
    Directories,
    Files,
}

impl Default for DirectoryEntryFilter {
    fn default() -> Self {
        Self::All
    }
}

impl DirectoryEntryFilter {
    fn matches(self, kind: DirEntryKind) -> bool {
        match self {
            Self::All => true,
            Self::Directories => kind == DirEntryKind::Directory,
            Self::Files => kind == DirEntryKind::File,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ListDirectoryRequest {
    pub(crate) path: PathBuf,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) depth: usize,
    pub(crate) max_entries: usize,
    pub(crate) respect_ignore: bool,
    pub(crate) include_hidden: bool,
    pub(crate) kind: DirectoryEntryFilter,
}

#[derive(Debug)]
pub(crate) struct ListDirectoryOutput {
    pub(crate) summary: String,
    pub(crate) entries: Vec<String>,
}

#[derive(Clone, Debug)]
struct DirectoryScanPolicy {
    max_depth: usize,
    threads: usize,
    ignored_recursive_dirs: Vec<String>,
}

impl DirectoryScanPolicy {
    fn from_env() -> Self {
        Self {
            max_depth: read_usize_env("PRAXIS_LIST_DIRECTORY_MAX_DEPTH", DEFAULT_MAX_DEPTH, 16),
            threads: read_usize_env("PRAXIS_LIST_DIRECTORY_THREADS", 0, 64),
            ignored_recursive_dirs: std::env::var("PRAXIS_LIST_DIRECTORY_SKIP_DIRS")
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
        .filter(|value| *value > 0 || default_value == 0)
        .map(|value| value.min(hard_max))
        .unwrap_or(default_value)
}

pub(crate) async fn list_directory(
    request: ListDirectoryRequest,
) -> Result<ListDirectoryOutput, FunctionCallError> {
    validate_request(&request).await?;
    let policy = DirectoryScanPolicy::from_env();
    if request.depth > policy.max_depth {
        return Err(FunctionCallError::RespondToModel(format!(
            "depth must be at most {}; use a narrower path instead",
            policy.max_depth
        )));
    }

    task::spawn_blocking(move || list_directory_blocking(request, policy))
        .await
        .map_err(|err| FunctionCallError::RespondToModel(format!("directory scan failed: {err}")))?
}

async fn validate_request(request: &ListDirectoryRequest) -> Result<(), FunctionCallError> {
    if request.offset == 0 {
        return Err(FunctionCallError::RespondToModel(
            "offset must be a 1-indexed entry number".to_string(),
        ));
    }
    if request.limit == 0 {
        return Err(FunctionCallError::RespondToModel(
            "limit must be greater than zero".to_string(),
        ));
    }
    if request.depth == 0 {
        return Err(FunctionCallError::RespondToModel(
            "depth must be greater than zero".to_string(),
        ));
    }
    if request.max_entries == 0 {
        return Err(FunctionCallError::RespondToModel(
            "max_entries must be greater than zero".to_string(),
        ));
    }
    if !request.path.is_absolute() {
        return Err(FunctionCallError::RespondToModel(
            "path must resolve to an absolute directory path".to_string(),
        ));
    }
    let metadata = tokio::fs::metadata(&request.path).await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to inspect directory: {err}"))
    })?;
    if !metadata.is_dir() {
        return Err(FunctionCallError::RespondToModel(
            "path must be a directory".to_string(),
        ));
    }
    Ok(())
}

fn list_directory_blocking(
    request: ListDirectoryRequest,
    policy: DirectoryScanPolicy,
) -> Result<ListDirectoryOutput, FunctionCallError> {
    let max_scanned_entries = request
        .max_entries
        .clamp(request.limit.max(1), HARD_MAX_SCANNED_ENTRIES);
    let mut scan = collect_entries(&request, &policy, max_scanned_entries);

    if scan.entries.is_empty() {
        if scan.truncated {
            return Ok(ListDirectoryOutput {
                summary: format_summary(&request, &scan, max_scanned_entries),
                entries: vec![format!(
                    "Directory scan stopped after {max_scanned_entries} entries before any displayable entries were collected. Narrow path/depth or raise max_entries."
                )],
            });
        }
        return Ok(ListDirectoryOutput {
            summary: format_summary(&request, &scan, max_scanned_entries),
            entries: Vec::new(),
        });
    }

    scan.entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let start_index = request.offset - 1;
    if start_index >= scan.entries.len() {
        if scan.truncated {
            return Ok(ListDirectoryOutput {
                summary: format_summary(&request, &scan, max_scanned_entries),
                entries: vec![format!(
                    "offset exceeds the {count} entries scanned before the safety cap ({cap}) was reached; narrow path/depth or raise max_entries.",
                    count = scan.entries.len(),
                    cap = max_scanned_entries,
                )],
            });
        }
        return Err(FunctionCallError::RespondToModel(
            "offset exceeds directory entry count".to_string(),
        ));
    }

    let remaining_entries = scan.entries.len() - start_index;
    let capped_limit = request.limit.min(remaining_entries);
    let end_index = start_index + capped_limit;
    let selected_entries = &scan.entries[start_index..end_index];
    let mut formatted = Vec::with_capacity(selected_entries.len() + 3);

    if scan.truncated {
        formatted.push(format!(
            "Directory scan truncated after {scanned} entries; results are a capped prefix, not a complete directory listing.",
            scanned = scan.scanned_entries,
        ));
    }
    if scan.pruned_dirs > 0 {
        formatted.push(format!(
            "Skipped recursion into {count} large/generated directories ({dirs}).",
            count = scan.pruned_dirs,
            dirs = policy.ignored_recursive_dirs.join(", "),
        ));
    }

    for entry in selected_entries {
        formatted.push(format_entry_line(entry));
    }

    if end_index < scan.entries.len() {
        formatted.push(format!("More than {capped_limit} entries found"));
    } else if scan.truncated {
        formatted.push(format!(
            "Directory scan stopped after {max_scanned_entries} entries; use a narrower path/depth or a larger max_entries"
        ));
    }

    Ok(ListDirectoryOutput {
        summary: format_summary(&request, &scan, max_scanned_entries),
        entries: formatted,
    })
}

fn collect_entries(
    request: &ListDirectoryRequest,
    policy: &DirectoryScanPolicy,
    max_scanned_entries: usize,
) -> DirScan {
    let entries = Arc::new(Mutex::new(Vec::new()));
    let scanned_entries = Arc::new(AtomicUsize::new(0));
    let pruned_dirs = Arc::new(AtomicUsize::new(0));
    let truncated = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(AtomicUsize::new(0));
    let root_path = Arc::new(request.path.clone());

    let mut walk_builder = WalkBuilder::new(&request.path);
    let threads = if request.depth <= 2 {
        1
    } else {
        policy.threads
    };
    walk_builder
        .max_depth(Some(request.depth))
        .threads(threads)
        .hidden(!request.include_hidden)
        .follow_links(false)
        .parents(false);
    if request.respect_ignore {
        walk_builder.require_git(true);
    } else {
        walk_builder
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false);
    }

    let skipped_names = policy.ignored_recursive_dirs.clone();
    let pruned_for_filter = pruned_dirs.clone();
    walk_builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        if !is_directory_entry(entry) {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        if skipped_names
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored))
        {
            pruned_for_filter.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    });

    let walker = walk_builder.build_parallel();
    let kind_filter = request.kind;
    walker.run(|| {
        let entries = entries.clone();
        let scanned_entries = scanned_entries.clone();
        let truncated = truncated.clone();
        let errors = errors.clone();
        let root_path = root_path.clone();

        Box::new(move |result| {
            if truncated.load(Ordering::Relaxed) {
                return ignore::WalkState::Quit;
            }
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    return ignore::WalkState::Continue;
                }
            };
            if entry.depth() == 0 {
                return ignore::WalkState::Continue;
            }

            let scanned = scanned_entries.fetch_add(1, Ordering::Relaxed) + 1;
            if scanned > max_scanned_entries {
                truncated.store(true, Ordering::Relaxed);
                return ignore::WalkState::Quit;
            }

            let kind = DirEntryKind::from_ignore_entry(&entry);
            if !kind_filter.matches(kind) {
                return ignore::WalkState::Continue;
            }

            let Ok(relative_path) = entry.path().strip_prefix(root_path.as_ref()) else {
                return ignore::WalkState::Continue;
            };
            let display_name = format_entry_component(entry.file_name());
            let display_depth = entry.depth().saturating_sub(1);
            let sort_key = format_entry_name(relative_path);

            if let Ok(mut entries) = entries.lock() {
                entries.push(DirEntry {
                    name: sort_key,
                    display_name,
                    depth: display_depth,
                    kind,
                });
            }

            ignore::WalkState::Continue
        })
    });

    let mut entries = match Arc::try_unwrap(entries) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(entries) => entries
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default(),
    };
    entries.shrink_to_fit();
    DirScan {
        entries,
        scanned_entries: scanned_entries
            .load(Ordering::Relaxed)
            .min(max_scanned_entries),
        pruned_dirs: pruned_dirs.load(Ordering::Relaxed),
        errors: errors.load(Ordering::Relaxed),
        truncated: truncated.load(Ordering::Relaxed),
        strategy: if request.depth <= 2 {
            ScanStrategy::SingleThread
        } else {
            ScanStrategy::Parallel
        },
    }
}

fn is_directory_entry(entry: &ignore::DirEntry) -> bool {
    entry
        .file_type()
        .map(|file_type| file_type.is_dir())
        .unwrap_or(false)
}

fn format_summary(
    request: &ListDirectoryRequest,
    scan: &DirScan,
    max_scanned_entries: usize,
) -> String {
    format!(
        "Scan: strategy={strategy}, scanned={scanned}, displayed={displayed}, cap={cap}, depth={depth}, ignore={ignore}, hidden={hidden}, kind={kind}, pruned={pruned}, errors={errors}, truncated={truncated}",
        strategy = scan.strategy.as_str(),
        scanned = scan.scanned_entries,
        displayed = scan.entries.len(),
        cap = max_scanned_entries,
        depth = request.depth,
        ignore = if request.respect_ignore { "on" } else { "off" },
        hidden = if request.include_hidden {
            "shown"
        } else {
            "hidden"
        },
        kind = request.kind.as_str(),
        pruned = scan.pruned_dirs,
        errors = scan.errors,
        truncated = scan.truncated,
    )
}

impl DirectoryEntryFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Directories => "directories",
            Self::Files => "files",
        }
    }
}

#[derive(Debug)]
struct DirScan {
    entries: Vec<DirEntry>,
    scanned_entries: usize,
    pruned_dirs: usize,
    errors: usize,
    truncated: bool,
    strategy: ScanStrategy,
}

#[derive(Clone, Copy, Debug)]
enum ScanStrategy {
    SingleThread,
    Parallel,
}

impl ScanStrategy {
    fn as_str(self) -> &'static str {
        match self {
            Self::SingleThread => "single-thread-walk",
            Self::Parallel => "parallel-walk",
        }
    }
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

#[derive(Clone, Debug)]
struct DirEntry {
    name: String,
    display_name: String,
    depth: usize,
    kind: DirEntryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

impl DirEntryKind {
    fn from_ignore_entry(entry: &ignore::DirEntry) -> Self {
        let Some(file_type) = entry.file_type() else {
            return Self::Other;
        };
        if file_type.is_symlink() {
            Self::Symlink
        } else if file_type.is_dir() {
            Self::Directory
        } else if file_type.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
pub(crate) async fn list_directory_entries_for_test(
    request: ListDirectoryRequest,
) -> Result<Vec<String>, FunctionCallError> {
    list_directory(request).await.map(|output| output.entries)
}
