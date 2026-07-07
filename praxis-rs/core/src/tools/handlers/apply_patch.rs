use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use async_trait::async_trait;
use praxis_apply_patch::AffectedPaths;
use praxis_apply_patch::ApplyPatchAction;
use praxis_apply_patch::ApplyPatchFileChange;
use praxis_apply_patch::Hunk;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::models::PermissionProfile;
use praxis_sandboxing::policy_transforms::effective_file_system_sandbox_policy;
use praxis_sandboxing::policy_transforms::normalize_additional_permissions;
use praxis_tools::ApplyPatchToolArgs;
use praxis_utils_absolute_path::AbsolutePathBuf;

use crate::apply_patch;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::exec::ExecToolCallOutput;
use crate::exec::StreamOutput;
use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::context::ApplyPatchToolOutput;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEmitter;
use crate::tools::events::ToolEventCtx;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::handlers::parse_arguments;
use crate::tools::orchestrator::ToolOrchestrator;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::runtimes::apply_patch::ApplyPatchRequest;
use crate::tools::runtimes::apply_patch::ApplyPatchRuntime;
use crate::tools::runtimes::apply_patch::apply_patch_agent_os_command;
use crate::tools::runtimes::managed_execution_pipeline::RuntimeExecutionRoute;
use crate::tools::runtimes::managed_execution_pipeline::finish_agent_os_span_success_bytes;
use crate::tools::runtimes::managed_execution_pipeline::preflight_command_intent;
use crate::tools::runtimes::managed_execution_pipeline::record_known_dirty_files_or_finish;
use crate::tools::runtimes::managed_execution_pipeline::start_agent_os_span_for_route;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;

pub struct ApplyPatchHandler;

fn file_paths_for_action(action: &ApplyPatchAction) -> Vec<AbsolutePathBuf> {
    let mut keys = Vec::new();
    let cwd = action.cwd.as_path();

    for (path, change) in action.changes() {
        if let Some(key) = to_abs_path(cwd, path) {
            keys.push(key);
        }

        if let ApplyPatchFileChange::Update { move_path, .. } = change
            && let Some(dest) = move_path
            && let Some(key) = to_abs_path(cwd, dest)
        {
            keys.push(key);
        }
    }

    keys
}

fn to_abs_path(cwd: &Path, path: &Path) -> Option<AbsolutePathBuf> {
    AbsolutePathBuf::resolve_path_against_base(path, cwd).ok()
}

fn retarget_absolute_patch_cwd(mut action: ApplyPatchAction) -> ApplyPatchAction {
    if !patch_targets_are_all_absolute(&action.patch) {
        return action;
    }

    if let Some(cwd) = common_parent_for_action_paths(&action) {
        action.cwd = cwd;
    }

    action
}

fn patch_targets_are_all_absolute(patch: &str) -> bool {
    let Ok(args) = praxis_apply_patch::parse_patch(patch) else {
        return false;
    };
    let mut saw_target = false;
    for hunk in args.hunks {
        match hunk {
            Hunk::AddFile { path, .. } | Hunk::DeleteFile { path } => {
                saw_target = true;
                if !path.is_absolute() {
                    return false;
                }
            }
            Hunk::UpdateFile {
                path, move_path, ..
            } => {
                saw_target = true;
                if !path.is_absolute() {
                    return false;
                }
                if let Some(move_path) = move_path
                    && !move_path.is_absolute()
                {
                    return false;
                }
            }
        }
    }

    saw_target
}

