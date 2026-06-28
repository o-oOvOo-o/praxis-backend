use crate::ReverseError;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const SAFETY_EVAL_FILE: &str = "safety_eval.jsonl";

pub fn append<T>(root: &Path, event: &T) -> Result<(), ReverseError>
where
    T: Serialize,
{
    let path = safety_eval_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| ReverseError::io(parent, err))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| ReverseError::io(&path, err))?;
    serde_json::to_writer(&mut file, event).map_err(|err| ReverseError::json(&path, err))?;
    file.write_all(b"\n")
        .map_err(|err| ReverseError::io(&path, err))
}

fn safety_eval_path(root: &Path) -> PathBuf {
    root.join(SAFETY_EVAL_FILE)
}
