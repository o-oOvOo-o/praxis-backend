use super::*;

#[test]
fn derive_requested_execpolicy_amendment_returns_none_for_missing_prefix_rule() {
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(/*prefix_rule*/ None, &[])
    );
}

#[test]
fn derive_requested_execpolicy_amendment_returns_none_for_empty_prefix_rule() {
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(Some(&Vec::new()), &[])
    );
}

#[test]
fn derive_requested_execpolicy_amendment_returns_none_for_exact_banned_prefix_rule() {
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(
            Some(&vec!["python".to_string(), "-c".to_string()]),
            &[],
        )
    );
}

#[test]
fn derive_requested_execpolicy_amendment_returns_none_for_windows_and_pypy_variants() {
    for prefix_rule in [
        vec!["py".to_string()],
        vec!["py".to_string(), "-3".to_string()],
        vec!["pythonw".to_string()],
        vec!["pyw".to_string()],
        vec!["pypy".to_string()],
        vec!["pypy3".to_string()],
    ] {
        assert_eq!(
            None,
            derive_requested_execpolicy_amendment_for_test(Some(&prefix_rule), &[])
        );
    }
}

#[test]
fn derive_requested_execpolicy_amendment_returns_none_for_shell_and_powershell_variants() {
    for prefix_rule in [
        vec!["bash".to_string(), "-lc".to_string()],
        vec!["sh".to_string(), "-c".to_string()],
        vec!["sh".to_string(), "-lc".to_string()],
        vec!["zsh".to_string(), "-lc".to_string()],
        vec!["/bin/bash".to_string(), "-lc".to_string()],
        vec!["/bin/zsh".to_string(), "-lc".to_string()],
        vec!["pwsh".to_string()],
        vec!["pwsh".to_string(), "-Command".to_string()],
        vec!["pwsh".to_string(), "-c".to_string()],
        vec!["powershell".to_string()],
        vec!["powershell".to_string(), "-Command".to_string()],
        vec!["powershell".to_string(), "-c".to_string()],
        vec!["powershell.exe".to_string()],
        vec!["powershell.exe".to_string(), "-Command".to_string()],
        vec!["powershell.exe".to_string(), "-c".to_string()],
    ] {
        assert_eq!(
            None,
            derive_requested_execpolicy_amendment_for_test(Some(&prefix_rule), &[])
        );
    }
}

#[test]
fn derive_requested_execpolicy_amendment_allows_non_exact_banned_prefix_rule_match() {
    let prefix_rule = vec![
        "python".to_string(),
        "-c".to_string(),
        "print('hi')".to_string(),
    ];

    assert_eq!(
        Some(ExecPolicyAmendment::new(prefix_rule.clone())),
        derive_requested_execpolicy_amendment_for_test(Some(&prefix_rule), &[])
    );
}

#[test]
fn derive_requested_execpolicy_amendment_returns_none_when_policy_matches() {
    let prefix_rule = vec!["cargo".to_string(), "build".to_string()];

    let matched_rules_prompt = vec![RuleMatch::PrefixRuleMatch {
        matched_prefix: vec!["cargo".to_string()],
        decision: Decision::Prompt,
        resolved_program: None,
        justification: None,
    }];
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(Some(&prefix_rule), &matched_rules_prompt),
        "should return none when prompt policy matches"
    );
    let matched_rules_allow = vec![RuleMatch::PrefixRuleMatch {
        matched_prefix: vec!["cargo".to_string()],
        decision: Decision::Allow,
        resolved_program: None,
        justification: None,
    }];
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(Some(&prefix_rule), &matched_rules_allow),
        "should return none when prompt policy matches"
    );
    let matched_rules_forbidden = vec![RuleMatch::PrefixRuleMatch {
        matched_prefix: vec!["cargo".to_string()],
        decision: Decision::Forbidden,
        resolved_program: None,
        justification: None,
    }];
    assert_eq!(
        None,
        derive_requested_execpolicy_amendment_for_test(
            Some(&prefix_rule),
            &matched_rules_forbidden,
        ),
        "should return none when prompt policy matches"
    );
}
