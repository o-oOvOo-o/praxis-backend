use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::function_tool::FunctionCallError;
use crate::hook_runtime::record_additional_contexts;
use crate::hook_runtime::run_post_tool_use_hooks;
use crate::hook_runtime::run_pre_tool_use_hooks;
use crate::memories::usage::emit_metric_for_tool_read;
use crate::sandbox_tags::sandbox_tag;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use async_trait::async_trait;
use praxis_loop::tool::ToolEffects;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_tools::ConfiguredToolSpec;
use praxis_tools::ToolSpec;
use praxis_utils_readiness::Readiness;
use serde_json::Value;
use tracing::warn;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Function,
    Mcp,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    type Output: ToolOutput + 'static;

    fn kind(&self) -> ToolKind;

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            (self.kind(), payload),
            (ToolKind::Function, ToolPayload::Function { .. })
                | (ToolKind::Function, ToolPayload::ToolSearch { .. })
                | (ToolKind::Mcp, ToolPayload::Mcp { .. })
        )
    }

    /// Returns `true` if the [ToolInvocation] *might* mutate the environment of the
    /// user (through file system, OS operations, ...).
    /// This function must remains defensive and return `true` if a doubt exist on the
    /// exact effect of a ToolInvocation.
    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        false
    }

    async fn effects(&self, _invocation: &ToolInvocation) -> ToolEffects {
        ToolEffects::unknown_write()
    }

    async fn prepare(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolPreparation, FunctionCallError> {
        Ok(ToolPreparation::new(self.effects(invocation).await))
    }

    fn pre_tool_use_payload(&self, _invocation: &ToolInvocation) -> Option<PreToolUsePayload> {
        None
    }

    fn post_tool_use_payload(
        &self,
        _call_id: &str,
        _payload: &ToolPayload,
        _result: &dyn ToolOutput,
    ) -> Option<PostToolUsePayload> {
        None
    }

    /// Perform the actual [ToolInvocation] and returns a [ToolOutput] containing
    /// the final output to return to the model.
    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError>;

    async fn handle_prepared(
        &self,
        invocation: ToolInvocation,
        _preparation: ToolPreparation,
    ) -> Result<Self::Output, FunctionCallError> {
        self.handle(invocation).await
    }
}

pub(crate) struct ToolPreparation {
    effects: ToolEffects,
    payload: Option<Box<dyn Any + Send>>,
    rejection: Option<FunctionCallError>,
}

impl ToolPreparation {
    pub(crate) fn new(effects: ToolEffects) -> Self {
        Self {
            effects,
            payload: None,
            rejection: None,
        }
    }

    pub(crate) fn rejected(error: FunctionCallError) -> Self {
        Self {
            effects: ToolEffects::unknown_write(),
            payload: None,
            rejection: Some(error),
        }
    }

    pub(crate) fn with_payload<T>(mut self, payload: T) -> Self
    where
        T: Any + Send,
    {
        self.payload = Some(Box::new(payload));
        self
    }

    pub(crate) fn effects(&self) -> &ToolEffects {
        &self.effects
    }

    pub(crate) fn take_payload<T>(&mut self) -> Option<T>
    where
        T: Any + Send,
    {
        self.payload
            .take()?
            .downcast::<T>()
            .ok()
            .map(|payload| *payload)
    }

    pub(crate) fn take_rejection(&mut self) -> Option<FunctionCallError> {
        self.rejection.take()
    }
}

pub(crate) struct AnyToolResult {
    pub(crate) call_id: String,
    pub(crate) payload: ToolPayload,
    pub(crate) result: Box<dyn ToolOutput>,
}

impl AnyToolResult {
    pub(crate) fn into_response(self) -> ResponseInputItem {
        let Self {
            call_id,
            payload,
            result,
            ..
        } = self;
        result.to_response_item(&call_id, &payload)
    }