fn common_parent_for_action_paths(action: &ApplyPatchAction) -> Option<PathBuf> {
    let dirs = file_paths_for_action(action)
        .into_iter()
        .filter_map(|path| path.into_path_buf().parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    let mut common = dirs.first()?.clone();

    while !dirs.iter().all(|dir| dir.starts_with(&common)) {
        if !common.pop() {
            return None;
        }
    }

    Some(common)
}

fn write_permissions_for_paths(
    file_paths: &[AbsolutePathBuf],
    file_system_sandbox_policy: &praxis_protocol::permissions::FileSystemSandboxPolicy,
    cwd: &Path,
) -> Option<PermissionProfile> {
    let write_paths = file_paths
        .iter()
        .map(|path| {
            path.parent()
                .unwrap_or_else(|| path.clone())
                .into_path_buf()
        })
        .filter(|path| !file_system_sandbox_policy.can_write_path_with_cwd(path.as_path(), cwd))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(AbsolutePathBuf::from_absolute_path)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    let permissions = (!write_paths.is_empty()).then_some(PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(write_paths),
        }),
        ..Default::default()
    })?;

    normalize_additional_permissions(permissions).ok()
}

async fn effective_patch_permissions(
    session: &Session,
    turn: &TurnContext,
    action: &ApplyPatchAction,
) -> (
    Vec<AbsolutePathBuf>,
    crate::tools::handlers::EffectiveAdditionalPermissions,
    praxis_protocol::permissions::FileSystemSandboxPolicy,
) {
    let file_paths = file_paths_for_action(action);
    let permissions = turn.effective_permissions();
    let file_system_sandbox_policy = effective_file_system_sandbox_policy(
        &permissions.file_system_sandbox_policy,
        permissions.granted_permissions.as_ref(),
    );
    let effective_additional_permissions = apply_granted_turn_permissions(
        session,
        crate::sandboxing::SandboxPermissions::UseDefault,
        write_permissions_for_paths(&file_paths, &file_system_sandbox_policy, turn.cwd.as_path()),
    );

    (
        file_paths,
        effective_additional_permissions,
        file_system_sandbox_policy,
    )
}

