use praxis_app_gateway_protocol::AdditionalFileSystemPermissions;
use praxis_app_gateway_protocol::AdditionalNetworkPermissions;
use praxis_app_gateway_protocol::GrantedPermissionProfile;
use praxis_app_gateway_protocol::NetworkApprovalContext as AppGatewayNetworkApprovalContext;
use praxis_protocol::protocol::NetworkApprovalContext;
use praxis_protocol::protocol::NetworkApprovalProtocol;
use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;

pub(crate) fn network_approval_context_to_core(
    value: AppGatewayNetworkApprovalContext,
) -> NetworkApprovalContext {
    NetworkApprovalContext {
        host: value.host,
        protocol: match value.protocol {
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Http => {
                NetworkApprovalProtocol::Http
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Https => {
                NetworkApprovalProtocol::Https
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Tcp => {
                NetworkApprovalProtocol::Socks5Tcp
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Udp => {
                NetworkApprovalProtocol::Socks5Udp
            }
        },
    }
}

pub(crate) fn granted_permission_profile_from_request(
    value: CoreRequestPermissionProfile,
) -> GrantedPermissionProfile {
    GrantedPermissionProfile {
        network: value.network.map(|network| AdditionalNetworkPermissions {
            enabled: network.enabled,
        }),
        file_system: value
            .file_system
            .map(|file_system| AdditionalFileSystemPermissions {
                read: file_system.read,
                write: file_system.write,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::granted_permission_profile_from_request;
    use super::network_approval_context_to_core;
    use praxis_protocol::models::FileSystemPermissions;
    use praxis_protocol::models::NetworkPermissions;
    use praxis_protocol::protocol::NetworkApprovalContext;
    use praxis_protocol::protocol::NetworkApprovalProtocol;
    use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
    use praxis_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    fn absolute_path(path: &str) -> AbsolutePathBuf {
        AbsolutePathBuf::try_from(PathBuf::from(path)).expect("path must be absolute")
    }

    #[test]
    fn converts_app_gateway_network_approval_context_to_core() {
        assert_eq!(
            network_approval_context_to_core(praxis_app_gateway_protocol::NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Tcp,
            }),
            NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: NetworkApprovalProtocol::Socks5Tcp,
            }
        );
    }

    #[test]
    fn converts_request_permissions_into_granted_permissions() {
        assert_eq!(
            granted_permission_profile_from_request(CoreRequestPermissionProfile {
                network: Some(NetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(FileSystemPermissions {
                    read: Some(vec![absolute_path("/tmp/read-only")]),
                    write: Some(vec![absolute_path("/tmp/write")]),
                }),
            }),
            praxis_app_gateway_protocol::GrantedPermissionProfile {
                network: Some(praxis_app_gateway_protocol::AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(
                    praxis_app_gateway_protocol::AdditionalFileSystemPermissions {
                        read: Some(vec![absolute_path("/tmp/read-only")]),
                        write: Some(vec![absolute_path("/tmp/write")]),
                    }
                ),
            }
        );
    }
}
