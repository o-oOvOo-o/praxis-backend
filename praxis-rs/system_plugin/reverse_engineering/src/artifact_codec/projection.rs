use crate::ReverseError;
use crate::artifact_codec::redaction::safe_text_projection;
use crate::artifact_store::ArtifactIngest;
use crate::artifact_store::fingerprint_path;
use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct Projection {
    pub artifact_id: String,
    pub target_hash: String,
    pub target_kind: String,
    pub analyzer: String,
    pub summary: String,
    pub symbols: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub callgraph: CallgraphSummary,
    pub cfg: CfgSummary,
    pub family_label: Option<String>,
    pub metrics: serde_json::Value,
    pub clean_room_snippet: Option<String>,
    pub artifact_path: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct CallgraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub top_hubs: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct CfgSummary {
    pub node_count: usize,
    pub branch_density: f32,
    pub loop_count: usize,
}

pub fn from_ingest(ingest: &ArtifactIngest) -> Result<Projection, ReverseError> {
    assert_no_raw_exposure(&ingest.sha256, ingest)?;
    Ok(Projection {
        artifact_id: ingest.artifact_id.clone(),
        target_hash: ingest.sha256.clone(),
        target_kind: "stored_artifact".to_string(),
        analyzer: "praxis-artifact-ingest".to_string(),
        summary: format!(
            "Stored local artifact {} ({} bytes); raw content remains local.",
            ingest.artifact_id, ingest.size_bytes
        ),
        symbols: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        callgraph: CallgraphSummary {
            node_count: 0,
            edge_count: 0,
            top_hubs: Vec::new(),
        },
        cfg: CfgSummary {
            node_count: 0,
            branch_density: 0.0,
            loop_count: 0,
        },
        family_label: None,
        metrics: serde_json::json!({
            "size_bytes": ingest.size_bytes,
            "raw_exposed": false
        }),
        clean_room_snippet: None,
        artifact_path: format!("artifact://{}", ingest.artifact_id),
    })
}

pub fn summarize_local_artifact(path: &Path) -> Result<Projection, ReverseError> {
    let fingerprint = fingerprint_path(path)?;
    let artifact_id = format!("art_{}", &fingerprint.sha256[..16]);
    Ok(Projection {
        artifact_id: artifact_id.clone(),
        target_hash: fingerprint.sha256.clone(),
        target_kind: fingerprint.target_kind_hint,
        analyzer: "praxis-artifact-codec/summary".to_string(),
        summary: format!(
            "Summarized local artifact {} ({} bytes); raw content remains local.",
            artifact_id, fingerprint.size_bytes
        ),
        symbols: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        callgraph: CallgraphSummary {
            node_count: 0,
            edge_count: 0,
            top_hubs: Vec::new(),
        },
        cfg: CfgSummary {
            node_count: 0,
            branch_density: 0.0,
            loop_count: 0,
        },
        family_label: None,
        metrics: serde_json::json!({
            "size_bytes": fingerprint.size_bytes,
            "raw_exposed": false,
            "projection_mode": "summary",
            "redacted_preview_included": false
        }),
        clean_room_snippet: None,
        artifact_path: format!("artifact://{}", &fingerprint.sha256[..16]),
    })
}

pub fn redact_local_artifact(path: &Path) -> Result<Projection, ReverseError> {
    let fingerprint = fingerprint_path(path)?;
    let preview = safe_text_projection(path)?;
    let artifact_id = format!("art_{}", &fingerprint.sha256[..16]);
    Ok(Projection {
        artifact_id: artifact_id.clone(),
        target_hash: fingerprint.sha256.clone(),
        target_kind: fingerprint.target_kind_hint,
        analyzer: "praxis-artifact-codec/redaction".to_string(),
        summary: format!(
            "Redacted local artifact {} ({} bytes); preview is bucketed and bounded.",
            artifact_id, fingerprint.size_bytes
        ),
        symbols: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        callgraph: CallgraphSummary {
            node_count: 0,
            edge_count: 0,
            top_hubs: Vec::new(),
        },
        cfg: CfgSummary {
            node_count: 0,
            branch_density: 0.0,
            loop_count: 0,
        },
        family_label: None,
        metrics: serde_json::json!({
            "size_bytes": fingerprint.size_bytes,
            "raw_exposed": false,
            "projection_mode": "redaction",
            "redacted_preview": preview
        }),
        clean_room_snippet: None,
        artifact_path: format!("artifact://{}", &fingerprint.sha256[..16]),
    })
}

fn assert_no_raw_exposure(raw_hash: &str, ingest: &ArtifactIngest) -> Result<(), ReverseError> {
    let serialized = serde_json::to_string(&serde_json::json!({
        "artifact_id": ingest.artifact_id,
        "size_bytes": ingest.size_bytes
    }))
    .map_err(|err| ReverseError::Codec(err.to_string()))?;
    if serialized.contains(raw_hash) {
        return Err(ReverseError::Codec(
            "projection attempted to expose raw hash in an unsafe field".to_string(),
        ));
    }
    Ok(())
}
