use praxis_config::types::EnvironmentVariablePattern;
use praxis_config::types::ShellEnvironmentPolicy;
use praxis_config::types::ShellEnvironmentPolicyInherit;
use praxis_protocol::ThreadId;
use std::collections::HashMap;
use std::collections::HashSet;

pub const PRAXIS_THREAD_ID_ENV_VAR: &str = "PRAXIS_THREAD_ID";

const CODEX_STATE_ENV_VARS: &[&str] = &[
    "CODEX_HOME",
    "CODEX_HOME_NAMESPACE",
    "CODEX_SQLITE_HOME",
    "CODEX_THREAD_ID",
];

/// Construct an environment map based on the rules in the specified policy. The
/// resulting map can be passed directly to `Command::envs()` after calling
/// `env_clear()` to ensure no unintended variables are leaked to the spawned
/// process.
///
/// The derivation follows the algorithm documented in the struct-level comment
/// for [`ShellEnvironmentPolicy`].
///
/// `PRAXIS_THREAD_ID` is injected when a thread id is provided, even when
/// `include_only` is set.
///
/// Praxis also strips inherited Codex state variables by default.  Those
/// variables belong to the upstream Codex CLI and must not leak through Praxis
/// into nested shells or child Codex processes.  A command policy may still set
/// them explicitly via `set` when a task intentionally needs to control an
/// upstream Codex process.
pub fn create_env(
    policy: &ShellEnvironmentPolicy,
    thread_id: Option<ThreadId>,
) -> HashMap<String, String> {
    populate_env(std::env::vars(), policy, thread_id)
}

fn is_codex_state_env_var(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        CODEX_STATE_ENV_VARS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
    } else {
        CODEX_STATE_ENV_VARS.contains(&name)
    }
}

fn populate_env<I>(
    vars: I,
    policy: &ShellEnvironmentPolicy,
    thread_id: Option<ThreadId>,
) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    // Step 1 – determine the starting set of variables based on the
    // `inherit` strategy.
    let mut env_map: HashMap<String, String> = match policy.inherit {
        ShellEnvironmentPolicyInherit::All => vars.into_iter().collect(),
        ShellEnvironmentPolicyInherit::None => HashMap::new(),
        ShellEnvironmentPolicyInherit::Core => {
            const CORE_VARS: &[&str] = &[
                "HOME", "LOGNAME", "PATH", "SHELL", "USER", "USERNAME", "TMPDIR", "TEMP", "TMP",
            ];
            let allow: HashSet<&str> = CORE_VARS.iter().copied().collect();
            let is_core_var = |name: &str| {
                if cfg!(target_os = "windows") {
                    CORE_VARS
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(name))
                } else {
                    allow.contains(name)
                }
            };
            vars.into_iter().filter(|(k, _)| is_core_var(k)).collect()
        }
    };

    // Internal helper – does `name` match **any** pattern in `patterns`?
    let matches_any = |name: &str, patterns: &[EnvironmentVariablePattern]| -> bool {
        patterns.iter().any(|pattern| pattern.matches(name))
    };

    // Step 2 – Apply the default exclude if not disabled.
    if !policy.ignore_default_excludes {
        let default_excludes = vec![
            EnvironmentVariablePattern::new_case_insensitive("*KEY*"),
            EnvironmentVariablePattern::new_case_insensitive("*SECRET*"),
            EnvironmentVariablePattern::new_case_insensitive("*TOKEN*"),
        ];
        env_map.retain(|k, _| !matches_any(k, &default_excludes));
    }

    // Step 3 – Apply custom excludes.
    if !policy.exclude.is_empty() {
        env_map.retain(|k, _| !matches_any(k, &policy.exclude));
    }

    // Step 4 – Strip inherited upstream Codex state variables.  These are not
    // secrets, so the KEY/SECRET/TOKEN default filter will not remove them, but
    // they can make nested official Codex processes attach to the wrong thread
    // or write into a stale state database.  Do this before user-provided
    // overrides so an explicit command policy can still opt back in.
    env_map.retain(|k, _| !is_codex_state_env_var(k));

    // Step 5 – Apply user-provided overrides.
    for (key, val) in &policy.r#set {
        env_map.insert(key.clone(), val.clone());
    }

    // Step 6 – If include_only is non-empty, keep *only* the matching vars.
    if !policy.include_only.is_empty() {
        env_map.retain(|k, _| matches_any(k, &policy.include_only));
    }

    // Step 7 – Populate the Praxis thread ID environment variable when provided.
    if let Some(thread_id) = thread_id {
        env_map.insert(PRAXIS_THREAD_ID_ENV_VAR.to_string(), thread_id.to_string());
    }

    env_map
}

#[cfg(test)]
#[path = "exec_env_tests.rs"]
mod tests;
