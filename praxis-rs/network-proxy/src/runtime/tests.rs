use super::*;

use crate::config::NetworkProxyConfig;
use crate::config::NetworkProxySettings;
use crate::policy::compile_allowlist_globset;
use crate::policy::compile_denylist_globset;
use crate::state::NetworkProxyConstraints;
use crate::state::build_config_state;
use crate::state::validate_policy_against_constraints;
use pretty_assertions::assert_eq;

fn strings(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|entry| (*entry).to_string()).collect()
}

fn network_settings(allowed_domains: &[&str], denied_domains: &[&str]) -> NetworkProxySettings {
    let mut network = NetworkProxySettings::default();
    if !allowed_domains.is_empty() {
        network.set_allowed_domains(strings(allowed_domains));
    }
    if !denied_domains.is_empty() {
        network.set_denied_domains(strings(denied_domains));
    }
    network
}

fn network_settings_with_unix_sockets(
    allowed_domains: &[&str],
    denied_domains: &[&str],
    unix_sockets: &[String],
) -> NetworkProxySettings {
    let mut network = network_settings(allowed_domains, denied_domains);
    if !unix_sockets.is_empty() {
        network.set_allow_unix_sockets(unix_sockets.to_vec());
    }
    network
}

#[tokio::test]
async fn host_blocked_denied_wins_over_allowed() {
    let state =
        network_proxy_state_for_policy(network_settings(&["example.com"], &["example.com"]));

    assert_eq!(
        state
            .host_blocked("example.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::Denied)
    );
}

#[tokio::test]
async fn host_blocked_requires_allowlist_match() {
    let state = network_proxy_state_for_policy(network_settings(&["example.com"], &[]));

    assert_eq!(
        state
            .host_blocked("example.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
    assert_eq!(
        // Use a public IP literal to avoid relying on ambient DNS behavior (some networks
        // resolve unknown hostnames to private IPs, which would trigger `not_allowed_local`).
        state.host_blocked("8.8.8.8", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowed)
    );
}

#[tokio::test]
async fn add_allowed_domain_removes_matching_deny_entry() {
    let state = network_proxy_state_for_policy(network_settings(&[], &["example.com"]));

    state.add_allowed_domain("ExAmPlE.CoM").await.unwrap();

    let (allowed, denied) = state.current_patterns().await.unwrap();
    assert_eq!(allowed, vec!["example.com".to_string()]);
    assert!(denied.is_empty());
    assert_eq!(
        state
            .host_blocked("example.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
}

#[tokio::test]
async fn add_denied_domain_removes_matching_allow_entry() {
    let state = network_proxy_state_for_policy(network_settings(&["example.com"], &[]));

    state.add_denied_domain("EXAMPLE.COM").await.unwrap();

    let (allowed, denied) = state.current_patterns().await.unwrap();
    assert!(allowed.is_empty());
    assert_eq!(denied, vec!["example.com".to_string()]);
    assert_eq!(
        state
            .host_blocked("example.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::Denied)
    );
}

#[tokio::test]
async fn add_denied_domain_forces_block_with_global_wildcard_allowlist() {
    let state = network_proxy_state_for_policy(network_settings(&["*"], &[]));

    assert_eq!(
        // Use a public IP literal to avoid relying on ambient DNS behavior.
        state.host_blocked("8.8.8.8", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Allowed
    );

    state.add_denied_domain("8.8.8.8").await.unwrap();

    let (allowed, denied) = state.current_patterns().await.unwrap();
    assert_eq!(allowed, vec!["*".to_string()]);
    assert_eq!(denied, vec!["8.8.8.8".to_string()]);
    assert_eq!(
        state.host_blocked("8.8.8.8", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::Denied)
    );
}

#[tokio::test]
async fn add_allowed_domain_succeeds_when_managed_baseline_allows_expansion() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["managed.example.com"], &[]);
            network.enabled = true;
            network
        },
    };
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["managed.example.com".to_string()]),
        allowlist_expansion_enabled: Some(true),
        ..NetworkProxyConstraints::default()
    };
    let state = NetworkProxyState::with_reloader(
        build_config_state(config, constraints).unwrap(),
        Arc::new(NoopReloader),
    );

    state.add_allowed_domain("user.example.com").await.unwrap();

    let (allowed, denied) = state.current_patterns().await.unwrap();
    assert_eq!(
        allowed,
        vec![
            "managed.example.com".to_string(),
            "user.example.com".to_string()
        ]
    );
    assert!(denied.is_empty());
}

#[tokio::test]
async fn add_allowed_domain_rejects_expansion_when_managed_baseline_is_fixed() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["managed.example.com"], &[]);
            network.enabled = true;
            network
        },
    };
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["managed.example.com".to_string()]),
        allowlist_expansion_enabled: Some(false),
        ..NetworkProxyConstraints::default()
    };
    let state = NetworkProxyState::with_reloader(
        build_config_state(config, constraints).unwrap(),
        Arc::new(NoopReloader),
    );

    let err = state
        .add_allowed_domain("user.example.com")
        .await
        .expect_err("managed baseline should reject allowlist expansion");

    assert!(
        format!("{err:#}").contains("network.allowed_domains constrained by managed config"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn add_denied_domain_rejects_expansion_when_managed_baseline_is_fixed() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&[], &["managed.example.com"]);
            network.enabled = true;
            network
        },
    };
    let constraints = NetworkProxyConstraints {
        denied_domains: Some(vec!["managed.example.com".to_string()]),
        denylist_expansion_enabled: Some(false),
        ..NetworkProxyConstraints::default()
    };
    let state = NetworkProxyState::with_reloader(
        build_config_state(config, constraints).unwrap(),
        Arc::new(NoopReloader),
    );

    let err = state
        .add_denied_domain("user.example.com")
        .await
        .expect_err("managed baseline should reject denylist expansion");

    assert!(
        format!("{err:#}").contains("network.denied_domains constrained by managed config"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn blocked_snapshot_does_not_consume_entries() {
    let state = network_proxy_state_for_policy(NetworkProxySettings::default());

    state
        .record_blocked(BlockedRequest::new(BlockedRequestArgs {
            host: "google.com".to_string(),
            reason: "not_allowed".to_string(),
            client: None,
            method: Some("GET".to_string()),
            mode: None,
            protocol: "http".to_string(),
            decision: Some("ask".to_string()),
            source: Some("decider".to_string()),
            port: Some(80),
        }))
        .await
        .expect("entry should be recorded");

    let snapshot = state
        .blocked_snapshot()
        .await
        .expect("snapshot should succeed");
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].host, "google.com");
    assert_eq!(snapshot[0].decision.as_deref(), Some("ask"));

    let drained = state
        .drain_blocked()
        .await
        .expect("drain should include snapshot entry");
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].host, snapshot[0].host);
    assert_eq!(drained[0].reason, snapshot[0].reason);
    assert_eq!(drained[0].decision, snapshot[0].decision);
    assert_eq!(drained[0].source, snapshot[0].source);
    assert_eq!(drained[0].port, snapshot[0].port);
}

