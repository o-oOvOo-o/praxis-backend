use dirs::home_dir;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const CODEX_HOME_NAMESPACE_ENV_VAR: &str = "CODEX_HOME_NAMESPACE";
const CODEP_HOME_DIRNAME: &str = ".codep";
const CODEX_HOME_DIRNAME: &str = ".codex";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexHomeNamespace {
    Codep,
    Codex,
}

impl CodexHomeNamespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codep => "codep",
            Self::Codex => "codex",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codep" => Some(Self::Codep),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    fn default_dir_name(self) -> &'static str {
        match self {
            Self::Codep => CODEP_HOME_DIRNAME,
            Self::Codex => CODEX_HOME_DIRNAME,
        }
    }
}

/// Returns the path to the Codex configuration directory, which can be
/// specified by the `CODEX_HOME` environment variable. If not set, defaults to
/// the namespace-specific home directory (`~/.codep` for CodeP, `~/.codex` for
/// Codex).
///
/// - If `CODEX_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `CODEX_HOME` is not set, this function does not verify that the
///   directory exists.
pub fn find_codex_home() -> std::io::Result<PathBuf> {
    let codex_home_env = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .filter(|val| !val.is_empty());
    let namespace = current_codex_home_namespace();
    find_codex_home_from_env_and_namespace(codex_home_env.as_deref(), namespace)
}

pub fn current_codex_home_namespace() -> CodexHomeNamespace {
    std::env::var(CODEX_HOME_NAMESPACE_ENV_VAR)
        .ok()
        .as_deref()
        .and_then(CodexHomeNamespace::from_str)
        .unwrap_or_else(|| infer_codex_home_namespace_from_args(std::env::args_os()))
}

pub fn default_codex_home_for_namespace(namespace: CodexHomeNamespace) -> std::io::Result<PathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(namespace.default_dir_name());
    Ok(path)
}

/// Returns the shared Codex home that CodeP should use for bridged user-level
/// state such as config/auth, or `None` when the current process should remain
/// fully isolated.
///
/// The bridge is enabled only when:
/// - the current namespace resolves to `codep`
/// - `CODEX_HOME` is not explicitly set
/// - the provided `codex_home` is the default `~/.codep` location
///
/// This keeps custom/test homes isolated while allowing the default CodeP UX
/// to inherit Codex config/auth without sharing thread storage.
pub fn codep_shared_codex_home(codex_home: &Path) -> std::io::Result<Option<PathBuf>> {
    let codex_home_env = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let namespace = current_codex_home_namespace();
    codep_shared_codex_home_with_namespace_hint(codex_home_env.as_deref(), namespace, codex_home)
}

pub fn set_process_codex_home_namespace(namespace: CodexHomeNamespace) {
    // Safe because the binary entrypoint invokes this before the Tokio runtime
    // and worker threads are created.
    unsafe {
        std::env::set_var(CODEX_HOME_NAMESPACE_ENV_VAR, namespace.as_str());
    }
}

pub fn set_process_codex_home_namespace_if_unset_for_current_process() {
    let home_is_explicit = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let namespace_is_explicit = std::env::var(CODEX_HOME_NAMESPACE_ENV_VAR)
        .ok()
        .is_some_and(|value| CodexHomeNamespace::from_str(&value).is_some());
    if home_is_explicit || namespace_is_explicit {
        return;
    }

    let namespace = infer_codex_home_namespace_from_args(std::env::args_os());
    set_process_codex_home_namespace(namespace);
}

fn find_codex_home_from_env_and_namespace(
    codex_home_env: Option<&str>,
    namespace: CodexHomeNamespace,
) -> std::io::Result<PathBuf> {
    // Honor the `CODEX_HOME` environment variable when it is set to allow users
    // (and tests) to override the default location.
    match codex_home_env {
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
        None => default_codex_home_for_namespace(namespace),
    }
}

