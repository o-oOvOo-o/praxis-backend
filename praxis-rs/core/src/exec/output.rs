use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use uuid::Uuid;

use crate::error::PraxisErr;
use crate::error::Result;
use crate::text_encoding::bytes_to_string_smart;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecCommandOutputDeltaEvent;
use praxis_protocol::protocol::ExecOutputStream;
use praxis_utils_pty::process_group::kill_child_process_group;

use super::EXIT_CODE_SIGNAL_BASE;
use super::ExecCapturePolicy;
use super::ExecExpiration;
use super::MAX_EXEC_OUTPUT_DELTAS_PER_CALL;
use super::SIGKILL_CODE;
use super::StdoutStream;
use super::TIMEOUT_CODE;
use super::synthetic_exit_status;

const READ_CHUNK_SIZE: usize = 8192;
const AGGREGATE_BUFFER_INITIAL_CAPACITY: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub struct StreamOutput<T: Clone> {
    pub text: T,
    pub truncated_after_lines: Option<u32>,
}

#[derive(Debug)]
pub(super) struct RawExecToolCallOutput {
    pub exit_status: ExitStatus,
    pub stdout: StreamOutput<Vec<u8>>,
    pub stderr: StreamOutput<Vec<u8>>,
    pub aggregated_output: StreamOutput<Vec<u8>>,
    pub timed_out: bool,
    pub raw_output_spool: Option<ExecOutputSpool>,
}

impl StreamOutput<String> {
    pub fn new(text: String) -> Self {
        Self {
            text,
            truncated_after_lines: None,
        }
    }
}

