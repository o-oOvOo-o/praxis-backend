use crate::ReverseError;
use crate::authorization::AuthorizationScope;
use crate::hash_util::sha256_reader_hex;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct TargetFingerprint {
    pub target_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub target_kind_hint: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ArtifactIngest {
    pub artifact_id: String,
    pub source_path: PathBuf,
    pub artifact_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
}

pub fn fingerprint_path(path: &Path) -> Result<TargetFingerprint, ReverseError> {
    let metadata = std::fs::metadata(path).map_err(|err| ReverseError::io(path, err))?;
    let sha256 = sha256_file(path)?;
    Ok(TargetFingerprint {
        target_path: path.to_path_buf(),
        size_bytes: metadata.len(),
        sha256,
        target_kind_hint: target_kind_hint(path),
    })
}

pub fn ingest(
    scope: &AuthorizationScope,
    source_path: &Path,
) -> Result<ArtifactIngest, ReverseError> {
    let fingerprint = fingerprint_path(source_path)?;
    if fingerprint.sha256 != scope.target_hash {
        return Err(ReverseError::Authorization(format!(
            "target hash mismatch for scope {}; expected {}, got {}",
            scope.scope_id, scope.target_hash, fingerprint.sha256
        )));
    }
    let artifact_id = format!("art_{}", &fingerprint.sha256[..16]);
    let artifact_dir = scope.artifact_root.join("artifacts").join(&artifact_id);
    std::fs::create_dir_all(&artifact_dir).map_err(|err| ReverseError::io(&artifact_dir, err))?;
    let file_name = source_path
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("artifact.bin"));
    let artifact_path = artifact_dir.join(file_name);
    std::fs::copy(source_path, &artifact_path)
        .map_err(|err| ReverseError::io(&artifact_path, err))?;
    Ok(ArtifactIngest {
        artifact_id,
        source_path: source_path.to_path_buf(),
        artifact_path,
        size_bytes: fingerprint.size_bytes,
        sha256: fingerprint.sha256,
    })
}

fn sha256_file(path: &Path) -> Result<String, ReverseError> {
    let file = File::open(path).map_err(|err| ReverseError::io(path, err))?;
    sha256_reader_hex(file).map_err(|err| ReverseError::io(path, err))
}

fn target_kind_hint(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "dll" | "exe" | "so" | "dylib" => "native_or_managed_binary",
        "jar" | "class" => "managed_jvm",
        "spv" | "dxil" | "cso" | "hlsl" | "glsl" | "wgsl" => "shader",
        "asset" | "bundle" | "unity3d" => "unity_asset",
        _ => "unknown",
    }
    .to_string()
}