#[async_trait]
impl ToolHandler for ApplyPatchHandler {
    type Output = ApplyPatchToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::Custom { .. }
        )
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        true
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
            ..
        } = invocation;

        let patch_input = match payload {
            ToolPayload::Function { arguments } => {
                let args: ApplyPatchToolArgs = parse_arguments(&arguments)?;
                args.input
            }
            ToolPayload::Custom { input } => input,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received unsupported payload".to_string(),
                ));
            }
        };

        let cwd = turn.cwd.clone();
        let command = vec!["apply_patch".to_string(), patch_input.clone()];
        match praxis_apply_patch::maybe_parse_apply_patch_verified(&command, &cwd) {
            praxis_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
                let content = run_verified_apply_patch(
                    &command,
                    &cwd,
                    changes,
                    None,
                    session,
                    turn,
                    Some(&tracker),
                    &call_id,
                    tool_name.as_str(),
                    false,
                )
                .await?;
                Ok(ApplyPatchToolOutput::from_text(content))
            }
            praxis_apply_patch::MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
                Err(FunctionCallError::RespondToModel(format!(
                    "apply_patch verification failed: {parse_error}"
                )))
            }
            praxis_apply_patch::MaybeApplyPatchVerified::ShellParseError(error) => {
                tracing::trace!("Failed to parse apply_patch input, {error:?}");
                Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received invalid patch input".to_string(),
                ))
            }
            praxis_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => {
                Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received non-apply_patch input".to_string(),
                ))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_verified_apply_patch(
    _command: &[String],
    cwd: &Path,
    changes: ApplyPatchAction,
    timeout_ms: Option<u64>,
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    tracker: Option<&SharedTurnDiffTracker>,
    call_id: &str,
    tool_name: &str,
    warn_if_intercepted: bool,
) -> Result<String, FunctionCallError> {
    if warn_if_intercepted {
        session
            .record_model_warning(
                format!(
                    "apply_patch was requested via {tool_name}. Use the apply_patch tool instead of exec_command."
                ),
                turn.as_ref(),
            )
            .await;
    }

    let changes = retarget_absolute_patch_cwd(changes);
    let (file_paths, effective_additional_permissions, file_system_sandbox_policy) =
        effective_patch_permissions(session.as_ref(), turn.as_ref(), &changes).await;
    let dirty_files: Vec<std::path::PathBuf> = file_paths
        .iter()
        .map(|path| path.clone().into_path_buf())
        .collect();
    let tool_ctx = ToolCtx {
        session: session.clone(),
        turn: turn.clone(),
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
    };
    let agent_os_command = apply_patch_agent_os_command(&changes);
    match apply_patch::apply_patch(turn.as_ref(), &file_system_sandbox_policy, changes).await {
        InternalApplyPatchInvocation::Output(item) => {
            preflight_command_intent(&tool_ctx, &agent_os_command, cwd)
                .await
                .map_err(tool_error_to_model_error)?;
            let command_span = start_agent_os_span_for_route(
                &tool_ctx,
                &agent_os_command,
                cwd,
                RuntimeExecutionRoute::apply_patch(),
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(format!("{err:?}")))?;
            match item {
                Ok(content) => {
                    record_known_dirty_files_or_finish(&command_span, dirty_files)
                        .await
                        .map_err(|err| FunctionCallError::RespondToModel(format!("{err:?}")))?;
                    finish_agent_os_span_success_bytes(
                        &command_span,
                        "apply_patch",
                        content.as_bytes(),
                    )
                    .await;
                    Ok(content)
                }
                Err(err) => {
                    let error_text = err.to_string();
                    let _ = command_span.finish_failure(error_text.as_bytes()).await;
                    Err(err)
                }
            }
        }
        InternalApplyPatchInvocation::ApplyInProcess(apply) => {
            let changes = convert_apply_patch_to_protocol(&apply.action);
            let emitter = ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
            let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), call_id, tracker);
            emitter.begin(event_ctx).await;

            let started = Instant::now();
            let out = match run_apply_patch_in_process(&tool_ctx, &agent_os_command, &apply.action)
                .await
            {
                Ok(content) => Ok(apply_patch_exec_output(
                    0,
                    content,
                    String::new(),
                    started.elapsed(),
                )),
                Err(err) => Ok(apply_patch_exec_output(
                    1,
                    String::new(),
                    err.to_string(),
                    started.elapsed(),
                )),
            };
            let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), call_id, tracker);
            emitter.finish(event_ctx, out).await
        }
        InternalApplyPatchInvocation::DelegateToExec(apply) => {
            let changes = convert_apply_patch_to_protocol(&apply.action);
            let emitter = ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
            let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), call_id, tracker);
            emitter.begin(event_ctx).await;

            let req = ApplyPatchRequest {
                action: apply.action,
                agent_os_command,
                file_paths,
                changes,
                exec_approval_requirement: apply.exec_approval_requirement,
                additional_permissions: effective_additional_permissions.additional_permissions,
                permissions_preapproved: effective_additional_permissions.permissions_preapproved,
                timeout_ms,
            };

            let mut orchestrator = ToolOrchestrator::new();
            let mut runtime = ApplyPatchRuntime::new();
            let out = orchestrator
                .run(&mut runtime, &req, &tool_ctx, turn.as_ref())
                .await
                .map(|result| result.output);
            let event_ctx = ToolEventCtx::new(session.as_ref(), turn.as_ref(), call_id, tracker);
            emitter.finish(event_ctx, out).await
        }
    }
}

fn apply_patch_exec_output(
    exit_code: i32,
    stdout: String,
    stderr: String,
    duration: Duration,
) -> ExecToolCallOutput {
    let aggregated_output = if stdout.is_empty() {
        stderr.clone()
    } else if stderr.is_empty() {
        stdout.clone()
    } else {
        format!("{stdout}\n{stderr}")
    };
    ExecToolCallOutput {
        exit_code,
        stdout: StreamOutput::new(stdout),
        stderr: StreamOutput::new(stderr),
        aggregated_output: StreamOutput::new(aggregated_output),
        model_output: None,
        duration,
        timed_out: false,
        agent_os_artifact_id: None,
        raw_output_spool: None,
    }
}

