#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use core_test_support::responses::ev_apply_patch_function_call;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_with_timeout;
use core_test_support::zsh_fork::build_zsh_fork_test;
use core_test_support::zsh_fork::restrictive_workspace_write_policy;
use core_test_support::zsh_fork::zsh_fork_runtime;
use praxis_core::PraxisThread;
use praxis_core::config::Constrained;
use praxis_core::config_loader::ConfigLayerStack;
use praxis_core::config_loader::ConfigLayerStackOrdering;
use praxis_core::config_loader::NetworkConstraints;
use praxis_core::config_loader::NetworkRequirementsToml;
use praxis_core::config_loader::RequirementSource;
use praxis_core::config_loader::Sourced;
use praxis_core::sandboxing::SandboxPermissions;
use praxis_features::Feature;
use praxis_protocol::approvals::NetworkApprovalProtocol;
use praxis_protocol::approvals::NetworkPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyRuleAction;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::ExecPolicyAmendment;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use regex_lite::Regex;
use serde_json::Value;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[derive(Clone, Copy)]
enum TargetPath {
    Workspace(&'static str),
    OutsideWorkspace(&'static str),
}

impl TargetPath {
    fn resolve_for_patch(self, test: &TestPraxis) -> (PathBuf, String) {
        match self {
            TargetPath::Workspace(name) => {
                let path = test.cwd.path().join(name);
                (path, name.to_string())
            }
            TargetPath::OutsideWorkspace(name) => {
                let path = env::current_dir()
                    .expect("current dir should be available")
                    .join(name);
                (path.clone(), path.display().to_string())
            }
        }
    }
}

#[derive(Clone)]
enum ActionKind {
    WriteFile {
        target: TargetPath,
        content: &'static str,
    },
    FetchUrlNoProxy {
        endpoint: &'static str,
        response_body: &'static str,
    },
    FetchUrl {
        endpoint: &'static str,
        response_body: &'static str,
    },
    RunCommand {
        command: &'static str,
    },
    RunUnifiedExecCommand {
        command: &'static str,
        justification: Option<&'static str>,
    },
    ApplyPatchFunction {
        target: TargetPath,
        content: &'static str,
    },
    ApplyPatchShell {
        target: TargetPath,
        content: &'static str,
    },
}

const DEFAULT_UNIFIED_EXEC_JUSTIFICATION: &str =
    "Requires escalated permissions to bypass the sandbox in tests.";

impl ActionKind {
    async fn prepare(
        &self,
        test: &TestPraxis,
        server: &MockServer,
        call_id: &str,
        sandbox_permissions: SandboxPermissions,
    ) -> Result<(Value, Option<String>)> {
        match self {
            ActionKind::WriteFile { target, content } => {
                let (path, _) = target.resolve_for_patch(test);
                let _ = fs::remove_file(&path);
                let path_str = path.display().to_string();
                let script = format!(
                    "from pathlib import Path; path = Path({path_str:?}); content = {content:?}; path.write_text(content, encoding='utf-8'); print(path.read_text(encoding='utf-8'), end='')",
                );
                let command = format!("python3 -c {script:?}");
                let event = shell_event(
                    call_id,
                    &command,
                    /*timeout_ms*/ 5_000,
                    sandbox_permissions,
                )?;
                Ok((event, Some(command)))
            }
            ActionKind::FetchUrl {
                endpoint,
                response_body,
            } => {
                Mock::given(method("GET"))
                    .and(path(*endpoint))
                    .respond_with(
                        ResponseTemplate::new(200).set_body_string(response_body.to_string()),
                    )
                    .mount(server)
                    .await;

                let url = format!("{}{}", server.uri(), endpoint);
                let escaped_url = url.replace('\'', "\\'");
                let script = format!(
                    "import sys\nimport urllib.request\nurl = '{escaped_url}'\ntry:\n    data = urllib.request.urlopen(url, timeout=2).read().decode()\n    print('OK:' + data.strip())\nexcept Exception as exc:\n    print('ERR:' + exc.__class__.__name__)\n    sys.exit(1)",
                );

                let command = format!("python3 -c \"{script}\"");
                let event = shell_event(
                    call_id,
                    &command,
                    /*timeout_ms*/ 5_000,
                    sandbox_permissions,
                )?;
                Ok((event, Some(command)))
            }
            ActionKind::FetchUrlNoProxy {
                endpoint,
                response_body,
            } => {
                Mock::given(method("GET"))
                    .and(path(*endpoint))
                    .respond_with(
                        ResponseTemplate::new(200).set_body_string(response_body.to_string()),
                    )
                    .mount(server)
                    .await;

                let url = format!("{}{}", server.uri(), endpoint);
                let escaped_url = url.replace('\'', "\\'");
                let script = format!(
                    "import sys\nimport urllib.request\nurl = '{escaped_url}'\nopener = urllib.request.build_opener(urllib.request.ProxyHandler({{}}))\ntry:\n    data = opener.open(url, timeout=2).read().decode()\n    print('OK:' + data.strip())\nexcept Exception as exc:\n    print('ERR:' + exc.__class__.__name__)\n    sys.exit(1)",
                );

                let command = format!("python3 -c \"{script}\"");
                let event = shell_event(
                    call_id,
                    &command,
                    /*timeout_ms*/ 5_000,
                    sandbox_permissions,
                )?;
                Ok((event, Some(command)))
            }
            ActionKind::RunCommand { command } => {
                let event = shell_event(
                    call_id,
                    command,
                    /*timeout_ms*/ 2_000,
                    sandbox_permissions,
                )?;
                Ok((event, Some(command.to_string())))
            }
            ActionKind::RunUnifiedExecCommand {
                command,
                justification,
            } => {
                let event = exec_command_event(
                    call_id,
                    command,
                    Some(1000),
                    sandbox_permissions,
                    *justification,
                )?;
                Ok((event, Some(command.to_string())))
            }
            ActionKind::ApplyPatchFunction { target, content } => {
                let (path, patch_path) = target.resolve_for_patch(test);
                let _ = fs::remove_file(&path);
                let patch = build_add_file_patch(&patch_path, content);
                Ok((ev_apply_patch_function_call(call_id, &patch), None))
            }
            ActionKind::ApplyPatchShell { target, content } => {
                let (path, patch_path) = target.resolve_for_patch(test);
                let _ = fs::remove_file(&path);
                let patch = build_add_file_patch(&patch_path, content);
                let command = shell_apply_patch_command(&patch);
                let event = shell_event(
                    call_id,
                    &command,
                    /*timeout_ms*/ 5_000,
                    sandbox_permissions,
                )?;
                Ok((event, Some(command)))
            }
        }
    }
}

fn build_add_file_patch(patch_path: &str, content: &str) -> String {
    format!("*** Begin Patch\n*** Add File: {patch_path}\n+{content}\n*** End Patch\n")
}

fn shell_apply_patch_command(patch: &str) -> String {
    let mut script = String::from("apply_patch <<'PATCH'\n");
    script.push_str(patch);
    if !patch.ends_with('\n') {
        script.push('\n');
    }
    script.push_str("PATCH\n");
    script
}

fn shell_event(
    call_id: &str,
    command: &str,
    timeout_ms: u64,
    sandbox_permissions: SandboxPermissions,
) -> Result<Value> {
    shell_event_with_prefix_rule(
        call_id,
        command,
        timeout_ms,
        sandbox_permissions,
        /*prefix_rule*/ None,
    )
}

fn shell_event_with_prefix_rule(
    call_id: &str,
    command: &str,
    timeout_ms: u64,
    sandbox_permissions: SandboxPermissions,
    prefix_rule: Option<Vec<String>>,
) -> Result<Value> {
    let mut args = json!({
        "command": command,
        "timeout_ms": timeout_ms,
    });
    if sandbox_permissions.requests_sandbox_override() {
        args["sandbox_permissions"] = json!(sandbox_permissions);
    }
    if let Some(prefix_rule) = prefix_rule {
        args["prefix_rule"] = json!(prefix_rule);
    }
    let args_str = serde_json::to_string(&args)?;
    Ok(ev_function_call(call_id, "shell_command", &args_str))
}

fn exec_command_event(
    call_id: &str,
    cmd: &str,
    yield_time_ms: Option<u64>,
    sandbox_permissions: SandboxPermissions,
    justification: Option<&str>,
) -> Result<Value> {
    let mut args = json!({
        "cmd": cmd.to_string(),
    });
    if let Some(yield_time_ms) = yield_time_ms {
        args["yield_time_ms"] = json!(yield_time_ms);
    }
    if sandbox_permissions.requests_sandbox_override() {
        args["sandbox_permissions"] = json!(sandbox_permissions);
        let reason = justification.unwrap_or(DEFAULT_UNIFIED_EXEC_JUSTIFICATION);
        args["justification"] = json!(reason);
    }
    let args_str = serde_json::to_string(&args)?;
    Ok(ev_function_call(call_id, "exec_command", &args_str))
}

#[derive(Clone)]
enum Expectation {
    FileCreated {
        target: TargetPath,
        content: &'static str,
    },
    FileCreatedNoExitCode {
        target: TargetPath,
        content: &'static str,
    },
    PatchApplied {
        target: TargetPath,
        content: &'static str,
    },
    FileNotCreated {
        target: TargetPath,
        message_contains: &'static [&'static str],
    },
    NetworkSuccess {
        body_contains: &'static str,
    },
    NetworkSuccessNoExitCode {
        body_contains: &'static str,
    },
    NetworkFailure {
        expect_tag: &'static str,
    },
    CommandSuccess {
        stdout_contains: &'static str,
    },
    CommandSuccessNoExitCode {
        stdout_contains: &'static str,
    },
    CommandFailure {
        output_contains: &'static str,
    },
}

impl Expectation {
    fn verify(&self, test: &TestPraxis, result: &CommandResult) -> Result<()> {
        match self {
            Expectation::FileCreated { target, content } => {
                let (path, _) = target.resolve_for_patch(test);
                assert_eq!(
                    result.exit_code,
                    Some(0),
                    "expected successful exit for {path:?}"
                );
                assert!(
                    result.stdout.contains(content),
                    "stdout missing {content:?}: {}",
                    result.stdout
                );
                let file_contents = fs::read_to_string(&path)?;
                assert!(
                    file_contents.contains(content),
                    "file contents missing {content:?}: {file_contents}"
                );
                let _ = fs::remove_file(path);
            }
            Expectation::FileCreatedNoExitCode { target, content } => {
                let (path, _) = target.resolve_for_patch(test);
                assert!(
                    result.exit_code.is_none() || result.exit_code == Some(0),
                    "expected no exit code for {path:?}",
                );
                assert!(
                    result.stdout.contains(content),
                    "stdout missing {content:?}: {}",
                    result.stdout
                );
                let file_contents = fs::read_to_string(&path)?;
                assert!(
                    file_contents.contains(content),
                    "file contents missing {content:?}: {file_contents}"
                );
                let _ = fs::remove_file(path);
            }
            Expectation::PatchApplied { target, content } => {
                let (path, _) = target.resolve_for_patch(test);
                match result.exit_code {
                    Some(0) | None => {
                        if result.exit_code.is_none() {
                            assert!(
                                result.stdout.contains("Success."),
                                "patch output missing success indicator: {}",
                                result.stdout
                            );
                        }
                    }
                    Some(code) => panic!(
                        "expected successful patch exit for {:?}, got {code} with stdout {}",
                        path, result.stdout
                    ),
                }
                let file_contents = fs::read_to_string(&path)?;
                assert!(
                    file_contents.contains(content),
                    "patched file missing {content:?}: {file_contents}"
                );
                let _ = fs::remove_file(path);
            }
            Expectation::FileNotCreated {
                target,
                message_contains,
            } => {
                let (path, _) = target.resolve_for_patch(test);
                assert_ne!(
                    result.exit_code,
                    Some(0),
                    "expected non-zero exit for {path:?}"
                );
                for needle in *message_contains {
                    if needle.contains('|') {
                        let options: Vec<&str> = needle.split('|').collect();
                        let matches_any =
                            options.iter().any(|option| result.stdout.contains(option));
                        assert!(
                            matches_any,
                            "stdout missing one of {options:?}: {}",
                            result.stdout
                        );
                    } else {
                        assert!(
                            result.stdout.contains(needle),
                            "stdout missing {needle:?}: {}",
                            result.stdout
                        );
                    }
                }
                assert!(
                    !path.exists(),
                    "command should not create {path:?}, but file exists"
                );
            }
            Expectation::NetworkSuccess { body_contains } => {
                assert_eq!(
                    result.exit_code,
                    Some(0),
                    "expected successful network exit: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains("OK:"),
                    "stdout missing OK prefix: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(body_contains),
                    "stdout missing body text {body_contains:?}: {}",
                    result.stdout
                );
            }
            Expectation::NetworkSuccessNoExitCode { body_contains } => {
                assert!(
                    result.exit_code.is_none() || result.exit_code == Some(0),
                    "expected no exit code for successful network call: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains("OK:"),
                    "stdout missing OK prefix: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(body_contains),
                    "stdout missing body text {body_contains:?}: {}",
                    result.stdout
                );
            }
            Expectation::NetworkFailure { expect_tag } => {
                assert_ne!(
                    result.exit_code,
                    Some(0),
                    "expected non-zero exit for network failure: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains("ERR:"),
                    "stdout missing ERR prefix: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(expect_tag),
                    "stdout missing expected tag {expect_tag:?}: {}",
                    result.stdout
                );
            }
            Expectation::CommandSuccess { stdout_contains } => {
                assert_eq!(
                    result.exit_code,
                    Some(0),
                    "expected successful trusted command exit: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(stdout_contains),
                    "trusted command stdout missing {stdout_contains:?}: {}",
                    result.stdout
                );
            }
            Expectation::CommandSuccessNoExitCode { stdout_contains } => {
                assert!(
                    result.exit_code.is_none() || result.exit_code == Some(0),
                    "expected no exit code for trusted command: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(stdout_contains),
                    "trusted command stdout missing {stdout_contains:?}: {}",
                    result.stdout
                );
            }
            Expectation::CommandFailure { output_contains } => {
                assert_ne!(
                    result.exit_code,
                    Some(0),
                    "expected non-zero exit for command failure: {}",
                    result.stdout
                );
                assert!(
                    result.stdout.contains(output_contains),
                    "command failure stderr missing {output_contains:?}: {}",
                    result.stdout
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum Outcome {
    Auto,
    ExecApproval {
        decision: ReviewDecision,
        expected_reason: Option<&'static str>,
    },
    PatchApproval {
        decision: ReviewDecision,
        expected_reason: Option<&'static str>,
    },
}

#[derive(Clone)]
struct ScenarioSpec {
    name: &'static str,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    action: ActionKind,
    sandbox_permissions: SandboxPermissions,
    features: Vec<Feature>,
    model_override: Option<&'static str>,
    outcome: Outcome,
    expectation: Expectation,
}

struct CommandResult {
    exit_code: Option<i64>,
    stdout: String,
}

async fn submit_turn(
    test: &TestPraxis,
    prompt: &str,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
) -> Result<()> {
    let session_model = test.session_configured.model.clone();

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: prompt.into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd.path().to_path_buf(),
            approval_policy,
            approvals_reviewer: None,
            sandbox_policy,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    Ok(())
}

fn parse_result(item: &Value) -> CommandResult {
    let output_str = item
        .get("output")
        .and_then(Value::as_str)
        .expect("shell output payload");
    match serde_json::from_str::<Value>(output_str) {
        Ok(parsed) => {
            let exit_code = parsed["metadata"]["exit_code"].as_i64();
            let stdout = parsed["output"].as_str().unwrap_or_default().to_string();
            CommandResult { exit_code, stdout }
        }
        Err(_) => {
            let structured = Regex::new(r"(?s)^Exit code:\s*(-?\d+).*?Output:\n(.*)$").unwrap();
            let regex =
                Regex::new(r"(?s)^.*?Process exited with code (\d+)\n.*?Output:\n(.*)$").unwrap();
            // parse freeform output
            if let Some(captures) = structured.captures(output_str) {
                let exit_code = captures.get(1).unwrap().as_str().parse::<i64>().unwrap();
                let output = captures.get(2).unwrap().as_str();
                CommandResult {
                    exit_code: Some(exit_code),
                    stdout: output.to_string(),
                }
            } else if let Some(captures) = regex.captures(output_str) {
                let exit_code = captures.get(1).unwrap().as_str().parse::<i64>().unwrap();
                let output = captures.get(2).unwrap().as_str();
                CommandResult {
                    exit_code: Some(exit_code),
                    stdout: output.to_string(),
                }
            } else {
                CommandResult {
                    exit_code: None,
                    stdout: output_str.to_string(),
                }
            }
        }
    }
}

async fn expect_exec_approval(
    test: &TestPraxis,
    expected_command: &str,
) -> ExecApprovalRequestEvent {
    let event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;

    match event {
        EventMsg::ExecApprovalRequest(approval) => {
            let last_arg = approval
                .command
                .last()
                .map(std::string::String::as_str)
                .unwrap_or_default();
            assert_eq!(last_arg, expected_command);
            approval
        }
        EventMsg::TurnComplete(_) => panic!("expected approval request before completion"),
        other => panic!("unexpected event: {other:?}"),
    }
}

async fn expect_patch_approval(
    test: &TestPraxis,
    expected_call_id: &str,
) -> ApplyPatchApprovalRequestEvent {
    let event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::ApplyPatchApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;

    match event {
        EventMsg::ApplyPatchApprovalRequest(approval) => {
            assert_eq!(approval.call_id, expected_call_id);
            approval
        }
        EventMsg::TurnComplete(_) => panic!("expected patch approval request before completion"),
        other => panic!("unexpected event: {other:?}"),
    }
}

async fn wait_for_completion_without_approval(test: &TestPraxis) {
    let event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;

    match event {
        EventMsg::TurnComplete(_) => {}
        EventMsg::ExecApprovalRequest(event) => {
            panic!("unexpected approval request: {:?}", event.command)
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

async fn wait_for_completion(test: &TestPraxis) {
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
}

fn body_contains(req: &Request, text: &str) -> bool {
    let is_zstd = req
        .headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|entry| entry.trim().eq_ignore_ascii_case("zstd"))
        });
    let bytes = if is_zstd {
        zstd::stream::decode_all(std::io::Cursor::new(&req.body)).ok()
    } else {
        Some(req.body.clone())
    };
    bytes
        .and_then(|body| String::from_utf8(body).ok())
        .is_some_and(|body| body.contains(text))
}

async fn wait_for_spawned_thread(test: &TestPraxis) -> Result<Arc<PraxisThread>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let ids = test.thread_manager.list_thread_ids().await;
        if let Some(thread_id) = ids
            .iter()
            .find(|id| **id != test.session_configured.session_id)
        {
            return test
                .thread_manager
                .get_thread(*thread_id)
                .await
                .map_err(anyhow::Error::from);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for spawned thread");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

mod network_policy;
mod policy_persistence;
mod prefix_rules;
mod scenario_matrix;
mod subagent_policy;
