use super::*;
use praxis_app_gateway_client::AppGatewayRequestHandle;
use praxis_app_gateway_client::DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY;
use praxis_app_gateway_client::NativeAppGatewayClient;
use praxis_app_gateway_client::NativeAppGatewayClientStartArgs;
use praxis_app_gateway_client::NativeControlAuthSettings;
use praxis_arg0::Arg0DispatchPaths;
use praxis_cloud_requirements::cloud_config_bundle_loader_for_storage;
use praxis_core::config::ConfigBuilder;

use praxis_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use tempfile::TempDir;

async fn widget_forced_chatgpt() -> (AuthModeWidget, TempDir) {
    let praxis_home = TempDir::new().unwrap();
    let praxis_home_path = praxis_home.path().to_path_buf();
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home_path.clone())
        .build()
        .await
        .unwrap();
    let client = NativeAppGatewayClient::start(NativeAppGatewayClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config: Arc::new(config),
        cli_overrides: Vec::new(),
        loader_overrides: Default::default(),
        cloud_requirements: cloud_config_bundle_loader_for_storage(
            praxis_home_path.clone(),
            /*enable_praxis_api_key_env*/ false,
            AuthCredentialsStoreMode::File,
            "https://chatgpt.com/backend-api/".to_string(),
        ),
        feedback: praxis_feedback::PraxisFeedback::new(),
        config_warnings: Vec::new(),
        session_source: SessionSource::Cli,
        enable_praxis_api_key_env: false,
        client_name: "test".to_string(),
        client_version: "test".to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: DEFAULT_NATIVE_GATEWAY_CHANNEL_CAPACITY,
        control_listen: None,
        control_auth: NativeControlAuthSettings::default(),
    })
    .await
    .unwrap();
    let widget = AuthModeWidget {
        request_frame: FrameRequester::test_dummy(),
        highlighted_mode: SignInOption::ChatGpt,
        error: Arc::new(RwLock::new(None)),
        sign_in_state: Arc::new(RwLock::new(SignInState::PickMode)),
        praxis_home: praxis_home_path.clone(),
        cli_auth_credentials_store_mode: AuthCredentialsStoreMode::File,
        login_status: LoginStatus::NotAuthenticated,
        app_gateway_request_handle: AppGatewayRequestHandle::Native(client.request_handle()),
        forced_chatgpt_workspace_id: None,
        forced_login_method: Some(ForcedLoginMethod::Chatgpt),
        animations_enabled: true,
    };
    (widget, praxis_home)
}

#[tokio::test]
async fn api_key_flow_disabled_when_chatgpt_forced() {
    let (mut widget, _tmp) = widget_forced_chatgpt().await;

    widget.start_provider_key_entry(ProviderSetupKind::DeepSeek);

    assert_eq!(
        widget.error_message().as_deref(),
        Some(API_KEY_DISABLED_MESSAGE)
    );
    assert!(matches!(
        &*widget.sign_in_state.read().unwrap(),
        SignInState::PickMode
    ));
}

#[tokio::test]
async fn saving_api_key_is_blocked_when_chatgpt_forced() {
    let (mut widget, _tmp) = widget_forced_chatgpt().await;

    let mut state = ApiKeyInputState::new(ProviderSetupKind::DeepSeek);
    state.api_key = Zeroizing::new("sk-test".to_string());
    widget.save_provider_key(state);

    assert_eq!(
        widget.error_message().as_deref(),
        Some(API_KEY_DISABLED_MESSAGE)
    );
    assert!(matches!(
        &*widget.sign_in_state.read().unwrap(),
        SignInState::PickMode
    ));
    assert_eq!(widget.login_status, LoginStatus::NotAuthenticated);
}

#[tokio::test]
async fn existing_chatgpt_auth_tokens_login_counts_as_signed_in() {
    let (mut widget, _tmp) = widget_forced_chatgpt().await;
    widget.login_status = LoginStatus::AuthMode(AppGatewayAuthMode::ChatgptAuthTokens);

    let handled = widget.handle_existing_chatgpt_login();

    assert_eq!(handled, true);
    assert!(matches!(
        &*widget.sign_in_state.read().unwrap(),
        SignInState::ChatGptSuccess
    ));
}

#[tokio::test]
async fn cancel_active_attempt_resets_browser_login_state() {
    let (widget, _tmp) = widget_forced_chatgpt().await;
    *widget.error.write().unwrap() = Some("still logging in".to_string());
    *widget.sign_in_state.write().unwrap() =
        SignInState::ChatGptContinueInBrowser(ContinueInBrowserState {
            login_id: "login-1".to_string(),
            auth_url: "https://auth.example.com".to_string(),
        });

    widget.cancel_active_attempt();

    assert_eq!(widget.error_message(), None);
    assert!(matches!(
        &*widget.sign_in_state.read().unwrap(),
        SignInState::PickMode
    ));
}

