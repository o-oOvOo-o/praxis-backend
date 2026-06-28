use super::*;
use praxis_config::types::WindowsToml;
use praxis_features::Features;
use pretty_assertions::assert_eq;

#[test]
fn elevated_flag_works_by_itself() {
    let mut features = Features::with_defaults();
    features.enable(Feature::WindowsSandboxElevated);

    assert_eq!(
        WindowsSandboxLevel::from_features(&features),
        WindowsSandboxLevel::Elevated
    );
}

#[test]
fn restricted_token_flag_works_by_itself() {
    let mut features = Features::with_defaults();
    features.enable(Feature::WindowsSandbox);

    assert_eq!(
        WindowsSandboxLevel::from_features(&features),
        WindowsSandboxLevel::RestrictedToken
    );
}

#[test]
fn no_flags_means_no_sandbox() {
    let features = Features::with_defaults();

    assert_eq!(
        WindowsSandboxLevel::from_features(&features),
        WindowsSandboxLevel::Disabled
    );
}

#[test]
fn elevated_wins_when_both_flags_are_enabled() {
    let mut features = Features::with_defaults();
    features.enable(Feature::WindowsSandbox);
    features.enable(Feature::WindowsSandboxElevated);

    assert_eq!(
        WindowsSandboxLevel::from_features(&features),
        WindowsSandboxLevel::Elevated
    );
}

#[test]
fn resolve_windows_sandbox_mode_prefers_profile_windows() {
    let cfg = ConfigToml {
        windows: Some(WindowsToml {
            sandbox: Some(WindowsSandboxModeToml::Unelevated),
            ..Default::default()
        }),
        ..Default::default()
    };
    let profile = ConfigProfile {
        windows: Some(WindowsToml {
            sandbox: Some(WindowsSandboxModeToml::Elevated),
            ..Default::default()
        }),
        ..Default::default()
    };

    assert_eq!(
        resolve_windows_sandbox_mode(&cfg, &profile),
        Some(WindowsSandboxModeToml::Elevated)
    );
}

#[test]
fn resolve_windows_sandbox_private_desktop_prefers_profile_windows() {
    let cfg = ConfigToml {
        windows: Some(WindowsToml {
            sandbox: Some(WindowsSandboxModeToml::Unelevated),
            sandbox_private_desktop: Some(false),
        }),
        ..Default::default()
    };
    let profile = ConfigProfile {
        windows: Some(WindowsToml {
            sandbox: Some(WindowsSandboxModeToml::Elevated),
            sandbox_private_desktop: Some(true),
        }),
        ..Default::default()
    };

    assert!(resolve_windows_sandbox_private_desktop(&cfg, &profile));
}

#[test]
fn resolve_windows_sandbox_private_desktop_defaults_to_true() {
    assert!(resolve_windows_sandbox_private_desktop(
        &ConfigToml::default(),
        &ConfigProfile::default()
    ));
}

#[test]
fn resolve_windows_sandbox_private_desktop_respects_explicit_cfg_value() {
    let cfg = ConfigToml {
        windows: Some(WindowsToml {
            sandbox_private_desktop: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    };

    assert!(!resolve_windows_sandbox_private_desktop(
        &cfg,
        &ConfigProfile::default()
    ));
}
