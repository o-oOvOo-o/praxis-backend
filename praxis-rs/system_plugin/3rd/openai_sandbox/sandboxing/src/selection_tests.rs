use super::*;

#[test]
fn selection_requires_an_available_backend_only_when_isolation_is_required() {
    let platforms = [
        None,
        Some(SandboxType::None),
        Some(SandboxType::MacosSeatbelt),
        Some(SandboxType::LinuxSeccomp),
        Some(SandboxType::WindowsRestrictedToken),
    ];
    for platform in platforms {
        for (preference, policy_requires_sandbox, required) in [
            (SandboxablePreference::Auto, false, false),
            (SandboxablePreference::Auto, true, true),
            (SandboxablePreference::Require, false, true),
            (SandboxablePreference::Require, true, true),
            (SandboxablePreference::Forbid, false, false),
            (SandboxablePreference::Forbid, true, false),
        ] {
            let expected = match (required, platform) {
                (false, _) => Ok(SandboxType::None),
                (true, None | Some(SandboxType::None)) => {
                    Err(SandboxSelectionError::RequiredSandboxUnavailable)
                }
                (true, Some(backend)) => Ok(backend),
            };
            assert_eq!(
                select_sandbox(preference, policy_requires_sandbox, platform),
                expected,
                "{preference:?}, required={policy_requires_sandbox}, platform={platform:?}"
            );
        }
    }
}