#[tokio::test]
async fn drain_blocked_returns_buffered_window() {
    let state = network_proxy_state_for_policy(NetworkProxySettings::default());

    for idx in 0..(MAX_BLOCKED_EVENTS + 5) {
        state
            .record_blocked(BlockedRequest::new(BlockedRequestArgs {
                host: format!("example{idx}.com"),
                reason: "not_allowed".to_string(),
                client: None,
                method: Some("GET".to_string()),
                mode: None,
                protocol: "http".to_string(),
                decision: Some("ask".to_string()),
                source: Some("decider".to_string()),
                port: Some(80),
            }))
            .await
            .expect("entry should be recorded");
    }

    let blocked = state.drain_blocked().await.expect("drain should succeed");
    assert_eq!(blocked.len(), MAX_BLOCKED_EVENTS);
    assert_eq!(blocked[0].host, "example5.com");
}

#[test]
fn blocked_request_violation_log_line_serializes_payload() {
    let entry = BlockedRequest {
        host: "google.com".to_string(),
        reason: "not_allowed".to_string(),
        client: Some("127.0.0.1".to_string()),
        method: Some("GET".to_string()),
        mode: Some(NetworkMode::Full),
        protocol: "http".to_string(),
        decision: Some("ask".to_string()),
        source: Some("decider".to_string()),
        port: Some(80),
        timestamp: 1_735_689_600,
    };

    assert_eq!(
        blocked_request_violation_log_line(&entry),
        r#"PRAXIS_NETWORK_POLICY_VIOLATION {"host":"google.com","reason":"not_allowed","client":"127.0.0.1","method":"GET","mode":"full","protocol":"http","decision":"ask","source":"decider","port":80,"timestamp":1735689600}"#
    );
}

