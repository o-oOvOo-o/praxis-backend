use std::path::PathBuf;

use praxis_utils_home_dir::PraxisHomeNamespace;

use super::Config;

/// Returns the path to the Praxis configuration directory, which can be
/// specified by the `PRAXIS_HOME` environment variable. If not set, defaults to
/// `~/.praxis`.
///
/// - If `PRAXIS_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `PRAXIS_HOME` is not set, this function does not verify that the
///   directory exists.
pub fn find_praxis_home() -> std::io::Result<PathBuf> {
    praxis_utils_home_dir::find_praxis_home()
}

pub fn current_praxis_home_namespace() -> PraxisHomeNamespace {
    praxis_utils_home_dir::current_praxis_home_namespace()
}

pub fn default_praxis_home_for_namespace(
    namespace: PraxisHomeNamespace,
) -> std::io::Result<PathBuf> {
    praxis_utils_home_dir::default_praxis_home_for_namespace(namespace)
}

pub fn default_external_codex_home() -> std::io::Result<PathBuf> {
    praxis_utils_home_dir::default_external_codex_home()
}

pub fn default_legacy_codep_home() -> std::io::Result<PathBuf> {
    praxis_utils_home_dir::default_legacy_codep_home()
}

/// Returns the path to the folder where Praxis logs are stored. Does not verify
/// that the directory exists.
pub fn log_dir(cfg: &Config) -> std::io::Result<PathBuf> {
    Ok(cfg.log_dir.clone())
}
