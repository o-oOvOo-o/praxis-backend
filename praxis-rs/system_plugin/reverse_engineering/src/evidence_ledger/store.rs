use crate::ReverseError;
use crate::evidence_ledger::EvidenceRecord;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const LEDGER_FILE: &str = "ledger.jsonl";

pub fn append(root: &Path, record: &EvidenceRecord) -> Result<EvidenceRecord, ReverseError> {
    let path = ledger_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| ReverseError::io(parent, err))?;
    }
    let mut record = record.clone();
    record.prev_hash = last_record_hash(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| ReverseError::io(&path, err))?;
    serde_json::to_writer(&mut file, &record).map_err(|err| ReverseError::json(&path, err))?;
    file.write_all(b"\n")
        .map_err(|err| ReverseError::io(&path, err))?;
    Ok(record)
}

fn last_record_hash(path: &Path) -> Result<String, ReverseError> {
    if !path.is_file() {
        return Ok(String::new());
    }
    let last = read_last_nonempty_line(path)?;
    if last.is_empty() {
        return Ok(String::new());
    }
    Ok(crate::hash_util::sha256_hex(&[last.as_bytes()]))
}

fn read_last_nonempty_line(path: &Path) -> Result<String, ReverseError> {
    let mut file = std::fs::File::open(path).map_err(|err| ReverseError::io(path, err))?;
    let mut pos = file
        .metadata()
        .map_err(|err| ReverseError::io(path, err))?
        .len();
    let mut reversed = Vec::new();
    let mut saw_content = false;
    while pos > 0 {
        pos -= 1;
        file.seek(SeekFrom::Start(pos))
            .map_err(|err| ReverseError::io(path, err))?;
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte)
            .map_err(|err| ReverseError::io(path, err))?;
        match byte[0] {
            b'\n' | b'\r' if saw_content => break,
            b'\n' | b'\r' => {}
            value => {
                saw_content = true;
                reversed.push(value);
            }
        }
    }
    reversed.reverse();
    String::from_utf8(reversed).map_err(|err| ReverseError::Codec(err.to_string()))
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_FILE)
}
