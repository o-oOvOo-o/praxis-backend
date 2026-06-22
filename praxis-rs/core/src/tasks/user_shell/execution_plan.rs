use std::path::PathBuf;

use praxis_protocol::parse_command::ParsedCommand;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_sandboxing::SandboxType;
use praxis_shell_command::parse_command::parse_command;
use uuid::Uuid;

use super::types::USER_SHELL_TIMEOUT_MS;
use crate::exec::ExecCapturePolicy;
use crate::exec::StdoutStream;
use crate::exec_env::create_env;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::sandboxing::ExecRequest;
use crate::tools::runtimes::maybe_wrap_shell_lc_with_snapshot;

pub(super) struct UserShellExecutionPlan {
    pub(super) call_id: String,
    pub(super) raw_command: String,
    pub(super) display_command: Vec<String>,
    pub(super) exec_command: Vec<String>,
    pub(super) cwd: PathBuf,
    pub(super) parsed_cmd: Vec<ParsedCommand>,
}

impl UserShellExecutionPlan {
    pub(super) fn build(session: &Session, turn_context: &TurnContext, command: String) -> Self {
        let use_login_shell = true;
        let session_shell = session.user_shell();
        let display_command = session_shell.derive_exec_args(&command, use_login_shell);
        let exec_command = maybe_wrap_shell_lc_with_snapshot(
            &display_command,
            session_shell.as_ref(),
            turn_context.cwd.as_path(),
            &turn_context.shell_environment_policy.r#set,
        );

        Self {
            call_id: Uuid::new_v4().to_string(),
            raw_command: command,
            parsed_cmd: parse_command(&display_command),
            cwd: turn_context.cwd.to_path_buf(),
            display_command,
            exec_command,
        }
    }

    pub(super) fn exec_request(
        &self,
        session: &Session,
        turn_context: &TurnContext,
    ) -> ExecRequest {
        let sandbox_policy = SandboxPolicy::DangerFullAccess;
        let permissions = turn_context.effective_permissions();
        ExecRequest {
            command: self.exec_command.clone(),
            cwd: self.cwd.clone(),
            env: create_env(
                &turn_context.shell_environment_policy,
                Some(session.conversation_id),
            ),
            network: turn_context.network.clone(),
            expiration: USER_SHELL_TIMEOUT_MS.into(),
            capture_policy: ExecCapturePolicy::ShellTool,
            sandbox: SandboxType::None,
            windows_sandbox_level: permissions.windows_sandbox_level,
            windows_sandbox_private_desktop: turn_context
                .config
                .permissions
                .windows_sandbox_private_desktop,
            sandbox_policy: sandbox_policy.clone(),
            file_system_sandbox_policy: FileSystemSandboxPolicy::from(&sandbox_policy),
            network_sandbox_policy: NetworkSandboxPolicy::from(&sandbox_policy),
            windows_restricted_token_filesystem_overlay: None,
            raw_output_spool: false,
            arg0: None,
        }
    }

    pub(super) fn stdout_stream(
        &self,
        session: &Session,
        turn_context: &TurnContext,
    ) -> Option<StdoutStream> {
        Some(StdoutStream {
            sub_id: turn_context.sub_id.clone(),
            call_id: self.call_id.clone(),
            tx_event: session.get_tx_event(),
        })
    }
}
