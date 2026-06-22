use dirs::home_dir;
use std::path::Path;
use std::path::PathBuf;

const PRAXIS_HOME_ENV_VAR: &str = "PRAXIS_HOME";
const PRAXIS_HOME_NAMESPACE_ENV_VAR: &str = "PRAXIS_HOME_NAMESPACE";
const PRAXIS_HOME_DIRNAME: &str = ".praxis";
const UPSTREAM_CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const UPSTREAM_CODEX_HOME_NAMESPACE_ENV_VAR: &str = "CODEX_HOME_NAMESPACE";
const UPSTREAM_CODEX_SQLITE_HOME_ENV_VAR: &str = "CODEX_SQLITE_HOME";
const UPSTREAM_CODEX_THREAD_ID_ENV_VAR: &str = "CODEX_THREAD_ID";
const UPSTREAM_CODEX_STATE_ENV_VARS: &[&str] = &[
    UPSTREAM_CODEX_HOME_ENV_VAR,
    UPSTREAM_CODEX_HOME_NAMESPACE_ENV_VAR,
    UPSTREAM_CODEX_SQLITE_HOME_ENV_VAR,
    UPSTREAM_CODEX_THREAD_ID_ENV_VAR,
];
const UPSTREAM_CODEX_HOME_DIRNAME: &str = ".codex";
const LEGACY_CODEP_HOME_DIRNAME: &str = ".codep";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PraxisHomeNamespace {
    Praxis,
}

impl PraxisHomeNamespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Praxis => "praxis",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "praxis" => Some(Self::Praxis),
            // Codex is an upstream external source, not a Praxis namespace.
            "codex" => None,
            _ => None,
        }
    }

    fn default_dir_name(self) -> &'static str {
        match self {
            Self::Praxis => PRAXIS_HOME_DIRNAME,
        }
    }
}

/// Returns the path to the Praxis configuration directory, which can be
/// specified by the `PRAXIS_HOME` environment variable. If not set, defaults to
/// the Praxis home directory (`~/.praxis`).
///
/// - If `PRAXIS_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `PRAXIS_HOME` is not set, this function does not verify that the
///   directory exists.
pub fn find_praxis_home() -> std::io::Result<PathBuf> {
    let praxis_home_env = active_home_env_override();
    let namespace = current_praxis_home_namespace();
    find_praxis_home_from_env_and_namespace(praxis_home_env.as_ref(), namespace)
}

pub fn current_praxis_home_namespace() -> PraxisHomeNamespace {
    // Do not allow an environment variable or legacy binary name to move
    // Praxis state into any non-Praxis namespace. Codex/config/auth bridging is
    // explicit and read-through; Praxis runtime state always remains under
    // Praxis home.
    PraxisHomeNamespace::Praxis
}

pub fn default_praxis_home_for_namespace(
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    default_home_with_dir_name(namespace.default_dir_name())
}

/// Returns the upstream Codex home used only by explicit read-through bridges.
///
/// This is deliberately not represented as a `PraxisHomeNamespace`: Codex is
/// an external data source, never a namespace where Praxis runtime state,
/// thread indexes, goals, logs, or SQLite databases may be written.
pub fn default_upstream_codex_home() -> std::io::Result<PathBuf> {
    default_home_with_dir_name(UPSTREAM_CODEX_HOME_DIRNAME)
}

/// Returns the legacy Codep home used only for explicit diagnostics/migration.
///
/// Praxis must not auto-rename or auto-import this directory because old
/// Codep profiles commonly contain Codex env wrappers, stale thread ids, and
/// schema-incompatible SQLite state.
pub fn default_legacy_codep_home() -> std::io::Result<PathBuf> {
    default_home_with_dir_name(LEGACY_CODEP_HOME_DIRNAME)
}

fn default_home_with_dir_name(dir_name: &str) -> std::io::Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(dir_name);
    Ok(path)
}

/// Returns the upstream Codex home that Praxis may use for bridged user-level
/// state such as config/auth, or `None` when the current process should remain
/// fully isolated.
///
/// The bridge is enabled only when:
/// - the current namespace resolves to `praxis`
/// - the provided `praxis_home` is the default `~/.praxis` location
///
/// This keeps custom/test homes isolated while allowing the default Praxis UX
/// to inherit Codex config/auth without sharing thread storage.
pub fn upstream_codex_read_through_home(praxis_home: &Path) -> std::io::Result<Option<PathBuf>> {
    let namespace = current_praxis_home_namespace();
    upstream_codex_read_through_home_with_namespace_hint(namespace, praxis_home)
}

pub fn scrub_upstream_codex_state_env_for_current_process() {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.  Keeping these variables in-process is a
    // footgun: profile wrappers can point them at stale Codex/Codep homes and
    // nested child commands can inherit them.
    unsafe {
        for &name in UPSTREAM_CODEX_STATE_ENV_VARS {
            std::env::remove_var(name);
        }
    }
}

