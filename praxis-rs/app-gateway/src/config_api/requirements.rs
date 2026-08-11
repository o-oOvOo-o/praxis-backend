use praxis_app_gateway_protocol::ConfigRequirements;
use praxis_app_gateway_protocol::NetworkDomainPermission;
use praxis_app_gateway_protocol::NetworkRequirements;
use praxis_app_gateway_protocol::NetworkUnixSocketPermission;
use praxis_app_gateway_protocol::SandboxMode;
use praxis_core::config_loader::ConfigRequirementsToml;
use praxis_core::config_loader::ResidencyRequirement as CoreResidencyRequirement;
use praxis_core::config_loader::SandboxModeRequirement as CoreSandboxModeRequirement;
use praxis_protocol::config_types::WebSearchMode;

pub(super) fn map_requirements_toml_to_api(
    requirements: ConfigRequirementsToml,
) -> ConfigRequirements {
    ConfigRequirements {
        allowed_approval_policies: requirements.allowed_approval_policies.map(|policies| {
            policies
                .into_iter()
                .map(praxis_app_gateway_protocol::AskForApproval::from)
                .collect()
        }),
        allowed_sandbox_modes: requirements.allowed_sandbox_modes.map(|modes| {
            modes
                .into_iter()
                .filter_map(map_sandbox_mode_requirement_to_api)
                .collect()
        }),
        allowed_web_search_modes: requirements.allowed_web_search_modes.map(|modes| {
            let mut normalized = modes
                .into_iter()
                .map(Into::into)
                .collect::<Vec<WebSearchMode>>();
            if !normalized.contains(&WebSearchMode::Disabled) {
                normalized.push(WebSearchMode::Disabled);
            }
            normalized
        }),
        feature_requirements: requirements
            .feature_requirements
            .map(|requirements| requirements.entries),
        enforce_residency: requirements
            .enforce_residency
            .map(map_residency_requirement_to_api),
        network: requirements.network.map(map_network_requirements_to_api),
    }
}

pub(super) fn map_sandbox_mode_requirement_to_api(
    mode: CoreSandboxModeRequirement,
) -> Option<SandboxMode> {
    match mode {
        CoreSandboxModeRequirement::ReadOnly => Some(SandboxMode::ReadOnly),
        CoreSandboxModeRequirement::WorkspaceWrite => Some(SandboxMode::WorkspaceWrite),
        CoreSandboxModeRequirement::DangerFullAccess => Some(SandboxMode::DangerFullAccess),
        CoreSandboxModeRequirement::ExternalSandbox => None,
    }
}

pub(super) fn map_residency_requirement_to_api(
    residency: CoreResidencyRequirement,
) -> praxis_app_gateway_protocol::ResidencyRequirement {
    match residency {
        CoreResidencyRequirement::Us => praxis_app_gateway_protocol::ResidencyRequirement::Us,
    }
}

pub(super) fn map_network_requirements_to_api(
    network: praxis_core::config_loader::NetworkRequirementsToml,
) -> NetworkRequirements {
    NetworkRequirements {
        enabled: network.enabled,
        http_port: network.http_port,
        socks_port: network.socks_port,
        allow_upstream_proxy: network.allow_upstream_proxy,
        dangerously_allow_non_loopback_proxy: network.dangerously_allow_non_loopback_proxy,
        dangerously_allow_all_unix_sockets: network.dangerously_allow_all_unix_sockets,
        domains: network.domains.map(|domains| {
            domains
                .entries
                .into_iter()
                .map(|(pattern, permission)| {
                    (pattern, map_network_domain_permission_to_api(permission))
                })
                .collect()
        }),
        managed_allowed_domains_only: network.managed_allowed_domains_only,
        unix_sockets: network.unix_sockets.map(|unix_sockets| {
            unix_sockets
                .entries
                .into_iter()
                .map(|(path, permission)| {
                    (path, map_network_unix_socket_permission_to_api(permission))
                })
                .collect()
        }),
        allow_local_binding: network.allow_local_binding,
    }
}

pub(super) fn map_network_domain_permission_to_api(
    permission: praxis_core::config_loader::NetworkDomainPermissionToml,
) -> NetworkDomainPermission {
    match permission {
        praxis_core::config_loader::NetworkDomainPermissionToml::Allow => {
            NetworkDomainPermission::Allow
        }
        praxis_core::config_loader::NetworkDomainPermissionToml::Deny => {
            NetworkDomainPermission::Deny
        }
    }
}

pub(super) fn map_network_unix_socket_permission_to_api(
    permission: praxis_core::config_loader::NetworkUnixSocketPermissionToml,
) -> NetworkUnixSocketPermission {
    match permission {
        praxis_core::config_loader::NetworkUnixSocketPermissionToml::Allow => {
            NetworkUnixSocketPermission::Allow
        }
        praxis_core::config_loader::NetworkUnixSocketPermissionToml::None => {
            NetworkUnixSocketPermission::None
        }
    }
}
