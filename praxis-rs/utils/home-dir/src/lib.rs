use dirs::home_dir;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

const PRAXIS_HOME_ENV_VAR: &str = "PRAXIS_HOME";
const PRAXIS_HOME_NAMESPACE_ENV_VAR: &str = "PRAXIS_HOME_NAMESPACE";
const PRAXIS_HOME_DIRNAME: &str = ".praxis";
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
    match active_namespace_env_override()
        .as_ref()
        .and_then(|env| PraxisHomeNamespace::from_str(env.value.as_str()))
    {
        Some(PraxisHomeNamespace::Praxis) => PraxisHomeNamespace::Praxis,
        // Do not allow an environment variable to move Praxis state into any
        // non-Praxis namespace.  Codex/config/auth bridging is explicit and
        // read-through; Praxis runtime state always remains under Praxis home.
        None => infer_praxis_home_namespace_from_args(std::env::args_os()),
    }
}

pub fn default_praxis_home_for_namespace(
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(namespace.default_dir_name());
    Ok(path)
}

/// Returns the upstream Codex home used only by explicit read-through bridges.
///
/// This is deliberately not represented as a `PraxisHomeNamespace`: Codex is
/// an external data source, never a namespace where Praxis runtime state,
/// thread indexes, goals, logs, or SQLite databases may be written.
pub fn default_upstream_codex_home() -> std::io::Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(UPSTREAM_CODEX_HOME_DIRNAME);
    Ok(path)
}

/// Returns the legacy Codep home used only for explicit diagnostics/migration.
///
/// Praxis must not auto-rename or auto-import this directory because old
/// Codep profiles commonly contain Codex env wrappers, stale thread ids, and
/// schema-incompatible SQLite state.
pub fn default_legacy_codep_home() -> std::io::Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(LEGACY_CODEP_HOME_DIRNAME);
    Ok(path)
}

/// Returns the upstream Codex home that Praxis may use for bridged user-level
/// state such as config/auth, or `None` when the current process should remain
/// fully isolated.
///
/// The bridge is enabled only when:
/// - the current namespace resolves to `praxis`
/// - `PRAXIS_HOME` is not explicitly set
/// - the provided `praxis_home` is the default `~/.praxis` location
///
/// This keeps custom/test homes isolated while allowing the default Praxis UX
/// to inherit Codex config/auth without sharing thread storage.
pub fn upstream_codex_read_through_home(praxis_home: &Path) -> std::io::Result<Option<PathBuf>> {
    let praxis_home_env = active_home_env_override();
    let namespace = current_praxis_home_namespace();
    upstream_codex_read_through_home_with_namespace_hint(
        praxis_home_env.as_ref(),
        namespace,
        praxis_home,
    )
}

pub fn scrub_upstream_codex_state_env_for_current_process() {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.  Keeping these variables in-process is a
    // footgun: profile wrappers can point them at stale Codex/Codep homes and
    // nested child commands can inherit them.
    unsafe {
        std::env::remove_var("CODEX_HOME");
        std::env::remove_var("CODEX_HOME_NAMESPACE");
        std::env::remove_var("CODEX_SQLITE_HOME");
        std::env::remove_var("CODEX_THREAD_ID");
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

    let namespace = infer_praxis_home_namespace_from_args(std::env::args_os());
    set_process_praxis_home_namespace(namespace);
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
        None => default_praxis_home_for_namespace_without_legacy_migration(namespace),
    }
}

