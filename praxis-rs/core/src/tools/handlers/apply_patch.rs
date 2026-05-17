use std::path::Path;

use crate::apply_patch;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::context::ApplyPatchToolOutput;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
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
use crate::tools::sandboxing::ToolCtx;
use async_trait::async_trait;
use praxis_apply_patch::ApplyPatchAction;
use praxis_apply_patch::ApplyPatchFileChange;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::models::PermissionProfile;
use praxis_sandboxing::policy_transforms::effective_file_system_sandbox_policy;
use praxis_sandboxing::policy_transforms::merge_permission_profiles;
use praxis_sandboxing::policy_transforms::normalize_additional_permissions;
use praxis_tools::ApplyPatchToolArgs;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::collections::BTreeSet;
use std::sync::Arc;

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
    let granted_permissions = merge_permission_profiles(
        session.granted_session_permissions().await.as_ref(),
        session.granted_turn_permissions().await.as_ref(),
    );
    let file_system_sandbox_policy = effective_file_system_sandbox_policy(
        &turn.file_system_sandbox_policy,
        granted_permissions.as_ref(),
    );
    let effective_additional_permissions = apply_granted_turn_permissions(
        session,
        crate::sandboxing::SandboxPermissions::UseDefault,
        write_permissions_for_paths(&file_paths, &file_system_sandbox_policy, turn.cwd.as_path()),
    )
    .await;

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

        // Re-parse and verify the patch so we can compute changes and approval.
        // Avoid building temporary ExecParams/command vectors; derive directly from inputs.
        let cwd = turn.cwd.clone();
        let command = vec!["apply_patch".to_string(), patch_input.clone()];
        match praxis_apply_patch::maybe_parse_apply_patch_verified(&command, &cwd) {
            praxis_apply_patch::MaybeApplyPatchVerified::Body(changes) => {
                session
                    .services
                    .agent_os
                    .preflight_command_intent(session.conversation_id, &command, &cwd)
                    .await
                    .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
                let command_span = session
                    .services
                    .agent_os
                    .start_managed_command(
                        session.conversation_id,
                        "apply_patch".to_string(),
                        &command,
                        &cwd,
                        None,
                    )
                    .await
                    .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
                let (file_paths, effective_additional_permissions, file_system_sandbox_policy) =
                    effective_patch_permissions(session.as_ref(), turn.as_ref(), &changes).await;
                let dirty_files = file_paths
                    .iter()
                    .map(|path| path.clone().into_path_buf())
                    .collect();
                if let Err(err) = command_span.record_dirty_files(dirty_files).await {
                    let error_text = err.to_string();
                    let _ = command_span.finish_failure(error_text.as_bytes()).await;
                    return Err(FunctionCallError::RespondToModel(error_text));
                }
                match apply_patch::apply_patch(turn.as_ref(), &file_system_sandbox_policy, changes)
                    .await
                {
                    InternalApplyPatchInvocation::Output(item) => {
                        let content = match item {
                            Ok(content) => content,
                            Err(err) => {
                                let error_text = err.to_string();
                                let _ = command_span.finish_failure(error_text.as_bytes()).await;
                                return Err(err);
                            }
                        };
                        let output = ApplyPatchToolOutput::from_text(content);
                        let output_preview = output.log_preview();
                        let _ = command_span.finish_success(output_preview.as_bytes()).await;
                        Ok(output)
                    }
                    InternalApplyPatchInvocation::DelegateToExec(apply) => {
                        let changes = convert_apply_patch_to_protocol(&apply.action);
                        let emitter =
                            ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        emitter.begin(event_ctx).await;

                        let req = ApplyPatchRequest {
                            action: apply.action,
                            file_paths,
                            changes,
                            exec_approval_requirement: apply.exec_approval_requirement,
                            additional_permissions: effective_additional_permissions
                                .additional_permissions,
                            permissions_preapproved: effective_additional_permissions
                                .permissions_preapproved,
                            timeout_ms: None,
                        };

                        let mut orchestrator = ToolOrchestrator::new();
                        let mut runtime = ApplyPatchRuntime::new();
                        let tool_ctx = ToolCtx {
                            session: session.clone(),
                            turn: turn.clone(),
                            call_id: call_id.clone(),
                            tool_name: tool_name.to_string(),
                        };
                        let out = orchestrator
                            .run(
                                &mut runtime,
                                &req,
                                &tool_ctx,
                                turn.as_ref(),
                                turn.approval_policy.value(),
                            )
                            .await
                            .map(|result| result.output);
                        let event_ctx = ToolEventCtx::new(
                            session.as_ref(),
                            turn.as_ref(),
                            &call_id,
                            Some(&tracker),
                        );
                        let finish_result = emitter.finish(event_ctx, out).await;
                        match &finish_result {
                            Ok(content) => {
                                let _ = command_span.finish_success(content.as_bytes()).await;
                            }
                            Err(err) => {
                                let error_text = err.to_string();
                                let _ = command_span.finish_failure(error_text.as_bytes()).await;
                            }
                        }
                        let content = finish_result?;
                        Ok(ApplyPatchToolOutput::from_text(content))
                    }
                }
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
            session
                .record_model_warning(
                    format!(
                        "apply_patch was requested via {tool_name}. Use the apply_patch tool instead of exec_command."
                    ),
                    turn.as_ref(),
                )
                .await;
            session
                .services
                .agent_os
                .preflight_command_intent(session.conversation_id, command, cwd)
                .await
                .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
            let command_span = session
                .services
                .agent_os
                .start_managed_command(
                    session.conversation_id,
                    praxis_shell_command::parse_command::shlex_join(command),
                    command,
                    cwd,
                    None,
                )
                .await
                .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
            let (approval_keys, effective_additional_permissions, file_system_sandbox_policy) =
                effective_patch_permissions(session.as_ref(), turn.as_ref(), &changes).await;
            let dirty_files = approval_keys
                .iter()
                .map(|path| path.clone().into_path_buf())
                .collect();
            if let Err(err) = command_span.record_dirty_files(dirty_files).await {
                let error_text = err.to_string();
                let _ = command_span.finish_failure(error_text.as_bytes()).await;
                return Err(FunctionCallError::RespondToModel(error_text));
            }
            match apply_patch::apply_patch(turn.as_ref(), &file_system_sandbox_policy, changes)
                .await
            {
                InternalApplyPatchInvocation::Output(item) => {
                    let content = match item {
                        Ok(content) => content,
                        Err(err) => {
                            let error_text = err.to_string();
                            let _ = command_span.finish_failure(error_text.as_bytes()).await;
                            return Err(err);
                        }
                    };
                    let _ = command_span.finish_success(content.as_bytes()).await;
                    Ok(Some(FunctionToolOutput::from_text(content, Some(true))))
                }
                InternalApplyPatchInvocation::DelegateToExec(apply) => {
                    let changes = convert_apply_patch_to_protocol(&apply.action);
                    let emitter = ToolEmitter::apply_patch(changes.clone(), apply.auto_approved);
                    let event_ctx = ToolEventCtx::new(
                        session.as_ref(),
                        turn.as_ref(),
                        call_id,
                        tracker.as_ref().copied(),
                    );
                    emitter.begin(event_ctx).await;

                    let req = ApplyPatchRequest {
                        action: apply.action,
                        file_paths: approval_keys,
                        changes,
                        exec_approval_requirement: apply.exec_approval_requirement,
                        additional_permissions: effective_additional_permissions
                            .additional_permissions,
                        permissions_preapproved: effective_additional_permissions
                            .permissions_preapproved,
                        timeout_ms,
                    };

                    let mut orchestrator = ToolOrchestrator::new();
                    let mut runtime = ApplyPatchRuntime::new();
                    let tool_ctx = ToolCtx {
                        session: session.clone(),
                        turn: turn.clone(),
                        call_id: call_id.to_string(),
                        tool_name: tool_name.to_string(),
                    };
                    let out = orchestrator
                        .run(
                            &mut runtime,
                            &req,
                            &tool_ctx,
                            turn.as_ref(),
                            turn.approval_policy.value(),
                        )
                        .await
                        .map(|result| result.output);
                    let event_ctx = ToolEventCtx::new(
                        session.as_ref(),
                        turn.as_ref(),
                        call_id,
                        tracker.as_ref().copied(),
                    );
                    let content = match emitter.finish(event_ctx, out).await {
                        Ok(content) => content,
                        Err(err) => {
                            let error_text = err.to_string();
                            let _ = command_span.finish_failure(error_text.as_bytes()).await;
                            return Err(err);
                        }
                    };
                    let _ = command_span.finish_success(content.as_bytes()).await;
                    Ok(Some(FunctionToolOutput::from_text(content, Some(true))))
                }
            }
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
