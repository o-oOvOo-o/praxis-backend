use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::agent_os::model::DirtyFileFingerprint;
use crate::path_scope::normalize_path_for_scope;

pub(in crate::agent_os) fn dirty_file_fingerprints(
    cwd: &Path,
    dirty_files: &[PathBuf],
) -> HashMap<String, DirtyFileFingerprint> {
    dirty_files
        .iter()
        .map(|path| {
            (
                normalize_path_for_scope(path),
                dirty_file_fingerprint(cwd, path),
            )
        })
        .collect()
}

pub(in crate::agent_os) fn dirty_file_fingerprint(cwd: &Path, path: &Path) -> DirtyFileFingerprint {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return DirtyFileFingerprint {
            exists: false,
            len: None,
            modified_unix_millis: None,
        };
    };
    let modified_unix_millis = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i128);
    DirtyFileFingerprint {
        exists: true,
        len: Some(metadata.len()),
        modified_unix_millis,
    }
}
