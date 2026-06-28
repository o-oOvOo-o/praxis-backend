#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::fs_navigation::DirectoryEntryFilter;
use crate::tools::fs_navigation::ListDirectoryRequest;
use crate::tools::fs_navigation::list_directory;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct ListDirectoryHandler;

const DEFAULT_MAX_SCAN_ENTRIES: usize = 2_000;

fn default_offset() -> usize {
    1
}

fn default_limit() -> usize {
    25
}

fn default_depth() -> usize {
    2
}

fn default_max_scan_entries() -> usize {
    DEFAULT_MAX_SCAN_ENTRIES
}

fn default_respect_ignore() -> bool {
    true
}

#[derive(Deserialize)]
struct ListDirectoryArgs {
    path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_depth")]
    depth: usize,
    #[serde(default = "default_max_scan_entries")]
    max_entries: usize,
    #[serde(default = "default_respect_ignore")]
    respect_ignore: bool,
    #[serde(default)]
    include_hidden: bool,
    #[serde(default)]
    kind: DirectoryEntryFilter,
}

#[async_trait]
impl ToolHandler for ListDirectoryHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, turn, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "list_directory handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: ListDirectoryArgs = parse_arguments(&arguments)?;
        if args.path.trim().is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "path must not be empty".to_string(),
            ));
        }

        let path = crate::util::resolve_path(turn.cwd.as_path(), &PathBuf::from(args.path));
        let output = list_directory(ListDirectoryRequest {
            path: path.clone(),
            offset: args.offset,
            limit: args.limit,
            depth: args.depth,
            max_entries: args.max_entries,
            respect_ignore: args.respect_ignore,
            include_hidden: args.include_hidden,
            kind: args.kind,
        })
        .await?;

        let mut lines = Vec::with_capacity(output.entries.len() + 2);
        lines.push(format!("Absolute path: {}", path.display()));
        lines.push(output.summary);
        lines.extend(output.entries);
        Ok(FunctionToolOutput::from_text(lines.join("\n"), Some(true)))
    }
}

#[cfg(test)]
async fn list_directory_slice(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
) -> Result<Vec<String>, FunctionCallError> {
    list_directory_slice_with_options(
        path,
        offset,
        limit,
        depth,
        DirectoryEntryFilter::All,
        /*respect_ignore*/ false,
        /*include_hidden*/ true,
    )
    .await
}

#[cfg(test)]
async fn list_directory_slice_with_options(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
    kind: DirectoryEntryFilter,
    respect_ignore: bool,
    include_hidden: bool,
) -> Result<Vec<String>, FunctionCallError> {
    crate::tools::fs_navigation::list_directory_entries_for_test(ListDirectoryRequest {
        path: path.to_path_buf(),
        offset,
        limit,
        depth,
        max_entries: DEFAULT_MAX_SCAN_ENTRIES,
        respect_ignore,
        include_hidden,
        kind,
    })
    .await
}

#[cfg(test)]
#[path = "list_directory_tests.rs"]
mod tests;
