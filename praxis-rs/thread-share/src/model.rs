use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub role: String,
    pub phase: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedThread {
    pub thread_id: String,
    pub title: String,
    pub created_at: String,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub cli_version: Option<String>,
    pub originator: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub conversation: Vec<ConversationMessage>,
    pub rollout_sha256: String,
    pub redaction_count: usize,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedText {
    pub text: String,
    pub count: usize,
    pub kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportIdentity {
    pub github_login: String,
    pub git_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    pub relative_path: String,
    pub project: String,
    pub team: String,
    pub message_count: usize,
    pub redaction_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub thread_id: String,
    pub relative_path: String,
    pub commit: String,
    pub web_url: Option<String>,
    pub pushed: bool,
    pub project: String,
    pub team: String,
    pub message_count: usize,
    pub redaction_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadExport {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub schema_version: u8,
    pub thread_id: String,
    pub title: String,
    pub submitted_by: SubmittedBy,
    pub created_at: String,
    pub published_at: String,
    pub workspace: Option<WorkspaceMetadata>,
    pub source: SourceMetadata,
    pub praxis: PraxisMetadata,
    pub stats: ExportStats,
    pub conversation: Vec<ConversationMessage>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmittedBy {
    pub github_login: String,
    pub git_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMetadata {
    pub project: String,
    pub project_key: String,
    pub team: String,
    pub team_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceMetadata {
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PraxisMetadata {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub cli_version: Option<String>,
    pub originator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportStats {
    pub message_count: usize,
    pub redaction_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Provenance {
    pub rollout_sha256: String,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadIndex {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub schema_version: u8,
    pub generated_at: Option<String>,
    pub threads: Vec<ThreadIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadIndexEntry {
    pub thread_id: String,
    pub title: String,
    pub submitted_by: String,
    pub published_at: String,
    pub path: String,
    pub message_count: usize,
    pub project: String,
    pub project_key: String,
    pub team: String,
    pub team_key: String,
    pub model: Option<String>,
    pub repository: Option<String>,
}