pub fn is_upstream_codex_state_env_var(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        UPSTREAM_CODEX_STATE_ENV_VARS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
    } else {
        UPSTREAM_CODEX_STATE_ENV_VARS.contains(&name)
    }
}

pub fn set_process_praxis_home_namespace(namespace: PraxisHomeNamespace) {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.
    unsafe {
        std::env::set_var(PRAXIS_HOME_NAMESPACE_ENV_VAR, namespace.as_str());
    }
}

pub fn set_process_praxis_home_namespace_if_unset_for_current_process() {
    let home_is_explicit = active_home_env_override().is_some();
    let namespace_is_explicit = active_namespace_env_override().as_ref().is_some_and(|env| {
        PraxisHomeNamespace::from_str(env.value.as_str()) == Some(PraxisHomeNamespace::Praxis)
    });
    if home_is_explicit || namespace_is_explicit {
        return;
    }

    set_process_praxis_home_namespace(PraxisHomeNamespace::Praxis);
}

fn find_praxis_home_from_env_and_namespace(
    praxis_home_env: Option<&HomeEnvOverride>,
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    match praxis_home_env {
        Some(env) => {
            let path = PathBuf::from(env.value.as_str());
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "{} points to {:?}, but that path does not exist",
                        env.name, env.value
                    ),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read {} {:?}: {err}", env.name, env.value),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "{} points to {:?}, but that path is not a directory",
                        env.name, env.value
                    ),
                ))
            } else {
                path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize {} {:?}: {err}", env.name, env.value),
                    )
                })
            }
        }
        None => default_praxis_home_for_namespace(namespace),
    }
}

fn upstream_codex_read_through_home_with_namespace_hint(
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    if namespace != PraxisHomeNamespace::Praxis {
        return Ok(None);
    }

    let default_praxis_home = default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis)?;
    if !paths_match(praxis_home, &default_praxis_home) {
        return Ok(None);
    }

    default_upstream_codex_home().map(Some)
}

#[derive(Clone, Debug)]
struct HomeEnvOverride {
    name: &'static str,
    value: String,
}

fn active_home_env_override() -> Option<HomeEnvOverride> {
    // Praxis must not honor CODEX_HOME.  CODEX_HOME belongs to the upstream
    // Codex CLI and can point at an unrelated or stale state database when a
    // user has shell-profile aliases such as codep/codex.  Praxis may read
    // selected Codex config/auth as an explicit read-through bridge, but its
    // own home/state must be controlled only by PRAXIS_HOME.
    env_override(PRAXIS_HOME_ENV_VAR)
}

fn active_namespace_env_override() -> Option<HomeEnvOverride> {
    // Same isolation rule as PRAXIS_HOME: do not let CODEX_HOME_NAMESPACE move
    // Praxis into the upstream Codex namespace.
    env_override(PRAXIS_HOME_NAMESPACE_ENV_VAR)
}

fn env_override(name: &'static str) -> Option<HomeEnvOverride> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| HomeEnvOverride { name, value })
}

fn paths_match(lhs: &Path, rhs: &Path) -> bool {
    if lhs == rhs {
        return true;
    }

    match (lhs.canonicalize(), rhs.canonicalize()) {
        (Ok(lhs), Ok(rhs)) => lhs == rhs,
        _ => false,
    }
}

#[cfg(test)]
fn find_praxis_home_with_namespace_hint(
    praxis_home_env: Option<&str>,
    namespace_env: Option<&str>,
) -> std::io::Result<PathBuf> {
    let namespace = namespace_env
        .and_then(PraxisHomeNamespace::from_str)
        .unwrap_or(PraxisHomeNamespace::Praxis);
    let home_env = praxis_home_env.map(|value| HomeEnvOverride {
        name: PRAXIS_HOME_ENV_VAR,
        value: value.to_string(),
    });
    find_praxis_home_from_env_and_namespace(home_env.as_ref(), namespace)
}

#[cfg(test)]
fn upstream_codex_read_through_home_for_test(
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    upstream_codex_read_through_home_with_namespace_hint(namespace, praxis_home)
}

#[cfg(test)]
mod tests {
    use super::LEGACY_CODEP_HOME_DIRNAME;
    use super::PRAXIS_HOME_DIRNAME;
    use super::PRAXIS_HOME_ENV_VAR;
    use super::PRAXIS_HOME_NAMESPACE_ENV_VAR;
    use super::PraxisHomeNamespace;
    use super::UPSTREAM_CODEX_HOME_DIRNAME;
    use super::default_praxis_home_for_namespace;
    use super::find_praxis_home_with_namespace_hint;
    use super::is_upstream_codex_state_env_var;
    use super::upstream_codex_read_through_home_for_test;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn find_praxis_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-praxis-home");
        let missing_str = missing
            .to_str()
            .expect("missing praxis home path should be valid utf-8");

