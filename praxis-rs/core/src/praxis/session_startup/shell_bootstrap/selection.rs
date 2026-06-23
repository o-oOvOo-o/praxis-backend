use praxis_features::Feature;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
use crate::shell;
use crate::shell::Shell;

pub(super) fn resolve(
    config: &Config,
    session_configuration: &SessionConfiguration,
) -> anyhow::Result<Shell> {
    if let Some(user_shell_override) = session_configuration.user_shell_override.clone() {
        return Ok(user_shell_override);
    }

    if config.features.enabled(Feature::ShellZshFork) {
        let zsh_path = config.zsh_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "zsh fork feature enabled, but `zsh_path` is not configured; set `zsh_path` in config.toml"
            )
        })?;
        let zsh_path = zsh_path.to_path_buf();
        return shell::get_shell(shell::ShellType::Zsh, Some(&zsh_path)).ok_or_else(|| {
            anyhow::anyhow!(
                "zsh fork feature enabled, but zsh_path `{}` is not usable; set `zsh_path` to a valid zsh executable",
                zsh_path.display()
            )
        });
    }

    Ok(shell::default_user_shell())
}
