use praxis_thread_share::ExportIdentity;
use praxis_thread_share::parse_rollout;
use praxis_thread_share::redact_text;
use praxis_thread_share::write_export;
use pretty_assertions::assert_eq;
use std::fs;

const THREAD_ID: &str = "019faa33-9741-7a10-bf14-b81f7f5d6377";

fn rollout_fixture() -> String {
    [
        serde_json::json!({
            "timestamp": "2026-07-28T19:28:55.868Z",
            "type": "session_meta",
            "payload": {
                "id": THREAD_ID,
                "timestamp": "2026-07-28T19:28:55.868Z",
                "originator": "cunning3d_harness",
                "cli_version": "0.0.0",
                "model_provider": "openai",
                "cwd": "F:\\Cunning3D",
                "base_instructions": "private bootstrap material",
                "git": {
                    "commit_hash": "46aa074a430bbb4e5c6d81d946a5e35a90f87347ee",
                    "branch": "main",
                    "repository_url": "https://github.com/Cunning3D/Cunning3D-Dev.git"
                }
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:28:55.900Z",
            "type": "turn_context",
            "payload": { "model": "gpt-5.6-sol", "cwd": "F:\\Cunning3D" }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:28:55.910Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "# AGENTS.md instructions for F:\\Cunning3D\nprivate" },
                    { "type": "input_text", "text": "<environment_context>private</environment_context>" }
                ]
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:28:55.920Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "只回复 HARNESS_OK_20260729，不要调用工具。" }]
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:29:01.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "phase": "final_answer",
                "content": [{ "type": "output_text", "text": "HARNESS_OK_20260729" }]
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:29:01.010Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "<environment_context>late private bootstrap</environment_context>" }]
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:29:01.020Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "<praxis_internal_context source=\"goal\">private goal control</praxis_internal_context>" }]
            }
        }),
        serde_json::json!({
            "timestamp": "2026-07-28T19:29:01.100Z",
            "type": "response_item",
            "payload": { "type": "function_call", "name": "shell_command", "arguments": "secret" }
        }),
    ]
    .into_iter()
    .map(|value| serde_json::to_string(&value).expect("serialize fixture line"))
    .collect::<Vec<_>>()
    .join("\n")
}

#[test]
fn parser_exports_only_real_user_and_assistant_messages() {
    let parsed = parse_rollout(rollout_fixture().as_bytes()).expect("parse rollout");

    assert_eq!(parsed.thread_id, THREAD_ID);
    assert_eq!(parsed.title, "只回复 HARNESS_OK_20260729，不要调用工具。");
    assert_eq!(parsed.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(parsed.conversation.len(), 2);
    assert_eq!(parsed.conversation[0].role, "user");
    assert_eq!(
        parsed.conversation[0].text,
        "只回复 HARNESS_OK_20260729，不要调用工具。"
    );
    assert_eq!(parsed.conversation[1].role, "assistant");
    assert_eq!(
        parsed.conversation[1].phase.as_deref(),
        Some("final_answer")
    );
    assert!(
        !serde_json::to_string(&parsed)
            .expect("serialize parsed")
            .contains("AGENTS.md")
    );
}

#[test]
fn redaction_removes_credentials_emails_and_absolute_paths() {
    let input = concat!(
        "Authorization: Bearer abcdefghijklmnopqrstuvwxyz\n",
        "GH_TOKEN=github_pat_abcdefghijklmnopqrstuvwxyz123456\n",
        "mail me at private@example.com from F:\\Cunning3D\\secret.txt\n",
        "markdown [source](/F:/Cunning3D/src/private.rs:42) and F:/Cunning3D/other.rs\n",
        "public https://github.com/Cunning3D/Cunning3D-Dev stays visible\n",
        "home /home/alice/private/file.txt\n"
    );

    let redacted = redact_text(input).expect("redact text");

    assert!(!redacted.text.contains("abcdefghijklmnopqrstuvwxyz"));
    assert!(!redacted.text.contains("private@example.com"));
    assert!(!redacted.text.contains("F:\\Cunning3D"));
    assert!(!redacted.text.contains("F:/Cunning3D"));
    assert!(
        redacted
            .text
            .contains("https://github.com/Cunning3D/Cunning3D-Dev")
    );
    assert!(!redacted.text.contains("/home/alice"));
    assert!(redacted.kinds.contains(&"bearer-token".to_string()));
    assert!(redacted.kinds.contains(&"secret".to_string()));
    assert!(redacted.kinds.contains(&"email".to_string()));
    assert!(redacted.kinds.contains(&"absolute-path".to_string()));
}

#[test]
fn writer_updates_one_index_entry_when_thread_is_reshared() {
    let repository = tempfile::tempdir().expect("create repository tempdir");
    let parsed = parse_rollout(rollout_fixture().as_bytes()).expect("parse rollout");
    let identity = ExportIdentity {
        github_login: "o-oOvOo-o".to_string(),
        git_name: Some("0xAdrain".to_string()),
    };

    let first = write_export(
        repository.path(),
        &parsed,
        &identity,
        "Geometry Core",
        "2026-08-11T10:00:00Z",
    )
    .expect("write first export");
    let second = write_export(
        repository.path(),
        &parsed,
        &identity,
        "Geometry Core",
        "2026-08-11T11:00:00Z",
    )
    .expect("write second export");

    assert_eq!(first.relative_path, second.relative_path);
    assert_eq!(
        first.relative_path,
        format!("threads/2026/07/{THREAD_ID}.json")
    );
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(repository.path().join("index.json")).expect("read index"),
    )
    .expect("parse index");
    assert_eq!(index["threads"].as_array().expect("threads array").len(), 1);
    assert_eq!(index["schemaVersion"], 2);
    assert_eq!(index["threads"][0]["project"], "Cunning3D/Cunning3D-Dev");
    assert_eq!(
        index["threads"][0]["projectKey"],
        "github:cunning3d/cunning3d-dev"
    );
    assert_eq!(index["threads"][0]["team"], "Geometry Core");
    assert_eq!(index["threads"][0]["teamKey"], "geometry-core");
    assert_eq!(index["threads"][0]["publishedAt"], "2026-08-11T11:00:00Z");

    let exported: serde_json::Value = serde_json::from_slice(
        &fs::read(repository.path().join(first.relative_path)).expect("read export"),
    )
    .expect("parse export");
    assert_eq!(exported["schemaVersion"], 2);
    assert_eq!(exported["workspace"]["project"], "Cunning3D/Cunning3D-Dev");
    assert_eq!(exported["workspace"]["team"], "Geometry Core");
}