    pub(crate) fn code_mode_result(self) -> serde_json::Value {
        let Self {
            payload, result, ..
        } = self;
        result.code_mode_result(&payload)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreToolUsePayload {
    pub(crate) command: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PostToolUsePayload {
    pub(crate) command: String,
    pub(crate) tool_response: Value,
}

#[async_trait]
trait AnyToolHandler: Send + Sync {
    fn matches_kind(&self, payload: &ToolPayload) -> bool;

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool;

    async fn effects(&self, invocation: &ToolInvocation) -> ToolEffects;

    async fn prepare(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolPreparation, FunctionCallError>;

    fn pre_tool_use_payload(&self, invocation: &ToolInvocation) -> Option<PreToolUsePayload>;

    fn post_tool_use_payload(
        &self,
        call_id: &str,
        payload: &ToolPayload,
        result: &dyn ToolOutput,
    ) -> Option<PostToolUsePayload>;

    async fn handle_any(
        &self,
        invocation: ToolInvocation,
    ) -> Result<AnyToolResult, FunctionCallError>;

    async fn handle_prepared_any(
        &self,
        invocation: ToolInvocation,
        preparation: ToolPreparation,
    ) -> Result<AnyToolResult, FunctionCallError>;
}

#[async_trait]
impl<T> AnyToolHandler for T
where
    T: ToolHandler,
{
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        ToolHandler::matches_kind(self, payload)
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        ToolHandler::is_mutating(self, invocation).await
    }

    async fn effects(&self, invocation: &ToolInvocation) -> ToolEffects {
        ToolHandler::effects(self, invocation).await
    }

    async fn prepare(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolPreparation, FunctionCallError> {
        ToolHandler::prepare(self, invocation).await
    }

    fn pre_tool_use_payload(&self, invocation: &ToolInvocation) -> Option<PreToolUsePayload> {
        ToolHandler::pre_tool_use_payload(self, invocation)
    }

    fn post_tool_use_payload(
        &self,
        call_id: &str,
        payload: &ToolPayload,
        result: &dyn ToolOutput,
    ) -> Option<PostToolUsePayload> {
        ToolHandler::post_tool_use_payload(self, call_id, payload, result)
    }

    async fn handle_any(
        &self,
        invocation: ToolInvocation,
    ) -> Result<AnyToolResult, FunctionCallError> {
        let call_id = invocation.call_id.clone();
        let payload = invocation.payload.clone();
        let output = self.handle(invocation).await?;
        Ok(AnyToolResult {
            call_id,
            payload,
            result: Box::new(output),
        })
    }

    async fn handle_prepared_any(
        &self,
        invocation: ToolInvocation,
        preparation: ToolPreparation,
    ) -> Result<AnyToolResult, FunctionCallError> {
        let call_id = invocation.call_id.clone();
        let payload = invocation.payload.clone();
        let output = self.handle_prepared(invocation, preparation).await?;
        Ok(AnyToolResult {
            call_id,
            payload,
            result: Box::new(output),
        })
    }
}

pub(crate) fn tool_handler_key(tool_name: &str, namespace: Option<&str>) -> String {
    if let Some(namespace) = namespace {
        format!("{namespace}:{tool_name}")
    } else {
        tool_name.to_string()
    }
}

pub struct ToolRegistry {
    handlers: HashMap<String, Arc<dyn AnyToolHandler>>,
}

impl ToolRegistry {
    fn new(handlers: HashMap<String, Arc<dyn AnyToolHandler>>) -> Self {
        Self { handlers }
    }

    fn handler(&self, name: &str, namespace: Option<&str>) -> Option<Arc<dyn AnyToolHandler>> {
        self.handlers
            .get(&tool_handler_key(name, namespace))
            .map(Arc::clone)
    }

    #[cfg(test)]
    pub(crate) fn has_handler(&self, name: &str, namespace: Option<&str>) -> bool {
        self.handler(name, namespace).is_some()
    }

    // TODO(jif) for dynamic tools.
    // pub fn register(&mut self, name: impl Into<String>, handler: Arc<dyn ToolHandler>) {
    //     let name = name.into();
    //     if self.handlers.insert(name.clone(), handler).is_some() {
    //         warn!("overwriting handler for tool {name}");
    //     }
    // }

    pub(crate) async fn dispatch_any(
        &self,
        invocation: ToolInvocation,
    ) -> Result<AnyToolResult, FunctionCallError> {
        self.dispatch_any_with_preparation(invocation, None).await
    }

    pub(crate) async fn dispatch_prepared_any(
        &self,
        invocation: ToolInvocation,
        preparation: ToolPreparation,
    ) -> Result<AnyToolResult, FunctionCallError> {
        self.dispatch_any_with_preparation(invocation, Some(preparation))
            .await
    }

    async fn dispatch_any_with_preparation(
        &self,
        invocation: ToolInvocation,
        preparation: Option<ToolPreparation>,
    ) -> Result<AnyToolResult, FunctionCallError> {
        let tool_name = invocation.tool_name.clone();
        let tool_namespace = invocation.tool_namespace.clone();
        let call_id_owned = invocation.call_id.clone();
        let otel = invocation.turn.session_telemetry.clone();
        let payload_for_response = invocation.payload.clone();
        let log_payload = payload_for_response.log_payload();
        let permissions = invocation.turn.effective_permissions();
        let metric_tags = [
            (
                "sandbox",
                sandbox_tag(
                    &permissions.sandbox_policy,
                    permissions.windows_sandbox_level,
                ),
            ),
            (
                "sandbox_policy",
                sandbox_policy_tag(&permissions.sandbox_policy),
            ),
        ];
        let (mcp_server, mcp_server_origin) = match &invocation.payload {
            ToolPayload::Mcp { server, .. } => {
                let manager = invocation
                    .session
                    .services
                    .mcp_connection_manager
                    .read()
                    .await;
                let origin = manager.server_origin(server).map(str::to_owned);
                (Some(server.clone()), origin)
            }
            _ => (None, None),
        };
        let mcp_server_ref = mcp_server.as_deref();
        let mcp_server_origin_ref = mcp_server_origin.as_deref();

        {
            let mut active = invocation.session.active_turn.lock().await;
            if let Some(active_turn) = active.as_mut() {
                let mut turn_state = active_turn.turn_state.lock().await;
                turn_state.tool_calls = turn_state.tool_calls.saturating_add(1);
            }
        }

        let handler = match self.handler(tool_name.as_ref(), tool_namespace.as_deref()) {
            Some(handler) => handler,
            None => {
                let message = unsupported_tool_call_message(
                    &invocation.payload,
                    tool_name.as_ref(),
                    tool_namespace.as_deref(),
                );
                otel.tool_result_with_tags(
                    tool_name.as_ref(),
                    &call_id_owned,
                    log_payload.as_ref(),
                    Duration::ZERO,
                    /*success*/ false,
                    &message,
                    &metric_tags,
                    mcp_server_ref,
                    mcp_server_origin_ref,
                );
                return Err(FunctionCallError::RespondToModel(message));
            }
        };

        if !handler.matches_kind(&invocation.payload) {
            let message = format!("tool {tool_name} invoked with incompatible payload");
            otel.tool_result_with_tags(
                tool_name.as_ref(),
                &call_id_owned,
                log_payload.as_ref(),
                Duration::ZERO,
                /*success*/ false,
                &message,
                &metric_tags,
                mcp_server_ref,
                mcp_server_origin_ref,
            );
            return Err(FunctionCallError::Fatal(message));
        }

        if let Some(pre_tool_use_payload) = handler.pre_tool_use_payload(&invocation)
            && let Some(reason) = run_pre_tool_use_hooks(
                &invocation.session,
                &invocation.turn,
                invocation.call_id.clone(),
                pre_tool_use_payload.command.clone(),
            )
            .await
        {
            return Err(FunctionCallError::RespondToModel(format!(
                "Command blocked by PreToolUse hook: {reason}. Command: {}",
                pre_tool_use_payload.command
            )));
        }

        // Workspace checkpoints belong to the turn boundary, not this dispatch future. Ordered
        // transcript commit remains a batch barrier, so awaiting a recursive capture here can
        // withhold otherwise completed tool results behind unrelated calls.
        let is_mutating = handler.is_mutating(&invocation).await;
        let response_cell = tokio::sync::Mutex::new(None);
        let invocation_for_tool = invocation.clone();

        let result = otel
            .log_tool_result_with_tags(
                tool_name.as_ref(),
                &call_id_owned,
                log_payload.as_ref(),
                &metric_tags,
                mcp_server_ref,
                mcp_server_origin_ref,
                || {
                    let handler = handler.clone();
                    let response_cell = &response_cell;
                    async move {
                        if is_mutating {
                            tracing::trace!("waiting for tool gate");
                            invocation_for_tool.turn.tool_call_gate.wait_ready().await;
                            tracing::trace!("tool gate released");
                        }
                        let handled = match preparation {
                            Some(mut preparation) => {
                                if let Some(error) = preparation.take_rejection() {
                                    Err(error)
                                } else {
                                    handler
                                        .handle_prepared_any(invocation_for_tool, preparation)
                                        .await
                                }
                            }
                            None => handler.handle_any(invocation_for_tool).await,
                        };
                        match handled {
                            Ok(result) => {
                                let preview = result.result.log_preview();
                                let success = result.result.success_for_logging();
                                let mut guard = response_cell.lock().await;
                                *guard = Some(result);
                                Ok((preview, success))
                            }
                            Err(err) => Err(err),
                        }
                    }
                },
            )
            .await;
        let success = match &result {
            Ok((_, success)) => *success,
            Err(_) => false,
        };
        emit_metric_for_tool_read(&invocation, success).await;
        let post_tool_use_payload = if success {
            let guard = response_cell.lock().await;
            guard.as_ref().and_then(|result| {
                handler.post_tool_use_payload(
                    &result.call_id,
                    &result.payload,
                    result.result.as_ref(),
                )
            })
        } else {
            None
        };
        let post_tool_use_outcome = if let Some(post_tool_use_payload) = post_tool_use_payload {
            Some(
                run_post_tool_use_hooks(
                    &invocation.session,
                    &invocation.turn,
                    invocation.call_id.clone(),
                    post_tool_use_payload.command,
                    post_tool_use_payload.tool_response,
                )
                .await,
            )
        } else {
            None
        };
        if let Some(outcome) = &post_tool_use_outcome {
            record_additional_contexts(
                &invocation.session,
                &invocation.turn,
                outcome.additional_contexts.clone(),
            )
            .await;

            let replacement_text = if outcome.should_stop {
                Some(
                    outcome
                        .feedback_message
                        .clone()
                        .or_else(|| outcome.stop_reason.clone())
                        .unwrap_or_else(|| "PostToolUse hook stopped execution".to_string()),
                )
            } else {
                outcome.feedback_message.clone()
            };
            if let Some(replacement_text) = replacement_text {
                let mut guard = response_cell.lock().await;
                if let Some(result) = guard.as_mut() {
                    result.result = Box::new(FunctionToolOutput::from_text(
                        replacement_text,
                        /*success*/ None,
                    ));
                }
            }
        }

        match result {
            Ok(_) => {
                let mut guard = response_cell.lock().await;
                let result = guard.take().ok_or_else(|| {
                    FunctionCallError::Fatal("tool produced no output".to_string())
                })?;
                Ok(result)
            }
            Err(err) => Err(err),
        }
    }

    pub(crate) async fn prepare_for(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolPreparation, FunctionCallError> {
        let Some(handler) = self.handler(
            invocation.tool_name.as_ref(),
            invocation.tool_namespace.as_deref(),
        ) else {
            return Ok(ToolPreparation::new(ToolEffects::unknown_write()));
        };
        if !handler.matches_kind(&invocation.payload) {
            return Ok(ToolPreparation::new(ToolEffects::unknown_write()));
        }
        Ok(match handler.prepare(invocation).await {
            Ok(preparation) => preparation,
            Err(error) => ToolPreparation::rejected(error),
        })
    }
}

pub struct ToolRegistryBuilder {
    handlers: HashMap<String, Arc<dyn AnyToolHandler>>,
    specs: Vec<ConfiguredToolSpec>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            specs: Vec::new(),
        }
    }

    pub fn push_spec(&mut self, spec: ToolSpec) {
        self.push_spec_with_parallel_support(spec, /*supports_parallel_tool_calls*/ false);
    }

    pub fn push_spec_with_parallel_support(
        &mut self,
        spec: ToolSpec,
        supports_parallel_tool_calls: bool,
    ) {
        self.specs
            .push(ConfiguredToolSpec::new(spec, supports_parallel_tool_calls));
    }

    pub fn register_handler<H>(&mut self, name: impl Into<String>, handler: Arc<H>)
    where
        H: ToolHandler + 'static,
    {
        let name = name.into();
        let handler: Arc<dyn AnyToolHandler> = handler;
        if self
            .handlers
            .insert(name.clone(), handler.clone())
            .is_some()
        {
            warn!("overwriting handler for tool {name}");
        }
    }

    // TODO(jif) for dynamic tools.
    // pub fn register_many<I>(&mut self, names: I, handler: Arc<dyn ToolHandler>)
    // where
    //     I: IntoIterator,
    //     I::Item: Into<String>,
    // {
    //     for name in names {
    //         let name = name.into();
    //         if self
    //             .handlers
    //             .insert(name.clone(), handler.clone())
    //             .is_some()
    //         {
    //             warn!("overwriting handler for tool {name}");
    //         }
    //     }
    // }

    pub fn build(self) -> (Vec<ConfiguredToolSpec>, ToolRegistry) {
        let registry = ToolRegistry::new(self.handlers);
        (self.specs, registry)
    }
}

fn unsupported_tool_call_message(
    payload: &ToolPayload,
    tool_name: &str,
    namespace: Option<&str>,
) -> String {
    let tool_name = tool_handler_key(tool_name, namespace);
    match payload {
        ToolPayload::Custom { .. } => format!("unsupported custom tool call: {tool_name}"),
        _ => format!("unsupported call: {tool_name}"),
    }
}

fn sandbox_policy_tag(policy: &SandboxPolicy) -> &'static str {
    match policy {
        SandboxPolicy::ReadOnly { .. } => "read-only",
        SandboxPolicy::WorkspaceWrite { .. } => "workspace-write",
        SandboxPolicy::DangerFullAccess => "danger-full-access",
        SandboxPolicy::ExternalSandbox { .. } => "external-sandbox",
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
