use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use crate::config::StartedNetworkProxy;

pub(super) fn from_started_proxy(
    network_proxy: &StartedNetworkProxy,
) -> SessionNetworkProxyRuntime {
    let proxy = network_proxy.proxy();
    SessionNetworkProxyRuntime {
        http_addr: proxy.http_addr().to_string(),
        socks_addr: proxy.socks_addr().to_string(),
    }
}