async fn run_apply_patch_in_process(
    tool_ctx: &ToolCtx,
    agent_os_command: &[String],
    action: &ApplyPatchAction,
) -> Result<String, FunctionCallError> {
    preflight_command_intent(tool_ctx, agent_os_command, &action.cwd)
        .await
        .map_err(tool_error_to_model_error)?;

    let ticket = tool_ctx
        .session
        .services
        .agent_os
        .request_command_ticket(
            tool_ctx.session.conversation_id,
            agent_os_command,
            &action.cwd,
        )
        .await
        .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;

    let result = apply_verified_patch_contents(action);
    let success = result.is_ok();
    if let Err(err) = tool_ctx
        .session
        .services
        .agent_os
        .finish_tool_ticket(&ticket, success)
        .await
    {
        return Err(FunctionCallError::RespondToModel(format!(
            "apply_patch ticket finish failed: {err}"
        )));
    }
    result
}

fn apply_verified_patch_contents(action: &ApplyPatchAction) -> Result<String, FunctionCallError> {
    let mut affected = AffectedPaths {
        added: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
    };

    for (path, change) in action.changes() {
        let path = resolve_action_path(&action.cwd, path);
        match change {
            ApplyPatchFileChange::Add { content } => {
                write_patch_file(&path, content)?;
                affected.added.push(path);
            }
            ApplyPatchFileChange::Delete { .. } => {
                remove_patch_file(&path)?;
                affected.deleted.push(path);
            }
            ApplyPatchFileChange::Update {
                move_path,
                new_content,
                ..
            } => {
                if let Some(dest) = move_path {
                    let dest = resolve_action_path(&action.cwd, dest);
                    write_patch_file(&dest, new_content)?;
                    if dest != path {
                        remove_patch_file(&path)?;
                    }
                    affected.modified.push(dest);
                } else {
                    write_patch_file(&path, new_content)?;
                    affected.modified.push(path);
                }
            }
        }
    }

    affected.added.sort();
    affected.modified.sort();
    affected.deleted.sort();

    let mut out = Vec::new();
    praxis_apply_patch::print_summary(&affected, &mut out)
        .map_err(|err| FunctionCallError::RespondToModel(format!("apply_patch failed: {err}")))?;
    String::from_utf8(out).map_err(|err| {
        FunctionCallError::RespondToModel(format!("apply_patch produced invalid UTF-8: {err}"))
    })
}

fn resolve_action_path(cwd: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn write_patch_file(path: &Path, content: &str) -> Result<(), FunctionCallError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "apply_patch failed to create parent directories for {}: {err}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(path, content.as_bytes()).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "apply_patch failed to write {}: {err}",
            path.display()
        ))
    })
}

fn remove_patch_file(path: &Path) -> Result<(), FunctionCallError> {
    std::fs::remove_file(path).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "apply_patch failed to delete {}: {err}",
            path.display()
        ))
    })
}

fn tool_error_to_model_error(err: ToolError) -> FunctionCallError {
    let message = match err {
        ToolError::Rejected(message) => message,
        ToolError::Praxis(err) => err.to_string(),
    };
    FunctionCallError::RespondToModel(message)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn intercept_apply_patch(
    command: &[String],
    cwd: &Path,
    timeout_ms: Option<u64>,
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    tracker: Option<&SharedTurnDiffTracker>,
    call_id: &str,
    tool_name: &str,
) -> Result<Option<FunctionToolOutput>, FunctionCallError> {
    match praxis_apply_patch::maybe_parse_apply_patch_verified(command, cwd) {
        praxis_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
            let content = run_verified_apply_patch(
                command, cwd, changes, timeout_ms, session, turn, tracker, call_id, tool_name, true,
            )
            .await?;
            Ok(Some(FunctionToolOutput::from_text(content, Some(true))))
        }
        praxis_apply_patch::MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
            Err(FunctionCallError::RespondToModel(format!(
                "apply_patch verification failed: {parse_error}"
            )))
        }
        praxis_apply_patch::MaybeApplyPatchVerified::ShellParseError(error) => {
            tracing::trace!("Failed to parse apply_patch input, {error:?}");
            Ok(None)
        }
        praxis_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => Ok(None),
    }
}

#[cfg(test)]
#[path = "apply_patch_tests.rs"]
mod tests;
