use praxis_features::Feature;
use praxis_hooks::Hooks;
use praxis_hooks::HooksConfig;

use crate::config::Config;
use crate::shell::Shell;

pub(super) fn build(config: &Config, shell: &Shell) -> Hooks {
    let mut hook_shell_argv = shell.derive_exec_args("", /*use_login_shell*/ false);
    let hook_shell_program = hook_shell_argv.remove(0);
    let _ = hook_shell_argv.pop();
    Hooks::new(HooksConfig {
        legacy_notify_argv: config.notify.clone(),
        feature_enabled: config.features.enabled(Feature::PraxisHooks),
        config_layer_stack: Some(config.config_layer_stack.clone()),
        shell_program: Some(hook_shell_program),
        shell_args: hook_shell_argv,
    })
}
