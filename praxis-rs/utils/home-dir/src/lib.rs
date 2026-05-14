use dirs::home_dir;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const CODEX_HOME_NAMESPACE_ENV_VAR: &str = "CODEX_HOME_NAMESPACE";
const PRAXIS_HOME_DIRNAME: &str = ".praxis";
const CODEX_HOME_DIRNAME: &str = ".codex";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PraxisHomeNamespace {
    Praxis,
    Codex,
}

impl PraxisHomeNamespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Praxis => "praxis",
            Self::Codex => "codex",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "praxis" => Some(Self::Praxis),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    fn default_dir_name(self) -> &'static str {
        match self {
            Self::Praxis => PRAXIS_HOME_DIRNAME,
            Self::Codex => CODEX_HOME_DIRNAME,
        }
    }
}

/// Returns the path to the Praxis configuration directory, which can be
/// specified by the `CODEX_HOME` environment variable. If not set, defaults to
/// the namespace-specific home directory (`~/.praxis` for Praxis, `~/.codex` for
/// Codex).
///
/// - If `CODEX_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `CODEX_HOME` is not set, this function does not verify that the
///   directory exists.
pub fn find_praxis_home() -> std::io::Result<PathBuf> {
    let praxis_home_env = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .filter(|val| !val.is_empty());
    let namespace = current_praxis_home_namespace();
    find_praxis_home_from_env_and_namespace(praxis_home_env.as_deref(), namespace)
}

pub fn current_praxis_home_namespace() -> PraxisHomeNamespace {
    std::env::var(CODEX_HOME_NAMESPACE_ENV_VAR)
        .ok()
        .as_deref()
        .and_then(PraxisHomeNamespace::from_str)
        .unwrap_or_else(|| infer_praxis_home_namespace_from_args(std::env::args_os()))
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

/// Returns the shared Codex home that Praxis should use for bridged user-level
/// state such as config/auth, or `None` when the current process should remain
/// fully isolated.
///
/// The bridge is enabled only when:
/// - the current namespace resolves to `praxis`
/// - `CODEX_HOME` is not explicitly set
/// - the provided `praxis_home` is the default `~/.praxis` location
///
/// This keeps custom/test homes isolated while allowing the default Praxis UX
/// to inherit Codex config/auth without sharing thread storage.
pub fn praxis_shared_praxis_home(praxis_home: &Path) -> std::io::Result<Option<PathBuf>> {
    let praxis_home_env = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let namespace = current_praxis_home_namespace();
    praxis_shared_praxis_home_with_namespace_hint(
        praxis_home_env.as_deref(),
        namespace,
        praxis_home,
    )
}

pub fn set_process_praxis_home_namespace(namespace: PraxisHomeNamespace) {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.
    unsafe {
        std::env::set_var(CODEX_HOME_NAMESPACE_ENV_VAR, namespace.as_str());
    }
}

pub fn set_process_praxis_home_namespace_if_unset_for_current_process() {
    let home_is_explicit = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let namespace_is_explicit = std::env::var(CODEX_HOME_NAMESPACE_ENV_VAR)
        .ok()
        .is_some_and(|value| PraxisHomeNamespace::from_str(&value).is_some());
    if home_is_explicit || namespace_is_explicit {
        return;
    }

    let namespace = infer_praxis_home_namespace_from_args(std::env::args_os());
    set_process_praxis_home_namespace(namespace);
}

fn find_praxis_home_from_env_and_namespace(
    praxis_home_env: Option<&str>,
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    // Honor the `CODEX_HOME` environment variable when it is set to allow users
    // (and tests) to override the default location.
    match praxis_home_env {
        Some(val) => {
            let path = PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("CODEX_HOME points to {val:?}, but that path does not exist"),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read CODEX_HOME {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("CODEX_HOME points to {val:?}, but that path is not a directory"),
                ))
            } else {
                path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize CODEX_HOME {val:?}: {err}"),
                    )
                })
            }
        }
        None => default_praxis_home_for_namespace(namespace),
    }
}

fn praxis_shared_praxis_home_with_namespace_hint(
    praxis_home_env: Option<&str>,
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    if praxis_home_env.is_some_and(|value| !value.trim().is_empty())
        || namespace != PraxisHomeNamespace::Praxis
    {
        return Ok(None);
    }

    let default_praxis_home = default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis)?;
    if !paths_match(praxis_home, &default_praxis_home) {
        return Ok(None);
    }

    default_praxis_home_for_namespace(PraxisHomeNamespace::Codex).map(Some)
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

