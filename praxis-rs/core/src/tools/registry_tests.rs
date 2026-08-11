use super::*;
use async_trait::async_trait;
use pretty_assertions::assert_eq;

struct TestHandler;

struct MutatingTestHandler;

#[async_trait]
impl ToolHandler for TestHandler {
    type Output = crate::tools::context::FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, _invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        unreachable!("test handler should not be invoked")
    }
}

#[async_trait]
impl ToolHandler for MutatingTestHandler {
    type Output = crate::tools::context::FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        true
    }

    async fn handle(&self, _invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        Ok(crate::tools::context::FunctionToolOutput::from_text(
            "done".to_string(),
            Some(true),
        ))
    }
}

#[tokio::test]
async fn registry_does_not_own_workspace_checkpoint_persistence() {
    let (session, mut turn) = crate::praxis::make_session_and_context().await;
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace.path().join("tracked.txt"), b"content").expect("tracked file");
    turn.cwd =
        praxis_utils_absolute_path::AbsolutePathBuf::try_from(workspace.path().to_path_buf())
            .expect("absolute workspace path");
    let session = Arc::new(session);
    let turn = Arc::new(turn);
    let registry = ToolRegistry::new(HashMap::from([(
        "mutating".to_string(),
        Arc::new(MutatingTestHandler) as Arc<dyn AnyToolHandler>,
    )]));

    registry
        .dispatch_any(ToolInvocation {
            session: Arc::clone(&session),
            turn,
            tracker: Arc::new(tokio::sync::Mutex::new(
                crate::turn_diff_tracker::TurnDiffTracker::new(),
            )),
            call_id: "call-mutating".to_string(),
            tool_name: "mutating".to_string(),
            tool_namespace: None,
            payload: crate::tools::context::ToolPayload::Function {
                arguments: "{}".to_string(),
            },
        })
        .await
        .expect("mutating tool result");

    assert!(
        session
            .clone_history()
            .await
            .raw_items()
            .iter()
            .all(|item| !matches!(
                item,
                praxis_protocol::models::ResponseItem::WorkspaceCheckpoint { .. }
            ))
    );
}

#[test]
fn handler_looks_up_namespaced_aliases_explicitly() {
    let plain_handler = Arc::new(TestHandler) as Arc<dyn AnyToolHandler>;
    let namespaced_handler = Arc::new(TestHandler) as Arc<dyn AnyToolHandler>;
    let namespace = "mcp__praxis_apps__gmail";
    let tool_name = "gmail_get_recent_emails";
    let namespaced_name = tool_handler_key(tool_name, Some(namespace));
    let registry = ToolRegistry::new(HashMap::from([
        (tool_name.to_string(), Arc::clone(&plain_handler)),
        (namespaced_name, Arc::clone(&namespaced_handler)),
    ]));

    let plain = registry.handler(tool_name, /*namespace*/ None);
    let namespaced = registry.handler(tool_name, Some(namespace));
    let missing_namespaced = registry.handler(tool_name, Some("mcp__praxis_apps__calendar"));

    assert_eq!(plain.is_some(), true);
    assert_eq!(namespaced.is_some(), true);
    assert_eq!(missing_namespaced.is_none(), true);
    assert!(
        plain
            .as_ref()
            .is_some_and(|handler| Arc::ptr_eq(handler, &plain_handler))
    );
    assert!(
        namespaced
            .as_ref()
            .is_some_and(|handler| Arc::ptr_eq(handler, &namespaced_handler))
    );
}