fn upstream_codex_read_through_home_with_namespace_hint(
    praxis_home_env: Option<&HomeEnvOverride>,
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    if praxis_home_env.is_some() || namespace != PraxisHomeNamespace::Praxis {
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

fn default_praxis_home_for_namespace_without_legacy_migration(
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    // Deliberately do not auto-rename legacy .codep into .praxis.  Codep was
    // a frequent source of Codex env/thread/sqlite pollution; importing it must
    // be an explicit migration/doctor command, never an implicit startup side
    // effect hidden inside home resolution.
    default_praxis_home_for_namespace(namespace)
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

fn infer_praxis_home_namespace_from_args<I, S>(_args: I) -> PraxisHomeNamespace
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // Praxis is a separate product/runtime.  Even when launched through a
    // legacy binary name or wrapper, default to the Praxis namespace so we do
    // not write sessions, state_*.sqlite, logs, or feature flags into
    // ~/.codex.  Explicit bridging to Codex config/auth is handled elsewhere
    // and remains read-through only.
    PraxisHomeNamespace::Praxis
}

fn executable_stem(value: &OsStr) -> String {
    Path::new(value)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
fn lowercase_arg<S: AsRef<OsStr>>(value: S) -> Option<String> {
    value.as_ref().to_str().map(|arg| arg.to_ascii_lowercase())
}

#[cfg(test)]
fn find_praxis_home_with_namespace_hint(
    praxis_home_env: Option<&str>,
    namespace_env: Option<&str>,
    args: &[&str],
) -> std::io::Result<PathBuf> {
    let namespace = namespace_env
        .and_then(PraxisHomeNamespace::from_str)
        .unwrap_or_else(|| infer_praxis_home_namespace_from_args(args));
    let home_env = praxis_home_env.map(|value| HomeEnvOverride {
        name: PRAXIS_HOME_ENV_VAR,
        value: value.to_string(),
    });
    find_praxis_home_from_env_and_namespace_without_rename(home_env.as_ref(), namespace)
}

#[cfg(test)]
fn find_praxis_home_from_env_and_namespace_without_rename(
    praxis_home_env: Option<&HomeEnvOverride>,
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    match praxis_home_env {
        Some(env) => find_praxis_home_from_env_and_namespace(Some(env), namespace),
        None => default_praxis_home_for_namespace(namespace),
    }
}

#[cfg(test)]
fn infer_praxis_home_namespace_for_test(args: &[&str]) -> PraxisHomeNamespace {
    infer_praxis_home_namespace_from_args(args)
}

#[cfg(test)]
fn executable_stem_for_test(value: &str) -> String {
    executable_stem(OsStr::new(value))
}

#[cfg(test)]
fn lowercase_arg_for_test(value: &str) -> Option<String> {
    lowercase_arg(value)
}

#[cfg(test)]
fn default_home_for_test(namespace: PraxisHomeNamespace) -> std::io::Result<PathBuf> {
    default_praxis_home_for_namespace(namespace)
}

#[cfg(test)]
fn namespace_as_str_for_test(namespace: PraxisHomeNamespace) -> &'static str {
    namespace.as_str()
}

#[cfg(test)]
fn namespace_from_str_for_test(value: &str) -> Option<PraxisHomeNamespace> {
    PraxisHomeNamespace::from_str(value)
}

#[cfg(test)]
fn namespace_default_dir_name_for_test(namespace: PraxisHomeNamespace) -> &'static str {
    namespace.default_dir_name()
}

#[cfg(test)]
fn namespace_env_var_for_test() -> &'static str {
    PRAXIS_HOME_NAMESPACE_ENV_VAR
}

#[cfg(test)]
fn home_env_var_for_test() -> &'static str {
    PRAXIS_HOME_ENV_VAR
}

#[cfg(test)]
fn praxis_dir_name_for_test() -> &'static str {
    PRAXIS_HOME_DIRNAME
}

#[cfg(test)]
fn codex_dir_name_for_test() -> &'static str {
    UPSTREAM_CODEX_HOME_DIRNAME
}

#[cfg(test)]
fn previous_praxis_dir_name_for_test() -> &'static str {
    LEGACY_CODEP_HOME_DIRNAME
}

#[cfg(test)]
fn set_process_namespace_for_test(namespace: PraxisHomeNamespace) {
    set_process_praxis_home_namespace(namespace);
}

#[cfg(test)]
fn set_process_namespace_if_unset_for_test() {
    set_process_praxis_home_namespace_if_unset_for_current_process();
}

#[cfg(test)]
fn current_namespace_for_test() -> PraxisHomeNamespace {
    current_praxis_home_namespace()
}

#[cfg(test)]
fn find_home_for_test() -> std::io::Result<PathBuf> {
    find_praxis_home()
}

#[cfg(test)]
fn find_home_for_namespace_from_default(
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    default_praxis_home_for_namespace(namespace)
}

#[cfg(test)]
fn upstream_codex_read_through_home_for_test(
    praxis_home_env: Option<&str>,
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    let home_env = praxis_home_env.map(|value| HomeEnvOverride {
        name: PRAXIS_HOME_ENV_VAR,
        value: value.to_string(),
    });
    upstream_codex_read_through_home_with_namespace_hint(home_env.as_ref(), namespace, praxis_home)
}

#[cfg(test)]
mod tests {
    use super::PraxisHomeNamespace;
    use super::codex_dir_name_for_test;
    use super::default_home_for_test;
    use super::executable_stem_for_test;
    use super::find_praxis_home_with_namespace_hint;
    use super::home_env_var_for_test;
    use super::infer_praxis_home_namespace_for_test;
    use super::lowercase_arg_for_test;
    use super::namespace_as_str_for_test;
    use super::namespace_default_dir_name_for_test;
    use super::namespace_env_var_for_test;
    use super::namespace_from_str_for_test;
    use super::praxis_dir_name_for_test;
    use super::previous_praxis_dir_name_for_test;
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

