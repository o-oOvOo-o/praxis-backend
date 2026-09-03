use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::PoisonError;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::FunctionCallOutputContentItem;
use crate::runtime::DEFAULT_EXEC_YIELD_TIME_MS;
use crate::runtime::ExecuteRequest;
use crate::runtime::RuntimeCommand;
use crate::runtime::RuntimeEvent;
use crate::runtime::RuntimeResponse;
use crate::runtime::StoredValues;
use crate::runtime::TurnMessage;
use crate::runtime::WaitRequest;
use crate::runtime::spawn_runtime;

#[async_trait]
pub trait CodeModeTurnHost: Send + Sync {
    async fn invoke_tool(
        &self,
        tool_name: String,
        input: Option<JsonValue>,
        cancellation_token: CancellationToken,
    ) -> Result<JsonValue, String>;

    async fn notify(&self, call_id: String, cell_id: String, text: String) -> Result<(), String>;
}

#[derive(Clone)]
struct SessionHandle {
    control_tx: mpsc::Sender<SessionControlCommand>,
    runtime_tx: mpsc::Sender<RuntimeCommand>,
    cancellation_token: CancellationToken,
}

struct Inner {
    stored_values: Mutex<StoredValues>,
    sessions: Mutex<HashMap<String, SessionHandle>>,
    turn_message_tx: mpsc::Sender<TurnMessage>,
    turn_message_rx: Arc<Mutex<mpsc::Receiver<TurnMessage>>>,
    next_cell_id: AtomicU64,
    modules: RwLock<Arc<BTreeMap<String, Arc<str>>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeModule {
    pub specifier: String,
    pub source: String,
}

pub struct CodeModeService {
    inner: Arc<Inner>,
}

impl CodeModeService {
    const CONTROL_CAPACITY: usize = 4;
    const RUNTIME_EVENT_CAPACITY: usize = 256;
    const TOOL_CALL_CONCURRENCY: usize = 32;
    const TURN_MESSAGE_CAPACITY: usize = 128;

    pub fn new() -> Self {
        let (turn_message_tx, turn_message_rx) = mpsc::channel(Self::TURN_MESSAGE_CAPACITY);

        Self {
            inner: Arc::new(Inner {
                stored_values: Mutex::new(StoredValues::default()),
                sessions: Mutex::new(HashMap::new()),
                turn_message_tx,
                turn_message_rx: Arc::new(Mutex::new(turn_message_rx)),
                next_cell_id: AtomicU64::new(1),
                modules: RwLock::new(Arc::new(BTreeMap::new())),
            }),
        }
    }

    pub fn register_module(&self, module: CodeModule) -> Result<(), String> {
        validate_module(&module)?;
        let mut modules = self
            .inner
            .modules
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::make_mut(&mut modules).insert(module.specifier, Arc::from(module.source));
        Ok(())
    }

    pub fn unregister_module(&self, specifier: &str) -> bool {
        let mut modules = self
            .inner
            .modules
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::make_mut(&mut modules).remove(specifier).is_some()
    }