impl StreamOutput<Vec<u8>> {
    pub fn from_utf8_lossy(&self) -> StreamOutput<String> {
        StreamOutput {
            text: bytes_to_string_smart(&self.text),
            truncated_after_lines: self.truncated_after_lines,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExecStreamSpool {
    pub path: PathBuf,
    pub bytes: usize,
}

#[derive(Clone, Debug)]
pub struct ExecOutputSpool {
    pub stdout: Option<ExecStreamSpool>,
    pub stderr: Option<ExecStreamSpool>,
}

impl ExecOutputSpool {
    pub fn from_streams(
        stdout: Option<ExecStreamSpool>,
        stderr: Option<ExecStreamSpool>,
    ) -> Option<Self> {
        let spool = Self { stdout, stderr };
        (!spool.is_empty()).then_some(spool)
    }

    pub fn is_empty(&self) -> bool {
        self.total_bytes() == 0
    }

    pub fn total_bytes(&self) -> usize {
        self.stdout
            .as_ref()
            .map_or(0, |stream| stream.bytes)
            .saturating_add(self.stderr.as_ref().map_or(0, |stream| stream.bytes))
    }

    pub async fn cleanup(&self) {
        if let Some(stdout) = &self.stdout {
            let _ = tokio::fs::remove_file(stdout.path.as_path()).await;
        }
        if let Some(stderr) = &self.stderr {
            let _ = tokio::fs::remove_file(stderr.path.as_path()).await;
        }
    }
}

pub(super) struct ReadOutputResult {
    pub(super) output: StreamOutput<Vec<u8>>,
    pub(super) spool: Option<ExecStreamSpool>,
}

struct OutputSpoolWriter {
    path: PathBuf,
    file: tokio::fs::File,
    bytes: usize,
}

impl OutputSpoolWriter {
    async fn create(is_stderr: bool) -> Option<Self> {
        let stream_name = if is_stderr { "stderr" } else { "stdout" };
        let path = std::env::temp_dir().join(format!(
            "praxis-exec-output-{}-{stream_name}.log",
            Uuid::new_v4()
        ));
        match tokio::fs::File::create(path.as_path()).await {
            Ok(file) => Some(Self {
                path,
                file,
                bytes: 0,
            }),
            Err(err) => {
                tracing::warn!("failed to create exec output spool file: {err}");
                None
            }
        }
    }

    async fn write_chunk(&mut self, chunk: &[u8]) -> io::Result<()> {
        self.file.write_all(chunk).await?;
        self.bytes = self.bytes.saturating_add(chunk.len());
        Ok(())
    }

    async fn finish(mut self) -> Option<ExecStreamSpool> {
        if let Err(err) = self.file.flush().await {
            tracing::warn!("failed to flush exec output spool file: {err}");
            let _ = tokio::fs::remove_file(self.path.as_path()).await;
            return None;
        }
        if self.bytes == 0 {
            let _ = tokio::fs::remove_file(self.path.as_path()).await;
            return None;
        }
        Some(ExecStreamSpool {
            path: self.path,
            bytes: self.bytes,
        })
    }
}

pub(super) async fn write_capture_output_spool(
    stdout: &[u8],
    stderr: &[u8],
) -> Option<ExecOutputSpool> {
    async fn write_stream(bytes: &[u8], is_stderr: bool) -> Option<ExecStreamSpool> {
        if bytes.is_empty() {
            return None;
        }
        let mut spool = OutputSpoolWriter::create(is_stderr).await?;
        if let Err(err) = spool.write_chunk(bytes).await {
            tracing::warn!("failed to write exec output capture spool: {err}");
            let _ = tokio::fs::remove_file(spool.path.as_path()).await;
            return None;
        }
        spool.finish().await
    }

    let stdout = write_stream(stdout, false).await;
    let stderr = write_stream(stderr, true).await;
    ExecOutputSpool::from_streams(stdout, stderr)
}

#[inline]
fn append_capped(dst: &mut Vec<u8>, src: &[u8], max_bytes: usize) {
    if dst.len() >= max_bytes {
        return;
    }
    let remaining = max_bytes.saturating_sub(dst.len());
    let take = remaining.min(src.len());
    dst.extend_from_slice(&src[..take]);
}

pub(super) fn aggregate_output(
    stdout: &StreamOutput<Vec<u8>>,
    stderr: &StreamOutput<Vec<u8>>,
    max_bytes: Option<usize>,
) -> StreamOutput<Vec<u8>> {
    let Some(max_bytes) = max_bytes else {
        let total_len = stdout.text.len().saturating_add(stderr.text.len());
        let mut aggregated = Vec::with_capacity(total_len);
        aggregated.extend_from_slice(&stdout.text);
        aggregated.extend_from_slice(&stderr.text);
        return StreamOutput {
            text: aggregated,
            truncated_after_lines: None,
        };
    };

    let total_len = stdout.text.len().saturating_add(stderr.text.len());
    let mut aggregated = Vec::with_capacity(total_len.min(max_bytes));

    if total_len <= max_bytes {
        aggregated.extend_from_slice(&stdout.text);
        aggregated.extend_from_slice(&stderr.text);
        return StreamOutput {
            text: aggregated,
            truncated_after_lines: None,
        };
    }

    // Under contention, reserve 1/3 for stdout and 2/3 for stderr; rebalance unused stderr to stdout.
    let want_stdout = stdout.text.len().min(max_bytes / 3);
    let want_stderr = stderr.text.len();
    let stderr_take = want_stderr.min(max_bytes.saturating_sub(want_stdout));
    let remaining = max_bytes.saturating_sub(want_stdout + stderr_take);
    let stdout_take = want_stdout + remaining.min(stdout.text.len().saturating_sub(want_stdout));

    aggregated.extend_from_slice(&stdout.text[..stdout_take]);
    aggregated.extend_from_slice(&stderr.text[..stderr_take]);

    StreamOutput {
        text: aggregated,
        truncated_after_lines: None,
    }
}

#[derive(Clone, Debug)]
pub struct ExecToolCallOutput {
    pub exit_code: i32,
    pub stdout: StreamOutput<String>,
    pub stderr: StreamOutput<String>,
    pub aggregated_output: StreamOutput<String>,
    pub model_output: Option<StreamOutput<String>>,
    pub duration: Duration,
    pub timed_out: bool,
    /// AgentOS artifact containing the full raw command output, when the
    /// command was executed through the managed execution path.  Tool output
    /// formatters use this to keep huge logs out of the model context.
    pub agent_os_artifact_id: Option<String>,
    /// Temporary raw stdout/stderr spool files produced by the exec reader.
    /// Managed runtimes consume these into AgentOS artifacts before formatting.
    pub raw_output_spool: Option<ExecOutputSpool>,
}

impl Default for ExecToolCallOutput {
    fn default() -> Self {
        Self {
            exit_code: 0,
            stdout: StreamOutput::new(String::new()),
            stderr: StreamOutput::new(String::new()),
            aggregated_output: StreamOutput::new(String::new()),
            model_output: None,
            duration: Duration::ZERO,
            timed_out: false,
            agent_os_artifact_id: None,
            raw_output_spool: None,
        }
    }
}

/// Consumes the output of a child process according to the configured capture
/// policy.
pub(super) async fn consume_output(
    mut child: Child,
    expiration: ExecExpiration,
    capture_policy: ExecCapturePolicy,
    raw_output_spool: bool,
    stdout_stream: Option<StdoutStream>,
) -> Result<RawExecToolCallOutput> {
    // Both stdout and stderr were configured with `Stdio::piped()`
    // above, therefore `take()` should normally return `Some`.  If it doesn't
    // we treat it as an exceptional I/O error

    let stdout_reader = child.stdout.take().ok_or_else(|| {
        PraxisErr::Io(io::Error::other(
            "stdout pipe was unexpectedly not available",
        ))
    })?;
    let stderr_reader = child.stderr.take().ok_or_else(|| {
        PraxisErr::Io(io::Error::other(
            "stderr pipe was unexpectedly not available",
        ))
    })?;

    let retained_bytes_cap = capture_policy.retained_bytes_cap();
    let stdout_handle = tokio::spawn(read_output(
        BufReader::new(stdout_reader),
        stdout_stream.clone(),
        /*is_stderr*/ false,
        retained_bytes_cap,
        raw_output_spool,
    ));
    let stderr_handle = tokio::spawn(read_output(
        BufReader::new(stderr_reader),
        stdout_stream.clone(),
        /*is_stderr*/ true,
        retained_bytes_cap,
        raw_output_spool,
    ));

    let expiration_wait = async {
        if capture_policy.uses_expiration() {
            expiration.wait().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::pin!(expiration_wait);
    let (exit_status, timed_out) = tokio::select! {
        status_result = child.wait() => {
            let exit_status = status_result?;
            (exit_status, false)
        }
        _ = &mut expiration_wait => {
            kill_child_process_group(&mut child)?;
            child.start_kill()?;
            (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + TIMEOUT_CODE), true)
        }
        _ = tokio::signal::ctrl_c() => {
            kill_child_process_group(&mut child)?;
            child.start_kill()?;
            (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + SIGKILL_CODE), false)
        }
    };

    // We need mutable bindings so we can `abort()` them on timeout.
    use tokio::task::JoinHandle;

    async fn await_output(
        handle: &mut JoinHandle<std::io::Result<ReadOutputResult>>,
        timeout: Duration,
    ) -> std::io::Result<ReadOutputResult> {
        match tokio::time::timeout(timeout, &mut *handle).await {
            Ok(join_res) => match join_res {
                Ok(io_res) => io_res,
                Err(join_err) => Err(std::io::Error::other(join_err)),
            },
            Err(_elapsed) => {
                // Timeout: abort the task to avoid hanging on open pipes.
                handle.abort();
                Ok(ReadOutputResult {
                    output: StreamOutput {
                        text: Vec::new(),
                        truncated_after_lines: None,
                    },
                    spool: None,
                })
            }
        }
    }

    let mut stdout_handle = stdout_handle;
    let mut stderr_handle = stderr_handle;

    let stdout_result = await_output(&mut stdout_handle, capture_policy.io_drain_timeout()).await?;
    let stderr_result = await_output(&mut stderr_handle, capture_policy.io_drain_timeout()).await?;
    let raw_output_spool =
        ExecOutputSpool::from_streams(stdout_result.spool.clone(), stderr_result.spool.clone());
    let stdout = stdout_result.output;
    let stderr = stderr_result.output;
    let aggregated_output = aggregate_output(&stdout, &stderr, retained_bytes_cap);

    Ok(RawExecToolCallOutput {
        exit_status,
        stdout,
        stderr,
        aggregated_output,
        timed_out,
        raw_output_spool,
    })
}

pub(super) async fn read_output<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    stream: Option<StdoutStream>,
    is_stderr: bool,
    max_bytes: Option<usize>,
    raw_output_spool: bool,
) -> io::Result<ReadOutputResult> {
    let mut buf = Vec::with_capacity(
        max_bytes.map_or(AGGREGATE_BUFFER_INITIAL_CAPACITY, |max_bytes| {
            AGGREGATE_BUFFER_INITIAL_CAPACITY.min(max_bytes)
        }),
    );
    let mut tmp = [0u8; READ_CHUNK_SIZE];
    let mut emitted_deltas: usize = 0;
    let mut spool = if raw_output_spool {
        OutputSpoolWriter::create(is_stderr).await
    } else {
        None
    };

    loop {
        let n = reader.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        let chunk = &tmp[..n];

        if let Some(spool_writer) = spool.as_mut()
            && let Err(err) = spool_writer.write_chunk(chunk).await
        {
            tracing::warn!("failed to write exec output spool: {err}");
            let path = spool_writer.path.clone();
            spool = None;
            let _ = tokio::fs::remove_file(path.as_path()).await;
        }

        if let Some(stream) = &stream
            && emitted_deltas < MAX_EXEC_OUTPUT_DELTAS_PER_CALL
        {
            let chunk = chunk.to_vec();
            let msg = EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
                call_id: stream.call_id.clone(),
                stream: if is_stderr {
                    ExecOutputStream::Stderr
                } else {
                    ExecOutputStream::Stdout
                },
                chunk,
            });
            let event = Event {
                id: stream.sub_id.clone(),
                msg,
            };
            #[allow(clippy::let_unit_value)]
            let _ = stream.tx_event.send(event).await;
            emitted_deltas += 1;
        }

        if let Some(max_bytes) = max_bytes {
            append_capped(&mut buf, chunk, max_bytes);
        } else {
            buf.extend_from_slice(chunk);
        }
        // Continue reading to EOF to avoid back-pressure
    }

    let spool = match spool {
        Some(spool) => spool.finish().await,
        None => None,
    };
    Ok(ReadOutputResult {
        output: StreamOutput {
            text: buf,
            truncated_after_lines: None,
        },
        spool,
    })
}