#[tokio::test]
async fn host_blocked_subdomain_wildcards_exclude_apex() {
    let state = network_proxy_state_for_policy(network_settings(&["*.openai.com"], &[]));

    assert_eq!(
        state
            .host_blocked("api.openai.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
    assert_eq!(
        state.host_blocked("openai.com", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowed)
    );
}

#[tokio::test]
async fn host_blocked_global_wildcard_allowlist_allows_public_hosts_except_denylist() {
    let state = network_proxy_state_for_policy(network_settings(&["*"], &["evil.example"]));

    assert_eq!(
        state
            .host_blocked("example.com", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
    assert_eq!(
        state
            .host_blocked("api.openai.com", /*port*/ 443)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
    assert_eq!(
        state
            .host_blocked("evil.example", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::Denied)
    );
}

#[tokio::test]
async fn host_blocked_rejects_loopback_when_local_binding_disabled() {
    let state = network_proxy_state_for_policy(network_settings(&["example.com"], &[]));

    assert_eq!(
        state.host_blocked("127.0.0.1", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
    assert_eq!(
        state.host_blocked("localhost", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
}

#[tokio::test]
async fn host_blocked_allows_loopback_when_explicitly_allowlisted_and_local_binding_disabled() {
    let state = network_proxy_state_for_policy(network_settings(&["localhost"], &[]));

    assert_eq!(
        state.host_blocked("localhost", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Allowed
    );
}

#[tokio::test]
async fn host_blocked_allows_private_ip_literal_when_explicitly_allowlisted() {
    let state = network_proxy_state_for_policy(network_settings(&["10.0.0.1"], &[]));

    assert_eq!(
        state.host_blocked("10.0.0.1", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Allowed
    );
}

#[tokio::test]
async fn host_blocked_rejects_scoped_ipv6_literal_when_not_allowlisted() {
    let state = network_proxy_state_for_policy(network_settings(&["example.com"], &[]));

    assert_eq!(
        state
            .host_blocked("fe80::1%lo0", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
}

#[tokio::test]
async fn host_blocked_allows_scoped_ipv6_literal_when_explicitly_allowlisted() {
    let state = network_proxy_state_for_policy(network_settings(&["fe80::1%lo0"], &[]));

    assert_eq!(
        state
            .host_blocked("fe80::1%lo0", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Allowed
    );
}

#[tokio::test]
async fn host_blocked_rejects_private_ip_literals_when_local_binding_disabled() {
    let state = network_proxy_state_for_policy(network_settings(&["example.com"], &[]));

    assert_eq!(
        state.host_blocked("10.0.0.1", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
}

#[tokio::test]
async fn host_blocked_rejects_loopback_when_allowlist_empty() {
    let state = network_proxy_state_for_policy(NetworkProxySettings::default());

    assert_eq!(
        state.host_blocked("127.0.0.1", /*port*/ 80).await.unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
}

#[tokio::test]
async fn host_blocked_rejects_allowlisted_hostname_when_dns_lookup_fails() {
    let mut network = NetworkProxySettings::default();
    network.set_allowed_domains(vec!["does-not-resolve.invalid".to_string()]);
    let state = network_proxy_state_for_policy(network);

    assert_eq!(
        state
            .host_blocked("does-not-resolve.invalid", /*port*/ 80)
            .await
            .unwrap(),
        HostBlockDecision::Blocked(HostBlockReason::NotAllowedLocal)
    );
}

#[test]
fn validate_policy_against_constraints_disallows_widening_allowed_domains() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["example.com".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["example.com", "evil.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_allows_expanding_allowed_domains_when_enabled() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["example.com".to_string()]),
        allowlist_expansion_enabled: Some(true),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["example.com", "api.openai.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_ok());
}

#[test]
fn validate_policy_against_constraints_disallows_widening_mode() {
    let constraints = NetworkProxyConstraints {
        mode: Some(NetworkMode::Limited),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            mode: NetworkMode::Full,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_allows_narrowing_wildcard_allowlist() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["*.example.com".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["api.example.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_ok());
}

#[test]
fn validate_policy_against_constraints_rejects_widening_wildcard_allowlist() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["*.example.com".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["**.example.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_rejects_global_wildcard_in_managed_allowlist() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["*".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["api.example.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_rejects_bracketed_global_wildcard_in_managed_allowlist() {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["[*]".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["api.example.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_rejects_double_wildcard_bracketed_global_wildcard_in_managed_allowlist()
 {
    let constraints = NetworkProxyConstraints {
        allowed_domains: Some(vec!["**.[*]".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["api.example.com"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_requires_managed_denied_domains_entries() {
    let constraints = NetworkProxyConstraints {
        denied_domains: Some(vec!["evil.com".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_disallows_expanding_denied_domains_when_fixed() {
    let constraints = NetworkProxyConstraints {
        denied_domains: Some(vec!["evil.com".to_string()]),
        denylist_expansion_enabled: Some(false),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&[], &["evil.com", "more-evil.com"]);
            network.enabled = true;
            network
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_disallows_enabling_when_managed_disabled() {
    let constraints = NetworkProxyConstraints {
        enabled: Some(false),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_disallows_allow_local_binding_when_managed_disabled() {
    let constraints = NetworkProxyConstraints {
        allow_local_binding: Some(false),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            allow_local_binding: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_disallows_allow_all_unix_sockets_without_managed_opt_in() {
    let constraints = NetworkProxyConstraints {
        dangerously_allow_all_unix_sockets: Some(false),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            dangerously_allow_all_unix_sockets: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_disallows_allow_all_unix_sockets_when_allowlist_is_managed()
{
    let constraints = NetworkProxyConstraints {
        allow_unix_sockets: Some(vec!["/tmp/allowed.sock".to_string()]),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            dangerously_allow_all_unix_sockets: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_err());
}

#[test]
fn validate_policy_against_constraints_allows_allow_all_unix_sockets_with_managed_opt_in() {
    let constraints = NetworkProxyConstraints {
        dangerously_allow_all_unix_sockets: Some(true),
        ..NetworkProxyConstraints::default()
    };

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            dangerously_allow_all_unix_sockets: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_ok());
}

#[test]
fn validate_policy_against_constraints_allows_allow_all_unix_sockets_when_unmanaged() {
    let constraints = NetworkProxyConstraints::default();

    let config = NetworkProxyConfig {
        network: NetworkProxySettings {
            enabled: true,
            dangerously_allow_all_unix_sockets: true,
            ..NetworkProxySettings::default()
        },
    };

    assert!(validate_policy_against_constraints(&config, &constraints).is_ok());
}

#[test]
fn compile_globset_is_case_insensitive() {
    let patterns = vec!["ExAmPle.CoM".to_string()];
    let set = compile_denylist_globset(&patterns).unwrap();
    assert!(set.is_match("example.com"));
    assert!(set.is_match("EXAMPLE.COM"));
}

#[test]
fn compile_globset_excludes_apex_for_subdomain_patterns() {
    let patterns = vec!["*.openai.com".to_string()];
    let set = compile_denylist_globset(&patterns).unwrap();
    assert!(set.is_match("api.openai.com"));
    assert!(!set.is_match("openai.com"));
    assert!(!set.is_match("evilopenai.com"));
}

#[test]
fn compile_globset_includes_apex_for_double_wildcard_patterns() {
    let patterns = vec!["**.openai.com".to_string()];
    let set = compile_denylist_globset(&patterns).unwrap();
    assert!(set.is_match("openai.com"));
    assert!(set.is_match("api.openai.com"));
    assert!(!set.is_match("evilopenai.com"));
}

#[test]
fn compile_globset_rejects_global_wildcard() {
    let patterns = vec!["*".to_string()];
    assert!(compile_denylist_globset(&patterns).is_err());
}

#[test]
fn compile_globset_allows_global_wildcard_when_enabled() {
    let patterns = vec!["*".to_string()];
    let set = compile_allowlist_globset(&patterns).unwrap();
    assert!(set.is_match("example.com"));
    assert!(set.is_match("api.openai.com"));
    assert!(set.is_match("localhost"));
}

#[test]
fn compile_globset_rejects_bracketed_global_wildcard() {
    let patterns = vec!["[*]".to_string()];
    assert!(compile_denylist_globset(&patterns).is_err());
}

#[test]
fn compile_globset_rejects_double_wildcard_bracketed_global_wildcard() {
    let patterns = vec!["**.[*]".to_string()];
    assert!(compile_denylist_globset(&patterns).is_err());
}

#[test]
fn compile_globset_dedupes_patterns_without_changing_behavior() {
    let patterns = vec!["example.com".to_string(), "example.com".to_string()];
    let set = compile_denylist_globset(&patterns).unwrap();
    assert!(set.is_match("example.com"));
    assert!(set.is_match("EXAMPLE.COM"));
    assert!(!set.is_match("not-example.com"));
}

#[test]
fn compile_globset_rejects_invalid_patterns() {
    let patterns = vec!["[".to_string()];
    assert!(compile_denylist_globset(&patterns).is_err());
}

#[test]
fn build_config_state_allows_global_wildcard_allowed_domains() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["*"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(build_config_state(config, NetworkProxyConstraints::default()).is_ok());
}

#[test]
fn build_config_state_allows_bracketed_global_wildcard_allowed_domains() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["[*]"], &[]);
            network.enabled = true;
            network
        },
    };

    assert!(build_config_state(config, NetworkProxyConstraints::default()).is_ok());
}

#[test]
fn build_config_state_rejects_global_wildcard_denied_domains() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["example.com"], &["*"]);
            network.enabled = true;
            network
        },
    };

    assert!(build_config_state(config, NetworkProxyConstraints::default()).is_err());
}

#[test]
fn build_config_state_rejects_bracketed_global_wildcard_denied_domains() {
    let config = NetworkProxyConfig {
        network: {
            let mut network = network_settings(&["example.com"], &["[*]"]);
            network.enabled = true;
            network
        },
    };

    assert!(build_config_state(config, NetworkProxyConstraints::default()).is_err());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn unix_socket_allowlist_is_respected_on_macos() {
    let socket_path = "/tmp/example.sock".to_string();
    let state = network_proxy_state_for_policy(network_settings_with_unix_sockets(
        &["example.com"],
        &[],
        std::slice::from_ref(&socket_path),
    ));

    assert!(state.is_unix_socket_allowed(&socket_path).await.unwrap());
    assert!(
        !state
            .is_unix_socket_allowed("/tmp/not-allowed.sock")
            .await
            .unwrap()
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn unix_socket_allowlist_resolves_symlinks() {
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let dir = temp_dir.path();

    let real = dir.join("real.sock");
    let link = dir.join("link.sock");

    // The allowlist mechanism is path-based; for test purposes we don't need an actual unix
    // domain socket. Any filesystem entry works for canonicalization.
    std::fs::write(&real, b"not a socket").unwrap();
    symlink(&real, &link).unwrap();

    let real_s = real.to_str().unwrap().to_string();
    let link_s = link.to_str().unwrap().to_string();

    let state = network_proxy_state_for_policy(network_settings_with_unix_sockets(
        &["example.com"],
        &[],
        std::slice::from_ref(&real_s),
    ));

    assert!(state.is_unix_socket_allowed(&link_s).await.unwrap());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn unix_socket_allow_all_flag_bypasses_allowlist() {
    let state = network_proxy_state_for_policy({
        let mut network = network_settings(&["example.com"], &[]);
        network.dangerously_allow_all_unix_sockets = true;
        network
    });

    assert!(state.is_unix_socket_allowed("/tmp/any.sock").await.unwrap());
    assert!(!state.is_unix_socket_allowed("relative.sock").await.unwrap());
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn unix_socket_allowlist_is_rejected_on_non_macos() {
    let socket_path = "/tmp/example.sock".to_string();
    let state = network_proxy_state_for_policy({
        let mut network = network_settings_with_unix_sockets(
            &["example.com"],
            &[],
            std::slice::from_ref(&socket_path),
        );
        network.dangerously_allow_all_unix_sockets = true;
        network
    });

    assert!(!state.is_unix_socket_allowed(&socket_path).await.unwrap());
}