    pub fn module_specifiers(&self) -> Vec<String> {
        self.inner
            .modules
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    pub async fn stored_values(&self) -> StoredValues {
        self.inner.stored_values.lock().await.clone()
    }

    pub async fn replace_stored_values(&self, values: StoredValues) {
        *self.inner.stored_values.lock().await = values;
    }

    pub async fn execute(&self, request: ExecuteRequest) -> Result<RuntimeResponse, String> {
        let cell_id = self
            .inner
            .next_cell_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let (event_tx, event_rx) = mpsc::channel(Self::RUNTIME_EVENT_CAPACITY);
        let module_sources = self
            .inner
            .modules
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let (runtime_tx, runtime_terminate_handle) =
            spawn_runtime(request.clone(), module_sources, event_tx)?;
        let (control_tx, control_rx) = mpsc::channel(Self::CONTROL_CAPACITY);
        let (response_tx, response_rx) = oneshot::channel();
        let cancellation_token = CancellationToken::new();

        self.inner.sessions.lock().await.insert(
            cell_id.clone(),
            SessionHandle {
                control_tx: control_tx.clone(),
                runtime_tx: runtime_tx.clone(),
                cancellation_token: cancellation_token.clone(),
            },
        );

        tokio::spawn(run_session_control(
            Arc::clone(&self.inner),
            SessionControlContext {
                cell_id: cell_id.clone(),
                runtime_tx,
                runtime_terminate_handle,
                cancellation_token,
            },
            event_rx,
            control_rx,
            response_tx,
            request.yield_time_ms.unwrap_or(DEFAULT_EXEC_YIELD_TIME_MS),
        ));

        response_rx
            .await
            .map_err(|_| "exec runtime ended unexpectedly".to_string())
    }

    pub async fn wait(&self, request: WaitRequest) -> Result<RuntimeResponse, String> {
        let cell_id = request.cell_id.clone();
        let handle = self
            .inner
            .sessions
            .lock()
            .await
            .get(&request.cell_id)
            .cloned();
        let Some(handle) = handle else {
            return Ok(missing_cell_response(cell_id));
        };
        let (response_tx, response_rx) = oneshot::channel();
        let control_message = if request.terminate {
            handle.cancellation_token.cancel();
            SessionControlCommand::Terminate { response_tx }
        } else {
            SessionControlCommand::Poll {
                yield_time_ms: request.yield_time_ms,
                response_tx,
            }
        };
        if handle.control_tx.send(control_message).await.is_err() {
            return Ok(missing_cell_response(cell_id));
        }
        match response_rx.await {
            Ok(response) => Ok(response),
            Err(_) => Ok(missing_cell_response(request.cell_id)),
        }
    }

    pub fn start_turn_worker(&self, host: Arc<dyn CodeModeTurnHost>) -> CodeModeTurnWorker {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let inner = Arc::clone(&self.inner);
        let turn_message_rx = Arc::clone(&self.inner.turn_message_rx);
        let host = Arc::new(RwLock::new(host));
        let worker_host = Arc::clone(&host);
        let tool_call_permits = Arc::new(Semaphore::new(Self::TOOL_CALL_CONCURRENCY));

        tokio::spawn(async move {
            'worker: loop {
                let next_message = tokio::select! {
                    _ = &mut shutdown_rx => {
                        terminate_active_sessions(&inner).await;
                        break;
                    },
                    message = async {
                        let mut turn_message_rx = turn_message_rx.lock().await;
                        turn_message_rx.recv().await
                    } => message,
                };
                let Some(next_message) = next_message else {
                    break;
                };
                match next_message {
                    TurnMessage::Notify {
                        cell_id,
                        call_id,
                        text,
                    } => {
                        let host = worker_host
                            .read()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        if let Err(err) = host.notify(call_id, cell_id.clone(), text).await {
                            warn!(
                                "failed to deliver code mode notification for cell {cell_id}: {err}"
                            );
                        }
                    }
                    TurnMessage::ToolCall {
                        cell_id,
                        id,
                        name,
                        input,
                    } => {
                        let cancellation_token = inner
                            .sessions
                            .lock()
                            .await
                            .get(&cell_id)
                            .map(|handle| handle.cancellation_token.child_token());
                        let Some(cancellation_token) = cancellation_token else {
                            continue;
                        };
                        let host = worker_host
                            .read()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        let permit = tokio::select! {
                            _ = &mut shutdown_rx => {
                                terminate_active_sessions(&inner).await;
                                break 'worker;
                            }
                            permit = Arc::clone(&tool_call_permits).acquire_owned() => {
                                let Ok(permit) = permit else {
                                    break 'worker;
                                };
                                permit
                            }
                        };
                        let inner = Arc::clone(&inner);
                        tokio::spawn(async move {
                            let _permit = permit;
                            let response = host.invoke_tool(name, input, cancellation_token).await;
                            let runtime_tx = inner
                                .sessions
                                .lock()
                                .await
                                .get(&cell_id)
                                .map(|handle| handle.runtime_tx.clone());
                            let Some(runtime_tx) = runtime_tx else {
                                return;
                            };
                            let command = match response {
                                Ok(result) => RuntimeCommand::ToolResponse { id, result },
                                Err(error_text) => RuntimeCommand::ToolError { id, error_text },
                            };
                            let _ = runtime_tx.send(command).await;
                        });
                    }
                }
            }
        });

        CodeModeTurnWorker {
            shutdown_tx: Some(shutdown_tx),
            host,
        }
    }
}

async fn terminate_active_sessions(inner: &Inner) {
    let handles = inner
        .sessions
        .lock()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for handle in handles {
        handle.cancellation_token.cancel();
        let (response_tx, _response_rx) = oneshot::channel();
        let _ = handle
            .control_tx
            .send(SessionControlCommand::Terminate { response_tx })
            .await;
    }
}

fn validate_module(module: &CodeModule) -> Result<(), String> {
    let specifier = module.specifier.trim();
    if specifier.is_empty()
        || !specifier.contains(':')
        || specifier.chars().any(char::is_whitespace)
    {
        return Err("code module specifier must be a non-empty namespaced identifier".to_string());
    }
    if specifier == "praxis:runtime" {
        return Err("praxis:runtime is a reserved built-in module".to_string());
    }
    if module.source.trim().is_empty() {
        return Err(format!("code module `{specifier}` has empty source"));
    }
    Ok(())
}

impl Default for CodeModeService {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CodeModeTurnWorker {
    shutdown_tx: Option<oneshot::Sender<()>>,
    host: Arc<RwLock<Arc<dyn CodeModeTurnHost>>>,
}

impl CodeModeTurnWorker {
    pub fn replace_host(&self, host: Arc<dyn CodeModeTurnHost>) {
        *self.host.write().unwrap_or_else(PoisonError::into_inner) = host;
    }
}

impl Drop for CodeModeTurnWorker {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

enum SessionControlCommand {
    Poll {
        yield_time_ms: u64,
        response_tx: oneshot::Sender<RuntimeResponse>,
    },
    Terminate {
        response_tx: oneshot::Sender<RuntimeResponse>,
    },
}

struct PendingResult {
    content_items: Vec<FunctionCallOutputContentItem>,
    stored_values: StoredValues,
    error_text: Option<String>,
}

struct SessionControlContext {
    cell_id: String,
    runtime_tx: mpsc::Sender<RuntimeCommand>,
    runtime_terminate_handle: v8::IsolateHandle,
    cancellation_token: CancellationToken,
}

fn missing_cell_response(cell_id: String) -> RuntimeResponse {
    RuntimeResponse::Result {
        error_text: Some(format!("exec cell {cell_id} not found")),
        cell_id,
        content_items: Vec::new(),
        stored_values: StoredValues::default(),
    }
}

fn pending_result_response(cell_id: &str, result: PendingResult) -> RuntimeResponse {
    RuntimeResponse::Result {
        cell_id: cell_id.to_string(),
        content_items: result.content_items,
        stored_values: result.stored_values,
        error_text: result.error_text,
    }
}

fn send_or_buffer_result(
    cell_id: &str,
    result: PendingResult,
    response_tx: &mut Option<oneshot::Sender<RuntimeResponse>>,
    pending_result: &mut Option<PendingResult>,
) -> bool {
    if let Some(response_tx) = response_tx.take() {
        let _ = response_tx.send(pending_result_response(cell_id, result));
        return true;
    }

    *pending_result = Some(result);
    false
}

async fn run_session_control(
    inner: Arc<Inner>,
    context: SessionControlContext,
    mut event_rx: mpsc::Receiver<RuntimeEvent>,
    mut control_rx: mpsc::Receiver<SessionControlCommand>,
    initial_response_tx: oneshot::Sender<RuntimeResponse>,
    initial_yield_time_ms: u64,
) {
    let SessionControlContext {
        cell_id,
        runtime_tx,
        runtime_terminate_handle,
        cancellation_token,
    } = context;
    let mut content_items = Vec::new();
    let mut pending_result: Option<PendingResult> = None;
    let mut response_tx = Some(initial_response_tx);
    let mut termination_requested = false;
    let mut runtime_closed = false;
    let mut yield_timer: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;

    loop {
        tokio::select! {
            maybe_event = async {
                if runtime_closed {
                    std::future::pending::<Option<RuntimeEvent>>().await
                } else {
                    event_rx.recv().await
                }
            } => {
                let Some(event) = maybe_event else {
                    runtime_closed = true;
                    if termination_requested {
                        if let Some(response_tx) = response_tx.take() {
                            let _ = response_tx.send(RuntimeResponse::Terminated {
                                cell_id: cell_id.clone(),
                                content_items: std::mem::take(&mut content_items),
                            });
                        }
                        break;
                    }
                    if pending_result.is_none() {
                        let result = PendingResult {
                            content_items: std::mem::take(&mut content_items),
                            stored_values: StoredValues::default(),
                            error_text: Some("exec runtime ended unexpectedly".to_string()),
                        };
                        if send_or_buffer_result(
                            &cell_id,
                            result,
                            &mut response_tx,
                            &mut pending_result,
                        ) {
                            break;
                        }
                    }
                    continue;
                };
                match event {
                    RuntimeEvent::Started => {
                        yield_timer = Some(Box::pin(tokio::time::sleep(Duration::from_millis(initial_yield_time_ms))));
                    }
                    RuntimeEvent::ContentItem(item) => {
                        content_items.push(item);
                    }
                    RuntimeEvent::YieldRequested => {
                        yield_timer = None;
                        if let Some(response_tx) = response_tx.take() {
                            let _ = response_tx.send(RuntimeResponse::Yielded {
                                cell_id: cell_id.clone(),
                                content_items: std::mem::take(&mut content_items),
                            });
                        }
                    }
                    RuntimeEvent::Notify { call_id, text } => {
                        let _ = inner.turn_message_tx.send(TurnMessage::Notify {
                            cell_id: cell_id.clone(),
                            call_id,
                            text,
                        }).await;
                    }
                    RuntimeEvent::ToolCall { id, name, input } => {
                        let _ = inner.turn_message_tx.send(TurnMessage::ToolCall {
                            cell_id: cell_id.clone(),
                            id,
                            name,
                            input,
                        }).await;
                    }
                    RuntimeEvent::Result {
                        stored_values,
                        error_text,
                    } => {
                        yield_timer = None;
                        if termination_requested {
                            if let Some(response_tx) = response_tx.take() {
                                let _ = response_tx.send(RuntimeResponse::Terminated {
                                    cell_id: cell_id.clone(),
                                    content_items: std::mem::take(&mut content_items),
                                });
                            }
                            break;
                        }
                        let result = PendingResult {
                            content_items: std::mem::take(&mut content_items),
                            stored_values,
                            error_text,
                        };
                        if send_or_buffer_result(
                            &cell_id,
                            result,
                            &mut response_tx,
                            &mut pending_result,
                        ) {
                            break;
                        }
                    }
                }
            }
            maybe_command = control_rx.recv() => {
                let Some(command) = maybe_command else {
                    break;
                };
                match command {
                    SessionControlCommand::Poll {
                        yield_time_ms,
                        response_tx: next_response_tx,
                    } => {
                        if let Some(result) = pending_result.take() {
                            let _ = next_response_tx.send(pending_result_response(&cell_id, result));
                            break;
                        }
                        response_tx = Some(next_response_tx);
                        yield_timer = Some(Box::pin(tokio::time::sleep(Duration::from_millis(yield_time_ms))));
                    }
                    SessionControlCommand::Terminate { response_tx: next_response_tx } => {
                        if let Some(result) = pending_result.take() {
                            let _ = next_response_tx.send(pending_result_response(&cell_id, result));
                            break;
                        }

                        response_tx = Some(next_response_tx);
                        termination_requested = true;
                        yield_timer = None;
                        cancellation_token.cancel();
                        let _ = runtime_tx.try_send(RuntimeCommand::Terminate);
                        let _ = runtime_terminate_handle.terminate_execution();
                        if runtime_closed {
                            if let Some(response_tx) = response_tx.take() {
                                let _ = response_tx.send(RuntimeResponse::Terminated {
                                    cell_id: cell_id.clone(),
                                    content_items: std::mem::take(&mut content_items),
                                });
                            }
                            break;
                        } else {
                            continue;
                        }
                    }
                }
            }
            _ = async {
                if let Some(yield_timer) = yield_timer.as_mut() {
                    yield_timer.await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                yield_timer = None;
                if let Some(response_tx) = response_tx.take() {
                    let _ = response_tx.send(RuntimeResponse::Yielded {
                        cell_id: cell_id.clone(),
                        content_items: std::mem::take(&mut content_items),
                    });
                }
            }
        }
    }

    cancellation_token.cancel();
    let _ = runtime_tx.try_send(RuntimeCommand::Terminate);
    inner.sessions.lock().await.remove(&cell_id);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::RwLock;
    use std::sync::atomic::AtomicU64;
    use std::time::Duration;

    use pretty_assertions::assert_eq;
    use tokio::sync::Mutex;
    use tokio::sync::mpsc;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::CodeModeService;
    use super::Inner;
    use super::RuntimeCommand;
    use super::RuntimeResponse;
    use super::SessionControlCommand;
    use super::SessionControlContext;
    use super::run_session_control;
    use crate::FunctionCallOutputContentItem;
    use crate::runtime::ExecuteRequest;
    use crate::runtime::RuntimeEvent;
    use crate::runtime::spawn_runtime;

    fn execute_request(source: &str) -> ExecuteRequest {
        ExecuteRequest {
            tool_call_id: "call_1".to_string(),
            enabled_tools: Arc::from([]),
            source: source.to_string(),
            stored_values: StoredValues::default(),
            yield_time_ms: Some(1),
            max_output_tokens: None,
        }
    }

    fn test_inner() -> Arc<Inner> {
        let (turn_message_tx, turn_message_rx) =
            mpsc::channel(CodeModeService::TURN_MESSAGE_CAPACITY);
        Arc::new(Inner {
            stored_values: Mutex::new(StoredValues::default()),
            sessions: Mutex::new(HashMap::new()),
            turn_message_tx,
            turn_message_rx: Arc::new(Mutex::new(turn_message_rx)),
            next_cell_id: AtomicU64::new(1),
            modules: RwLock::new(Arc::new(BTreeMap::new())),
        })
    }

    #[tokio::test]
    async fn synchronous_exit_returns_successfully() {
        let service = CodeModeService::new();

        let response = service
            .execute(ExecuteRequest {
                source: r#"text("before"); exit(); text("after");"#.to_string(),
                yield_time_ms: None,
                ..execute_request("")
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            RuntimeResponse::Result {
                cell_id: "1".to_string(),
                content_items: vec![FunctionCallOutputContentItem::InputText {
                    text: "before".to_string(),
                }],
                stored_values: StoredValues::default(),
                error_text: None,
            }
        );
    }

    #[tokio::test]
    async fn v8_console_is_not_exposed_on_global_this() {
        let service = CodeModeService::new();

        let response = service
            .execute(ExecuteRequest {
                source: r#"text(String(Object.hasOwn(globalThis, "console")));"#.to_string(),
                yield_time_ms: None,
                ..execute_request("")
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            RuntimeResponse::Result {
                cell_id: "1".to_string(),
                content_items: vec![FunctionCallOutputContentItem::InputText {
                    text: "false".to_string(),
                }],
                stored_values: StoredValues::default(),
                error_text: None,
            }
        );
    }

    #[tokio::test]
    async fn output_helpers_return_undefined() {
        let service = CodeModeService::new();

        let response = service
            .execute(ExecuteRequest {
                source: r#"
const returnsUndefined = [
  text("first"),
  image("https://example.com/image.jpg"),
  notify("ping"),
].map((value) => value === undefined);
text(JSON.stringify(returnsUndefined));
"#
                .to_string(),
                yield_time_ms: None,
                ..execute_request("")
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            RuntimeResponse::Result {
                cell_id: "1".to_string(),
                content_items: vec![
                    FunctionCallOutputContentItem::InputText {
                        text: "first".to_string(),
                    },
                    FunctionCallOutputContentItem::InputImage {
                        image_url: "https://example.com/image.jpg".to_string(),
                        detail: None,
                    },
                    FunctionCallOutputContentItem::InputText {
                        text: "[true,true,true]".to_string(),
                    },
                ],
                stored_values: StoredValues::default(),
                error_text: None,
            }
        );
    }

    #[tokio::test]
    async fn terminate_waits_for_runtime_shutdown_before_responding() {
        let inner = test_inner();
        let (event_tx, event_rx) = mpsc::channel(CodeModeService::RUNTIME_EVENT_CAPACITY);
        let (control_tx, control_rx) = mpsc::channel(CodeModeService::CONTROL_CAPACITY);
        let (initial_response_tx, initial_response_rx) = oneshot::channel();
        let (runtime_event_tx, _runtime_event_rx) =
            mpsc::channel(CodeModeService::RUNTIME_EVENT_CAPACITY);
        let (runtime_tx, runtime_terminate_handle) = spawn_runtime(
            ExecuteRequest {
                source: "await new Promise(() => {})".to_string(),
                yield_time_ms: None,
                ..execute_request("")
            },
            Arc::new(BTreeMap::new()),
            runtime_event_tx,
        )
        .unwrap();

        tokio::spawn(run_session_control(
            inner,
            SessionControlContext {
                cell_id: "cell-1".to_string(),
                runtime_tx: runtime_tx.clone(),
                runtime_terminate_handle,
                cancellation_token: CancellationToken::new(),
            },
            event_rx,
            control_rx,
            initial_response_tx,
            /*initial_yield_time_ms*/ 60_000,
        ));

        event_tx.send(RuntimeEvent::Started).await.unwrap();
        event_tx.send(RuntimeEvent::YieldRequested).await.unwrap();
        assert_eq!(
            initial_response_rx.await.unwrap(),
            RuntimeResponse::Yielded {
                cell_id: "cell-1".to_string(),
                content_items: Vec::new(),
            }
        );

        let (terminate_response_tx, terminate_response_rx) = oneshot::channel();
        control_tx
            .send(SessionControlCommand::Terminate {
                response_tx: terminate_response_tx,
            })
            .await
            .unwrap();
        let terminate_response = async { terminate_response_rx.await.unwrap() };
        tokio::pin!(terminate_response);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), terminate_response.as_mut())
                .await
                .is_err()
        );

        drop(event_tx);

        assert_eq!(
            terminate_response.await,
            RuntimeResponse::Terminated {
                cell_id: "cell-1".to_string(),
                content_items: Vec::new(),
            }
        );

        let _ = runtime_tx.try_send(RuntimeCommand::Terminate);
    }
}
