const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const CODEX_HOME_NAMESPACE_ENV_VAR: &str = "CODEX_HOME_NAMESPACE";
const CODEX_SQLITE_HOME_ENV_VAR: &str = "CODEX_SQLITE_HOME";
const CODEX_THREAD_ID_ENV_VAR: &str = "CODEX_THREAD_ID";

const CODEX_STATE_ENV_VARS: &[&str] = &[
    CODEX_HOME_ENV_VAR,
    CODEX_HOME_NAMESPACE_ENV_VAR,
    CODEX_SQLITE_HOME_ENV_VAR,
    CODEX_THREAD_ID_ENV_VAR,
];

const CODEX_HOME_DIRNAME: &str = ".codex";

pub(crate) fn codex_home_dirname() -> &'static str {
    CODEX_HOME_DIRNAME
}

pub fn scrub_external_agent_state_env_for_current_process() {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.
    unsafe {
        for &name in CODEX_STATE_ENV_VARS {
            std::env::remove_var(name);
        }
    }
}

pub fn is_external_agent_state_env_var(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        CODEX_STATE_ENV_VARS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
    } else {
        CODEX_STATE_ENV_VARS.contains(&name)
    }
}
