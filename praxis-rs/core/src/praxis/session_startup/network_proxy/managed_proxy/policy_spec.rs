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