        let err = find_praxis_home_with_namespace_hint(Some(missing_str), None, &["praxis"])
            .expect_err("missing PRAXIS_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains(home_env_var_for_test()),
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

        let err = find_praxis_home_with_namespace_hint(Some(file_str), None, &["praxis"])
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

        let resolved = find_praxis_home_with_namespace_hint(Some(temp_str), None, &["praxis"])
            .expect("valid PRAXIS_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_praxis_home_without_env_uses_praxis_default_home_dir() {
        let resolved =
            find_praxis_home_with_namespace_hint(/*praxis_home_env*/ None, None, &["praxis"])
                .expect("default PRAXIS_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(praxis_dir_name_for_test());
        assert_eq!(resolved, expected);
    }

    #[test]
    fn namespace_env_cannot_switch_praxis_into_codex_home() {
        let resolved = find_praxis_home_with_namespace_hint(None, Some("codex"), &["praxis"])
            .expect("default PRAXIS_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(praxis_dir_name_for_test());
        assert_eq!(resolved, expected);
    }

    #[test]
    fn infer_namespace_defaults_to_praxis_for_praxis_binaries() {
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["praxis"]),
            PraxisHomeNamespace::Praxis
        );
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["praxis-x86_64-unknown-linux-musl"]),
            PraxisHomeNamespace::Praxis
        );
    }

    #[test]
    fn infer_namespace_defaults_to_praxis_for_legacy_codex_binary_names() {
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["codex"]),
            PraxisHomeNamespace::Praxis
        );
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["codex.exe"]),
            PraxisHomeNamespace::Praxis
        );
    }

    #[test]
    fn infer_namespace_keeps_praxis_home_for_resume_bridge_commands() {
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["praxis", "resume", "codex"]),
            PraxisHomeNamespace::Praxis
        );
        assert_eq!(
            infer_praxis_home_namespace_for_test(&["praxis", "fork", "codex"]),
            PraxisHomeNamespace::Praxis
        );
    }

    #[test]
    fn helper_methods_roundtrip_namespace_metadata() {
        assert_eq!(
            namespace_as_str_for_test(PraxisHomeNamespace::Praxis),
            "praxis"
        );
        assert_eq!(
            namespace_from_str_for_test("PrAxIs"),
            Some(PraxisHomeNamespace::Praxis)
        );
        assert_eq!(namespace_from_str_for_test("codex"), None);
        assert_eq!(namespace_from_str_for_test("unknown"), None);
        assert_eq!(
            namespace_default_dir_name_for_test(PraxisHomeNamespace::Praxis),
            praxis_dir_name_for_test()
        );
        assert_eq!(namespace_env_var_for_test(), "PRAXIS_HOME_NAMESPACE");
        assert_eq!(home_env_var_for_test(), "PRAXIS_HOME");
    }

    #[test]
    fn default_home_builder_uses_expected_suffixes() {
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let codex_home = super::default_upstream_codex_home().expect("codex home");
        let codep_home = super::default_legacy_codep_home().expect("legacy codep home");
        assert!(praxis_home.ends_with(praxis_dir_name_for_test()));
        assert!(codex_home.ends_with(codex_dir_name_for_test()));
        assert!(codep_home.ends_with(previous_praxis_dir_name_for_test()));
    }

    #[test]
    fn legacy_codep_home_is_not_auto_renamed_to_praxis_home() {
        let temp_home = TempDir::new().expect("temp home");
        let previous_home = temp_home.path().join(previous_praxis_dir_name_for_test());
        let default_home = temp_home.path().join(praxis_dir_name_for_test());
        fs::create_dir_all(previous_home.join("sessions")).expect("create previous home");
        fs::write(previous_home.join("sessions").join("thread.jsonl"), "{}")
            .expect("write previous asset");

        let resolved = super::default_praxis_home_for_namespace_without_legacy_migration(
            PraxisHomeNamespace::Praxis,
        )
        .expect("default praxis home");

        assert!(resolved.ends_with(praxis_dir_name_for_test()));
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
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let shared = upstream_codex_read_through_home_for_test(
            None,
            PraxisHomeNamespace::Praxis,
            praxis_home.as_path(),
        )
        .expect("shared codex home");
        let expected = super::default_upstream_codex_home().expect("codex home");
        assert_eq!(shared, Some(expected));
    }

    #[test]
    fn upstream_codex_read_through_home_is_disabled_for_explicit_code_home() {
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let shared = upstream_codex_read_through_home_for_test(
            Some("C:\\custom-praxis-home"),
            PraxisHomeNamespace::Praxis,
            praxis_home.as_path(),
        )
        .expect("shared codex home");
        assert_eq!(shared, None);
    }

    #[test]
    fn upstream_codex_read_through_home_is_disabled_for_non_default_home() {
        let temp_home = TempDir::new().expect("temp home");
        let shared = upstream_codex_read_through_home_for_test(
            None,
            PraxisHomeNamespace::Praxis,
            temp_home.path(),
        )
        .expect("shared codex home");
        assert_eq!(shared, None);
    }

    #[test]
    fn executable_stem_handles_extensions() {
        assert_eq!(executable_stem_for_test("praxis.exe"), "praxis");
        assert_eq!(executable_stem_for_test("/tmp/praxis"), "praxis");
    }

    #[test]
    fn lowercase_arg_normalizes_ascii() {
        assert_eq!(lowercase_arg_for_test("CoDeX"), Some("codex".to_string()));
    }
}
