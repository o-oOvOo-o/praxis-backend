use http::HeaderMap;
use http::HeaderValue;
use praxis_client::Request;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthScheme {
    #[default]
    Bearer,
    XApiKey,
}

/// Provides an authentication secret, its header scheme, and optional account identity.
///
/// Implementations should be cheap and non-blocking; any asynchronous
/// refresh or I/O should be handled by higher layers before requests
/// reach this interface.
pub trait AuthProvider: Send + Sync {
    fn bearer_token(&self) -> Option<&str>;
    fn auth_scheme(&self) -> AuthScheme {
        AuthScheme::Bearer
    }
    fn account_id(&self) -> Option<&str> {
        None
    }
}

pub(crate) fn add_auth_headers_to_header_map<A: AuthProvider>(auth: &A, headers: &mut HeaderMap) {
    if let Some(token) = auth.bearer_token() {
        match auth.auth_scheme() {
            AuthScheme::Bearer if !headers.contains_key(http::header::AUTHORIZATION) => {
                let encoded = Zeroizing::new(format!("Bearer {token}"));
                if let Ok(mut header) = HeaderValue::from_str(encoded.as_str()) {
                    header.set_sensitive(true);
                    let _ = headers.insert(http::header::AUTHORIZATION, header);
                }
            }
            AuthScheme::XApiKey if !headers.contains_key("x-api-key") => {
                if let Ok(mut header) = HeaderValue::from_str(token) {
                    header.set_sensitive(true);
                    let _ = headers.insert("x-api-key", header);
                }
            }
            AuthScheme::Bearer | AuthScheme::XApiKey => {}
        }
    }
    if let Some(account_id) = auth.account_id()
        && let Ok(mut header) = HeaderValue::from_str(account_id)
    {
        header.set_sensitive(true);
        let _ = headers.insert("ChatGPT-Account-ID", header);
    }
}

pub(crate) fn add_auth_headers<A: AuthProvider>(auth: &A, mut req: Request) -> Request {
    add_auth_headers_to_header_map(auth, &mut req.headers);
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticAuth(&'static str);

    impl AuthProvider for StaticAuth {
        fn bearer_token(&self) -> Option<&str> {
            Some(self.0)
        }
    }

    #[test]
    fn authorization_header_is_marked_sensitive() {
        let mut headers = HeaderMap::new();
        add_auth_headers_to_header_map(&StaticAuth("secret-token"), &mut headers);

        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret-token")
        );
        assert!(!format!("{headers:?}").contains("secret-token"));
    }

    struct StaticApiKey(&'static str);

    impl AuthProvider for StaticApiKey {
        fn bearer_token(&self) -> Option<&str> {
            Some(self.0)
        }

        fn auth_scheme(&self) -> AuthScheme {
            AuthScheme::XApiKey
        }
    }

    #[test]
    fn api_key_header_is_marked_sensitive() {
        let mut headers = HeaderMap::new();
        add_auth_headers_to_header_map(&StaticApiKey("secret-key"), &mut headers);

        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("secret-key")
        );
        assert!(!format!("{headers:?}").contains("secret-key"));
    }
}
