use crate::config::NetworkProxySpec;

pub(super) fn with_exec_policy_network_rules(
    spec: &NetworkProxySpec,
    exec_policy: &praxis_execpolicy::Policy,
) -> NetworkProxySpec {
    spec.with_exec_policy_network_rules(exec_policy)
        .map_err(|err| {
            tracing::warn!(
                "failed to apply execpolicy network rules to managed proxy; continuing with configured network policy: {err}"
            );
            err
        })
        .unwrap_or_else(|_| spec.clone())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use praxis_execpolicy::Decision;
    use praxis_execpolicy::NetworkRuleProtocol;
    use praxis_execpolicy::Policy;
    use praxis_network_proxy::NetworkProxyConfig;
    use praxis_protocol::protocol::SandboxPolicy;

    use super::*;
    use crate::config_loader::NetworkConstraints;
    use crate::config_loader::NetworkDomainPermissionToml;
    use crate::config_loader::NetworkDomainPermissionsToml;

    #[test]
    fn overlays_valid_exec_policy_network_rules() {
        let sandbox = SandboxPolicy::new_workspace_write_policy();
        let spec = NetworkProxySpec::from_config_and_constraints(
            NetworkProxyConfig::default(),
            None,
            &sandbox,
        )
        .expect("base proxy spec should be valid");
        let mut policy = Policy::empty();
        policy
            .add_network_rule(
                "example.com",
                NetworkRuleProtocol::Https,
                Decision::Allow,
                None,
            )
            .expect("network rule should be valid");

        let actual = with_exec_policy_network_rules(&spec, &policy);
        let mut expected_config = NetworkProxyConfig::default();
        expected_config
            .network
            .set_allowed_domains(vec!["example.com".to_string()]);
        let expected =
            NetworkProxySpec::from_config_and_constraints(expected_config, None, &sandbox)
                .expect("overlaid proxy spec should be valid");

        assert_eq!(actual, expected);
    }

    #[test]
    fn preserves_configured_policy_when_overlay_violates_managed_constraints() {
        let sandbox = SandboxPolicy::new_workspace_write_policy();
        let constraints = NetworkConstraints {
            domains: Some(NetworkDomainPermissionsToml {
                entries: BTreeMap::from([(
                    "managed.example.com".to_string(),
                    NetworkDomainPermissionToml::Allow,
                )]),
            }),
            managed_allowed_domains_only: Some(true),
            ..Default::default()
        };
        let spec = NetworkProxySpec::from_config_and_constraints(
            NetworkProxyConfig::default(),
            Some(constraints),
            &sandbox,
        )
        .expect("managed proxy spec should be valid");
        let mut policy = Policy::empty();
        policy
            .add_network_rule(
                "outside-managed-policy.example.com",
                NetworkRuleProtocol::Https,
                Decision::Allow,
                None,
            )
            .expect("network rule should be valid");

        let actual = with_exec_policy_network_rules(&spec, &policy);

        assert_eq!(actual, spec);
    }
}