fn codep_shared_codex_home_with_namespace_hint(
    codex_home_env: Option<&str>,
    namespace: CodexHomeNamespace,
    codex_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    if codex_home_env.is_some_and(|value| !value.trim().is_empty())
        || namespace != CodexHomeNamespace::Codep
    {
        return Ok(None);
    }

    let default_codep_home = default_codex_home_for_namespace(CodexHomeNamespace::Codep)?;
    if !paths_match(codex_home, &default_codep_home) {
        return Ok(None);
    }

    default_codex_home_for_namespace(CodexHomeNamespace::Codex).map(Some)
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

fn infer_codex_home_namespace_from_args<I, S>(args: I) -> CodexHomeNamespace
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

    if argv0.starts_with("codep") {
        CodexHomeNamespace::Codep
    } else {
        CodexHomeNamespace::Codex
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
fn find_codex_home_with_namespace_hint(
    codex_home_env: Option<&str>,
    namespace_env: Option<&str>,
    args: &[&str],
) -> std::io::Result<PathBuf> {
    let namespace = namespace_env
        .and_then(CodexHomeNamespace::from_str)
        .unwrap_or_else(|| infer_codex_home_namespace_from_args(args));
    find_codex_home_from_env_and_namespace(codex_home_env, namespace)
}

#[cfg(test)]
fn infer_codex_home_namespace_for_test(args: &[&str]) -> CodexHomeNamespace {
    infer_codex_home_namespace_from_args(args)
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
fn default_home_for_test(namespace: CodexHomeNamespace) -> std::io::Result<PathBuf> {
    default_codex_home_for_namespace(namespace)
}

#[cfg(test)]
fn namespace_as_str_for_test(namespace: CodexHomeNamespace) -> &'static str {
    namespace.as_str()
}

#[cfg(test)]
fn namespace_from_str_for_test(value: &str) -> Option<CodexHomeNamespace> {
    CodexHomeNamespace::from_str(value)
}

#[cfg(test)]
fn namespace_default_dir_name_for_test(namespace: CodexHomeNamespace) -> &'static str {
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
fn codep_dir_name_for_test() -> &'static str {
    CODEP_HOME_DIRNAME
}

#[cfg(test)]
fn codex_dir_name_for_test() -> &'static str {
    CODEX_HOME_DIRNAME
}

#[cfg(test)]
fn set_process_namespace_for_test(namespace: CodexHomeNamespace) {
    set_process_codex_home_namespace(namespace);
}

#[cfg(test)]
fn set_process_namespace_if_unset_for_test() {
    set_process_codex_home_namespace_if_unset_for_current_process();
}

#[cfg(test)]
fn current_namespace_for_test() -> CodexHomeNamespace {
    current_codex_home_namespace()
}

#[cfg(test)]
fn find_home_for_test() -> std::io::Result<PathBuf> {
    find_codex_home()
}

#[cfg(test)]
fn find_home_for_namespace_from_default(namespace: CodexHomeNamespace) -> std::io::Result<PathBuf> {
    default_codex_home_for_namespace(namespace)
}

#[cfg(test)]
fn codep_shared_codex_home_for_test(
    codex_home_env: Option<&str>,
    namespace: CodexHomeNamespace,
    codex_home: &Path,
) -> std::io::Result<Option<PathBuf>> {
    codep_shared_codex_home_with_namespace_hint(codex_home_env, namespace, codex_home)
}

#[cfg(test)]
mod tests {
    use super::CodexHomeNamespace;
    use super::codep_dir_name_for_test;
    use super::codep_shared_codex_home_for_test;
    use super::codex_dir_name_for_test;
    use super::default_home_for_test;
    use super::executable_stem_for_test;
    use super::find_codex_home_with_namespace_hint;
    use super::home_env_var_for_test;
    use super::infer_codex_home_namespace_for_test;
    use super::lowercase_arg_for_test;
    use super::namespace_as_str_for_test;
    use super::namespace_default_dir_name_for_test;
    use super::namespace_env_var_for_test;
    use super::namespace_from_str_for_test;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn find_codex_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-codex-home");
        let missing_str = missing
            .to_str()
            .expect("missing codex home path should be valid utf-8");

