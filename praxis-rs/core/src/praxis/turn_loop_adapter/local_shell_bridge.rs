use praxis_protocol::models::LocalShellAction;
use praxis_protocol::models::LocalShellExecAction;
use praxis_protocol::models::ShellToolCallParams;
use serde_json::json;

pub(super) fn params_to_json(params: &ShellToolCallParams) -> String {
    json!({
        "command": params.command.clone(),
        "workdir": params.workdir.clone(),
        "timeout_ms": params.timeout_ms,
        "sandbox_permissions": params.sandbox_permissions.clone(),
        "prefix_rule": params.prefix_rule.clone(),
        "additional_permissions": params.additional_permissions.clone(),
        "justification": params.justification.clone(),
    })
    .to_string()
}

enum ShellArgumentsProjection {
    Parsed(ShellToolCallParams),
    Invalid,
}

pub(super) fn exec_action_from_arguments(arguments: &str) -> LocalShellAction {
    match shell_arguments_projection(arguments) {
        ShellArgumentsProjection::Parsed(params) => LocalShellAction::Exec(LocalShellExecAction {
            command: params.command,
            timeout_ms: params.timeout_ms,
            working_directory: params.workdir,
            env: None,
            user: None,
        }),
        ShellArgumentsProjection::Invalid => LocalShellAction::Exec(LocalShellExecAction {
            command: Vec::new(),
            timeout_ms: None,
            working_directory: None,
            env: None,
            user: None,
        }),
    }
}

fn shell_arguments_projection(arguments: &str) -> ShellArgumentsProjection {
    serde_json::from_str::<ShellToolCallParams>(arguments).map_or(
        ShellArgumentsProjection::Invalid,
        ShellArgumentsProjection::Parsed,
    )
}