        let err = find_praxis_home_with_namespace_hint(Some(missing_str), None)
            .expect_err("missing PRAXIS_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains(PRAXIS_HOME_ENV_VAR),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_praxis_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("praxis-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file praxis home path should be valid utf-8");

        let err = find_praxis_home_with_namespace_hint(Some(file_str), None)
            .expect_err("file PRAXIS_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_praxis_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp praxis home path should be valid utf-8");

        let resolved =
            find_praxis_home_with_namespace_hint(Some(temp_str), None).expect("valid PRAXIS_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_praxis_home_without_env_uses_praxis_default_home_dir() {
        let resolved = find_praxis_home_with_namespace_hint(/*praxis_home_env*/ None, None)
            .expect("default PRAXIS_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(PRAXIS_HOME_DIRNAME);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn namespace_env_cannot_switch_praxis_into_codex_home() {
        let resolved =
            find_praxis_home_with_namespace_hint(None, Some("codex")).expect("default PRAXIS_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(PRAXIS_HOME_DIRNAME);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn helper_methods_roundtrip_namespace_metadata() {
        assert_eq!(PraxisHomeNamespace::Praxis.as_str(), "praxis");
        assert_eq!(
            PraxisHomeNamespace::from_str("PrAxIs"),
            Some(PraxisHomeNamespace::Praxis)
        );
        assert_eq!(PraxisHomeNamespace::from_str("codex"), None);
        assert_eq!(PraxisHomeNamespace::from_str("unknown"), None);
        assert_eq!(
            PraxisHomeNamespace::Praxis.default_dir_name(),
            PRAXIS_HOME_DIRNAME
        );
        assert_eq!(PRAXIS_HOME_NAMESPACE_ENV_VAR, "PRAXIS_HOME_NAMESPACE");
        assert_eq!(PRAXIS_HOME_ENV_VAR, "PRAXIS_HOME");
    }

    #[test]
    fn default_home_builder_uses_expected_suffixes() {
        let praxis_home =
            default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis).expect("praxis home");
        let upstream_codex_home =
            super::default_upstream_codex_home().expect("upstream codex home");
        let codep_home = super::default_legacy_codep_home().expect("legacy codep home");
        assert!(praxis_home.ends_with(PRAXIS_HOME_DIRNAME));
        assert!(upstream_codex_home.ends_with(UPSTREAM_CODEX_HOME_DIRNAME));
        assert!(codep_home.ends_with(LEGACY_CODEP_HOME_DIRNAME));
    }

    #[test]
    fn upstream_codex_state_env_vars_are_classified_explicitly() {
        assert!(is_upstream_codex_state_env_var("CODEX_HOME"));
        assert!(is_upstream_codex_state_env_var("CODEX_HOME_NAMESPACE"));
        assert!(is_upstream_codex_state_env_var("CODEX_SQLITE_HOME"));
        assert!(is_upstream_codex_state_env_var("CODEX_THREAD_ID"));
        assert!(!is_upstream_codex_state_env_var("PRAXIS_HOME"));
    }

    #[test]
    fn legacy_codep_home_is_not_auto_renamed_to_praxis_home() {
        let temp_home = TempDir::new().expect("temp home");
        let previous_home = temp_home.path().join(LEGACY_CODEP_HOME_DIRNAME);
        let default_home = temp_home.path().join(PRAXIS_HOME_DIRNAME);
        fs::create_dir_all(previous_home.join("sessions")).expect("create previous home");
        fs::write(previous_home.join("sessions").join("thread.jsonl"), "{}")
            .expect("write previous asset");

        let resolved = super::default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis)
            .expect("default praxis home");

        assert!(resolved.ends_with(PRAXIS_HOME_DIRNAME));
        assert!(
            previous_home
                .join("sessions")
                .join("thread.jsonl")
                .is_file()
        );
        assert!(!default_home.exists());
    }

    #[test]
    fn upstream_codex_read_through_home_uses_default_praxis_home_when_bridge_is_active() {
        let praxis_home =
            default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis).expect("praxis home");
        let shared = upstream_codex_read_through_home_for_test(
            PraxisHomeNamespace::Praxis,
            praxis_home.as_path(),
        )
        .expect("shared codex home");
        let expected = super::default_upstream_codex_home().expect("codex home");
        assert_eq!(shared, Some(expected));
    }

    #[test]
    fn upstream_codex_read_through_home_is_disabled_for_non_default_home() {
        let temp_home = TempDir::new().expect("temp home");
        let shared = upstream_codex_read_through_home_for_test(
            PraxisHomeNamespace::Praxis,
            temp_home.path(),
        )
        .expect("shared codex home");
        assert_eq!(shared, None);
    }
}