fn infer_praxis_home_namespace_from_args<I, S>(args: I) -> PraxisHomeNamespace
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = args.into_iter();
    let argv0 = args
        .next()
        .as_ref()
        .map(|value| executable_stem(value.as_ref()).to_ascii_lowercase())
        .unwrap_or_default();

    if argv0.starts_with("praxis") {
        PraxisHomeNamespace::Praxis
    } else {
        PraxisHomeNamespace::Codex
    }
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
    find_praxis_home_from_env_and_namespace(praxis_home_env, namespace)
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
    CODEX_HOME_NAMESPACE_ENV_VAR
}

#[cfg(test)]
fn home_env_var_for_test() -> &'static str {
    CODEX_HOME_ENV_VAR
}

#[cfg(test)]
fn praxis_dir_name_for_test() -> &'static str {
    PRAXIS_HOME_DIRNAME
}

#[cfg(test)]
fn codex_dir_name_for_test() -> &'static str {
    CODEX_HOME_DIRNAME
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
fn praxis_shared_praxis_home_for_test(
    praxis_home_env: Option<&str>,
    namespace: PraxisHomeNamespace,
    praxis_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    praxis_shared_praxis_home_with_namespace_hint(praxis_home_env, namespace, praxis_home)
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
    use super::praxis_shared_praxis_home_for_test;
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
            .expect("missing codex home path should be valid utf-8");

        let err = find_praxis_home_with_namespace_hint(Some(missing_str), None, &["praxis"])
            .expect_err("missing CODEX_HOME");
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
            .expect("file codex home path should be valid utf-8");

        let err = find_praxis_home_with_namespace_hint(Some(file_str), None, &["praxis"])
            .expect_err("file CODEX_HOME");
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
            .expect("temp codex home path should be valid utf-8");

        let resolved = find_praxis_home_with_namespace_hint(Some(temp_str), None, &["praxis"])
            .expect("valid CODEX_HOME");
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
                .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(praxis_dir_name_for_test());
        assert_eq!(resolved, expected);
    }

    #[test]
    fn namespace_env_can_switch_to_praxis_home_without_overriding_praxis_default() {
        let resolved = find_praxis_home_with_namespace_hint(None, Some("codex"), &["praxis"])
            .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(codex_dir_name_for_test());
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
            namespace_as_str_for_test(PraxisHomeNamespace::Codex),
            "codex"
        );
        assert_eq!(
            namespace_from_str_for_test("PrAxIs"),
            Some(PraxisHomeNamespace::Praxis)
        );
        assert_eq!(
            namespace_from_str_for_test("codex"),
            Some(PraxisHomeNamespace::Codex)
        );
        assert_eq!(namespace_from_str_for_test("unknown"), None);
        assert_eq!(
            namespace_default_dir_name_for_test(PraxisHomeNamespace::Codex),
            codex_dir_name_for_test()
        );
        assert_eq!(
            namespace_default_dir_name_for_test(PraxisHomeNamespace::Praxis),
            praxis_dir_name_for_test()
        );
        assert_eq!(namespace_env_var_for_test(), "CODEX_HOME_NAMESPACE");
        assert_eq!(home_env_var_for_test(), "CODEX_HOME");
    }

    #[test]
    fn default_home_builder_uses_expected_suffixes() {
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let codex_home = default_home_for_test(PraxisHomeNamespace::Codex).expect("codex home");
        assert!(praxis_home.ends_with(praxis_dir_name_for_test()));
        assert!(codex_home.ends_with(codex_dir_name_for_test()));
    }

    #[test]
    fn praxis_shared_praxis_home_uses_default_praxis_home_when_bridge_is_active() {
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let shared = praxis_shared_praxis_home_for_test(
            None,
            PraxisHomeNamespace::Praxis,
            praxis_home.as_path(),
        )
        .expect("shared codex home");
        let expected = default_home_for_test(PraxisHomeNamespace::Codex).expect("codex home");
        assert_eq!(shared, Some(expected));
    }

    #[test]
    fn praxis_shared_praxis_home_is_disabled_for_explicit_code_home() {
        let praxis_home = default_home_for_test(PraxisHomeNamespace::Praxis).expect("praxis home");
        let shared = praxis_shared_praxis_home_for_test(
            Some("C:\\custom-praxis-home"),
            PraxisHomeNamespace::Praxis,
            praxis_home.as_path(),
        )
        .expect("shared codex home");
        assert_eq!(shared, None);
    }

    #[test]
    fn praxis_shared_praxis_home_is_disabled_for_non_default_home() {
        let temp_home = TempDir::new().expect("temp home");
        let shared =
            praxis_shared_praxis_home_for_test(None, PraxisHomeNamespace::Praxis, temp_home.path())
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