        let err = find_codex_home_with_namespace_hint(Some(missing_str), None, &["codep"])
            .expect_err("missing CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains(home_env_var_for_test()),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("codex-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file codex home path should be valid utf-8");

        let err = find_codex_home_with_namespace_hint(Some(file_str), None, &["codep"])
            .expect_err("file CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp codex home path should be valid utf-8");

        let resolved = find_codex_home_with_namespace_hint(Some(temp_str), None, &["codep"])
            .expect("valid CODEX_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_codex_home_without_env_uses_codep_default_home_dir() {
        let resolved =
            find_codex_home_with_namespace_hint(/*codex_home_env*/ None, None, &["codep"])
                .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(codep_dir_name_for_test());
        assert_eq!(resolved, expected);
    }

    #[test]
    fn namespace_env_can_switch_to_codex_home_without_overriding_codep_default() {
        let resolved = find_codex_home_with_namespace_hint(None, Some("codex"), &["codep"])
            .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(codex_dir_name_for_test());
        assert_eq!(resolved, expected);
    }

    #[test]
    fn infer_namespace_defaults_to_codep_for_codep_binaries() {
        assert_eq!(
            infer_codex_home_namespace_for_test(&["codep"]),
            CodexHomeNamespace::Codep
        );
        assert_eq!(
            infer_codex_home_namespace_for_test(&["codep-x86_64-unknown-linux-musl"]),
            CodexHomeNamespace::Codep
        );
    }

    #[test]
    fn infer_namespace_keeps_codep_home_for_resume_bridge_commands() {
        assert_eq!(
            infer_codex_home_namespace_for_test(&["codep", "resume", "codex"]),
            CodexHomeNamespace::Codep
        );
        assert_eq!(
            infer_codex_home_namespace_for_test(&["codep", "fork", "codex"]),
            CodexHomeNamespace::Codep
        );
    }

    #[test]
    fn helper_methods_roundtrip_namespace_metadata() {
        assert_eq!(
            namespace_as_str_for_test(CodexHomeNamespace::Codep),
            "codep"
        );
        assert_eq!(
            namespace_as_str_for_test(CodexHomeNamespace::Codex),
            "codex"
        );
        assert_eq!(
            namespace_from_str_for_test("CoDeP"),
            Some(CodexHomeNamespace::Codep)
        );
        assert_eq!(
            namespace_from_str_for_test("codex"),
            Some(CodexHomeNamespace::Codex)
        );
        assert_eq!(namespace_from_str_for_test("unknown"), None);
        assert_eq!(
            namespace_default_dir_name_for_test(CodexHomeNamespace::Codep),
            codep_dir_name_for_test()
        );
        assert_eq!(
            namespace_default_dir_name_for_test(CodexHomeNamespace::Codex),
            codex_dir_name_for_test()
        );
        assert_eq!(namespace_env_var_for_test(), "CODEX_HOME_NAMESPACE");
        assert_eq!(home_env_var_for_test(), "CODEX_HOME");
    }

    #[test]
    fn default_home_builder_uses_expected_suffixes() {
        let codep_home = default_home_for_test(CodexHomeNamespace::Codep).expect("codep home");
        let codex_home = default_home_for_test(CodexHomeNamespace::Codex).expect("codex home");
        assert!(codep_home.ends_with(codep_dir_name_for_test()));
        assert!(codex_home.ends_with(codex_dir_name_for_test()));
    }

    #[test]
    fn codep_shared_codex_home_uses_default_codex_home_when_bridge_is_active() {
        let codep_home = default_home_for_test(CodexHomeNamespace::Codep).expect("codep home");
        let shared =
            codep_shared_codex_home_for_test(None, CodexHomeNamespace::Codep, codep_home.as_path())
                .expect("shared codex home");
        let expected = default_home_for_test(CodexHomeNamespace::Codex).expect("codex home");
        assert_eq!(shared, Some(expected));
    }

    #[test]
    fn codep_shared_codex_home_is_disabled_for_explicit_code_home() {
        let codep_home = default_home_for_test(CodexHomeNamespace::Codep).expect("codep home");
        let shared = codep_shared_codex_home_for_test(
            Some("C:\\custom-codep-home"),
            CodexHomeNamespace::Codep,
            codep_home.as_path(),
        )
        .expect("shared codex home");
        assert_eq!(shared, None);
    }

    #[test]
    fn codep_shared_codex_home_is_disabled_for_non_default_home() {
        let temp_home = TempDir::new().expect("temp home");
        let shared =
            codep_shared_codex_home_for_test(None, CodexHomeNamespace::Codep, temp_home.path())
                .expect("shared codex home");
        assert_eq!(shared, None);
    }

    #[test]
    fn executable_stem_handles_extensions() {
        assert_eq!(executable_stem_for_test("codep.exe"), "codep");
        assert_eq!(executable_stem_for_test("/tmp/codep"), "codep");
    }

    #[test]
    fn lowercase_arg_normalizes_ascii() {
        assert_eq!(lowercase_arg_for_test("CoDeX"), Some("codex".to_string()));
    }
}