#[tokio::test]
async fn cancel_active_attempt_notifies_device_code_login() {
    let (widget, _tmp) = widget_forced_chatgpt().await;
    let cancel = Arc::new(Notify::new());
    *widget.error.write().unwrap() = Some("still logging in".to_string());
    *widget.sign_in_state.write().unwrap() =
        SignInState::ChatGptDeviceCode(ContinueWithDeviceCodeState {
            device_code: None,
            cancel: Some(cancel.clone()),
        });

    widget.cancel_active_attempt();

    assert_eq!(widget.error_message(), None);
    assert!(matches!(
        &*widget.sign_in_state.read().unwrap(),
        SignInState::PickMode
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), cancel.notified())
            .await
            .is_ok()
    );
}

/// Collects all buffer cell symbols that contain the OSC 8 open sequence
/// for the given URL.  Returns the concatenated "inner" characters.
fn collect_osc8_chars(buf: &Buffer, area: Rect, url: &str) -> String {
    let open = format!("\x1B]8;;{url}\x07");
    let close = "\x1B]8;;\x07";
    let mut chars = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let sym = buf[(x, y)].symbol();
            if let Some(rest) = sym.strip_prefix(open.as_str())
                && let Some(ch) = rest.strip_suffix(close)
            {
                chars.push_str(ch);
            }
        }
    }
    chars
}

#[test]
fn continue_in_browser_renders_osc8_hyperlink() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (widget, _tmp) = runtime.block_on(widget_forced_chatgpt());
    let url = "https://auth.example.com/login?state=abc123";
    *widget.sign_in_state.write().unwrap() =
        SignInState::ChatGptContinueInBrowser(ContinueInBrowserState {
            login_id: "login-1".to_string(),
            auth_url: url.to_string(),
        });

    // Render into a narrow buffer so the URL wraps across multiple rows.
    let area = Rect::new(0, 0, 30, 20);
    let mut buf = Buffer::empty(area);
    widget.render_continue_in_browser(area, &mut buf);

    // Every character of the URL should be present as an OSC 8 cell.
    let found = collect_osc8_chars(&buf, area, url);
    assert_eq!(found, url, "OSC 8 hyperlink should cover the full URL");
}

#[test]
fn mark_url_hyperlink_wraps_cyan_underlined_cells() {
    let url = "https://example.com";
    let area = Rect::new(0, 0, 20, 1);
    let mut buf = Buffer::empty(area);

    // Manually write some cyan+underlined characters to simulate a rendered URL.
    for (i, ch) in "example".chars().enumerate() {
        let cell = &mut buf[(i as u16, 0)];
        cell.set_symbol(&ch.to_string());
        cell.fg = Color::Cyan;
        cell.modifier = Modifier::UNDERLINED;
    }
    // Leave a plain cell that should NOT be marked.
    buf[(7, 0)].set_symbol("X");

    mark_url_hyperlink(&mut buf, area, url);

    // Each cyan+underlined cell should now carry the OSC 8 wrapper.
    let found = collect_osc8_chars(&buf, area, url);
    assert_eq!(found, "example");

    // The plain "X" cell should be untouched.
    assert_eq!(buf[(7, 0)].symbol(), "X");
}

#[test]
fn mark_url_hyperlink_sanitizes_control_chars() {
    let area = Rect::new(0, 0, 10, 1);
    let mut buf = Buffer::empty(area);

    // One cyan+underlined cell to mark.
    let cell = &mut buf[(0, 0)];
    cell.set_symbol("a");
    cell.fg = Color::Cyan;
    cell.modifier = Modifier::UNDERLINED;

    // URL contains ESC and BEL that could break the OSC 8 sequence.
    let malicious_url = "https://evil.com/\x1B]8;;\x07injected";
    mark_url_hyperlink(&mut buf, area, malicious_url);

    let sym = buf[(0, 0)].symbol().to_string();
    // The sanitized URL retains `]` (printable) but strips ESC and BEL.
    let sanitized = "https://evil.com/]8;;injected";
    assert!(
        sym.contains(sanitized),
        "symbol should contain sanitized URL, got: {sym:?}"
    );
    // The injected close-sequence must not survive: \x1B and \x07 are gone.
    assert!(
        !sym.contains("\x1B]8;;\x07injected"),
        "symbol must not contain raw control chars from URL"
    );
}
