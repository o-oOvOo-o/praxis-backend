use crate::exec::ExecStreamSpool;
use serde_json::json;
use std::path::PathBuf;

pub(super) fn sanitize_artifact_extension(extension: &str) -> String {
    let extension = extension
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if extension.is_empty() {
        "bin".to_string()
    } else {
        extension
    }
}

pub(super) fn metadata_with_blob(
    metadata: serde_json::Value,
    blob_bytes: usize,
    blob_path: Option<&PathBuf>,
) -> serde_json::Value {
    let blob_metadata = json!({
        "blob_bytes": blob_bytes,
        "blob_path": blob_path.map(|path| path.display().to_string()),
        "blob_persisted": blob_path.is_some(),
    });
    match metadata {
        serde_json::Value::Object(mut object) => {
            object.insert("blob".to_string(), blob_metadata);
            serde_json::Value::Object(object)
        }
        value => json!({
            "metadata": value,
            "blob": blob_metadata,
        }),
    }
}

pub(super) async fn append_spool_stream(
    out: &mut tokio::fs::File,
    stream: &ExecStreamSpool,
) -> std::io::Result<()> {
    let mut input = tokio::fs::File::open(stream.path.as_path()).await?;
    tokio::io::copy(&mut input, out).await?;
    Ok(())
}
