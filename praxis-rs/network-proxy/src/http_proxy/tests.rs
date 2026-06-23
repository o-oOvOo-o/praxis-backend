use super::*;

use crate::config::NetworkMode;
use crate::config::NetworkProxySettings;
use crate::runtime::network_proxy_state_for_policy;
use pretty_assertions::assert_eq;
use rama_http::Method;
use rama_http::Request;
use std::net::Ipv4Addr;
use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn http_connect_accept_blocks_in_limited_mode() {
    let policy = {
        let mut policy = NetworkProxySettings::default();
        policy.set_allowed_domains(vec!["example.com".to_string()]);
        policy
    };
    let state = Arc::new(network_proxy_state_for_policy(policy));
    state.set_network_mode(NetworkMode::Limited).await.unwrap();

    let mut req = Request::builder()
        .method(Method::CONNECT)
        .uri("https://example.com:443")
        .header("host", "example.com:443")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(state);

    let response = http_connect_accept(/*policy_decider*/ None, req)
        .await
        .unwrap_err();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("x-proxy-error").unwrap(),
        "blocked-by-mitm-required"
    );
}

#[tokio::test]
async fn http_connect_accept_allows_allowlisted_host_in_full_mode() {
    let policy = {
        let mut policy = NetworkProxySettings::default();
        policy.set_allowed_domains(vec!["example.com".to_string()]);
        policy
    };
    let state = Arc::new(network_proxy_state_for_policy(policy));

    let mut req = Request::builder()
        .method(Method::CONNECT)
        .uri("https://example.com:443")
        .header("host", "example.com:443")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(state);

    let (response, _request) = http_connect_accept(/*policy_decider*/ None, req)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn http_proxy_listener_accepts_plain_http1_connect_requests() {
    let target_listener = TokioTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("target listener should bind");
    let target_addr = target_listener
        .local_addr()
        .expect("target listener should expose local addr");
    let target_task = tokio::spawn(async move {
        let (mut stream, _) = target_listener
            .accept()
            .await
            .expect("target listener should accept");
        let mut buf = [0_u8; 1];
        let _ = timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
    });

    let state = Arc::new(network_proxy_state_for_policy({
        let mut network = NetworkProxySettings::default();
        network.set_allowed_domains(vec!["127.0.0.1".to_string()]);
        network.allow_local_binding = true;
        network
    }));
    let listener =
        StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("proxy listener should bind");
    let proxy_addr = listener
        .local_addr()
        .expect("proxy listener should expose local addr");
    let proxy_task = tokio::spawn(run_http_proxy_with_std_listener(
        state, listener, /*policy_decider*/ None,
    ));

    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .expect("client should connect to proxy");
    let request = format!(
        "CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n",
        port = target_addr.port()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("client should write CONNECT request");

    let mut buf = [0_u8; 256];
    let bytes_read = timeout(Duration::from_secs(2), stream.read(&mut buf))
        .await
        .expect("proxy should respond before timeout")
        .expect("client should read proxy response");
    let response = String::from_utf8_lossy(&buf[..bytes_read]);
    assert!(
        response.starts_with("HTTP/1.1 200 OK\r\n"),
        "unexpected proxy response: {response:?}"
    );

    drop(stream);
    proxy_task.abort();
    let _ = proxy_task.await;
    target_task.abort();
    let _ = target_task.await;
}

#[tokio::test(flavor = "current_thread")]
async fn http_plain_proxy_blocks_unix_socket_when_method_not_allowed() {
    let state = Arc::new(network_proxy_state_for_policy(
        NetworkProxySettings::default(),
    ));
    state
        .set_network_mode(NetworkMode::Limited)
        .await
        .expect("network mode should update");

    let mut req = Request::builder()
        .method(Method::POST)
        .uri("http://example.com")
        .header("x-unix-socket", "/tmp/test.sock")
        .body(Body::empty())
        .expect("request should build");
    req.extensions_mut().insert(state);

    let response = http_plain_proxy(/*policy_decider*/ None, req)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("x-proxy-error").unwrap(),
        "blocked-by-method-policy"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_plain_proxy_rejects_unix_socket_when_not_allowlisted() {
    let state = Arc::new(network_proxy_state_for_policy(
        NetworkProxySettings::default(),
    ));

    let mut req = Request::builder()
        .method(Method::GET)
        .uri("http://example.com")
        .header("x-unix-socket", "/tmp/test.sock")
        .body(Body::empty())
        .expect("request should build");
    req.extensions_mut().insert(state);

    let response = http_plain_proxy(/*policy_decider*/ None, req)
        .await
        .unwrap();

    if cfg!(target_os = "macos") {
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get("x-proxy-error").unwrap(),
            "blocked-by-allowlist"
        );
    } else {
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "current_thread")]
async fn http_plain_proxy_attempts_allowed_unix_socket_proxy() {
    let state = Arc::new(network_proxy_state_for_policy({
        let mut network = NetworkProxySettings::default();
        network.set_allow_unix_sockets(vec!["/tmp/test.sock".to_string()]);
        network
    }));

    let mut req = Request::builder()
        .method(Method::GET)
        .uri("http://example.com")
        .header("x-unix-socket", "/tmp/test.sock")
        .body(Body::empty())
        .expect("request should build");
    req.extensions_mut().insert(state);

    let response = http_plain_proxy(/*policy_decider*/ None, req)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn http_connect_accept_denies_denylisted_host() {
    let policy = {
        let mut policy = NetworkProxySettings::default();
        policy.set_allowed_domains(vec!["**.openai.com".to_string()]);
        policy.set_denied_domains(vec!["api.openai.com".to_string()]);
        policy
    };
    let state = Arc::new(network_proxy_state_for_policy(policy));

    let mut req = Request::builder()
        .method(Method::CONNECT)
        .uri("https://api.openai.com:443")
        .header("host", "api.openai.com:443")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(state);

    let response = http_connect_accept(/*policy_decider*/ None, req)
        .await
        .unwrap_err();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers().get("x-proxy-error").unwrap(),
        "blocked-by-denylist"
    );
}

#[tokio::test]
async fn http_plain_proxy_rejects_absolute_uri_host_header_mismatch() {
    let state = Arc::new(network_proxy_state_for_policy(
        NetworkProxySettings::default(),
    ));
    let mut req = Request::builder()
        .method(Method::GET)
        .uri("http://raw.githubusercontent.com/cunning3d/praxis/main/README.md")
        .header(header::HOST, "api.github.com")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(state);

    let response = http_plain_proxy(/*policy_decider*/ None, req).await;
    assert_eq!(response.unwrap().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn validate_absolute_form_host_header_allows_matching_default_port() {
    let req = Request::builder()
        .method(Method::GET)
        .uri("http://example.com/")
        .header("host", "example.com")
        .body(Body::empty())
        .unwrap();

    assert_eq!(
        validate_absolute_form_host_header(&req, &RequestContext::try_from(&req).unwrap(),),
        Ok(())
    );
}

#[test]
fn validate_absolute_form_host_header_rejects_mismatched_host() {
    let req = Request::builder()
        .method(Method::GET)
        .uri("http://raw.githubusercontent.com/")
        .header("host", "api.github.com")
        .body(Body::empty())
        .unwrap();

    assert_eq!(
        validate_absolute_form_host_header(&req, &RequestContext::try_from(&req).unwrap(),),
        Err("Host header does not match request target")
    );
}

#[test]
fn validate_absolute_form_host_header_rejects_missing_non_default_port() {
    let req = Request::builder()
        .method(Method::GET)
        .uri("http://example.com:8080/")
        .header("host", "example.com")
        .body(Body::empty())
        .unwrap();

    assert_eq!(
        validate_absolute_form_host_header(&req, &RequestContext::try_from(&req).unwrap(),),
        Err("Host header does not match request target")
    );
}

#[test]
fn remove_hop_by_hop_request_headers_keeps_forwarding_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONNECTION,
        HeaderValue::from_static("x-hop, keep-alive"),
    );
    headers.insert("x-hop", HeaderValue::from_static("1"));
    headers.insert(
        header::PROXY_AUTHORIZATION,
        HeaderValue::from_static("Basic abc"),
    );
    headers.insert(
        &header::X_FORWARDED_FOR,
        HeaderValue::from_static("127.0.0.1"),
    );
    headers.insert(header::HOST, HeaderValue::from_static("example.com"));

    remove_hop_by_hop_request_headers(&mut headers);

    assert_eq!(headers.get(header::CONNECTION), None);
    assert_eq!(headers.get("x-hop"), None);
    assert_eq!(headers.get(header::PROXY_AUTHORIZATION), None);
    assert_eq!(
        headers.get(&header::X_FORWARDED_FOR),
        Some(&HeaderValue::from_static("127.0.0.1"))
    );
    assert_eq!(
        headers.get(header::HOST),
        Some(&HeaderValue::from_static("example.com"))
    );
}
