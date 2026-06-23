mod support;
use support::*;

#[tokio::test]
async fn fetch_cloud_requirements_skips_non_chatgpt_auth() {
    let auth_manager = auth_manager_with_api_key();
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager,
        Arc::new(StaticFetcher { contents: None }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let result = service.fetch().await;
    assert_eq!(result, Ok(None));
}

#[tokio::test]
async fn fetch_cloud_requirements_skips_non_business_or_enterprise_plan() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("pro"),
        Arc::new(StaticFetcher { contents: None }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let result = service.fetch().await;
    assert_eq!(result, Ok(None));
}

#[tokio::test]
async fn fetch_cloud_requirements_skips_team_like_usage_based_plan() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("self_serve_business_usage_based"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    assert_eq!(service.fetch().await, Ok(None));
}

#[tokio::test]
async fn fetch_cloud_requirements_allows_business_plan() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
}

#[tokio::test]
async fn fetch_cloud_requirements_allows_business_like_usage_based_plan() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("enterprise_cbp_usage_based"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
}

#[tokio::test]
async fn fetch_cloud_requirements_allows_hc_plan_as_enterprise() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("hc"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
}

#[tokio::test]
async fn fetch_cloud_requirements_handles_missing_contents() {
    let result = parse_for_fetch(/*contents*/ None);
    assert!(result.is_none());
}

#[tokio::test]
async fn fetch_cloud_requirements_handles_empty_contents() {
    let result = parse_for_fetch(Some("   "));
    assert!(result.is_none());
}

#[tokio::test]
async fn fetch_cloud_requirements_handles_invalid_toml() {
    let result = parse_for_fetch(Some("not = ["));
    assert!(result.is_none());
}

#[tokio::test]
async fn fetch_cloud_requirements_ignores_empty_requirements() {
    let result = parse_for_fetch(Some("# comment"));
    assert!(result.is_none());
}

#[tokio::test]
async fn fetch_cloud_requirements_parses_valid_toml() {
    let result = parse_for_fetch(Some("allowed_approval_policies = [\"never\"]"));

    assert_eq!(
        result,
        Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        })
    );
}

#[tokio::test]
async fn fetch_cloud_requirements_parses_apps_requirements_toml() {
    let result = parse_for_fetch(Some(
        r#"
[apps.connector_5f3c8c41a1e54ad7a76272c89e2554fa]
enabled = false
"#,
    ));

    assert_eq!(
        result,
        Some(ConfigRequirementsToml {
            apps: Some(praxis_core::config_loader::AppsRequirementsToml {
                apps: BTreeMap::from([(
                    "connector_5f3c8c41a1e54ad7a76272c89e2554fa".to_string(),
                    praxis_core::config_loader::AppRequirementToml {
                        enabled: Some(false),
                    },
                )]),
            }),
            ..Default::default()
        })
    );
}

#[tokio::test(start_paused = true)]
async fn fetch_cloud_requirements_times_out() {
    let auth_manager = auth_manager_with_plan("enterprise");
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager,
        Arc::new(PendingFetcher),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let handle = tokio::spawn(async move { service.fetch_with_timeout().await });
    tokio::time::advance(CLOUD_REQUIREMENTS_TIMEOUT + Duration::from_millis(1)).await;

    let result = handle.await.expect("cloud requirements task");
    let err = result.expect_err("cloud requirements timeout should fail closed");
    assert!(
        err.to_string()
            .contains("timed out waiting for cloud requirements")
    );
}

#[tokio::test(start_paused = true)]
async fn fetch_cloud_requirements_retries_until_success() {
    let fetcher = Arc::new(SequenceFetcher::new(vec![
        Err(request_error()),
        Ok(Some("allowed_approval_policies = [\"never\"]".to_string())),
    ]));
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    let handle = tokio::spawn(async move { service.fetch().await });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1)).await;

    assert_eq!(
        handle.await.expect("cloud requirements task"),
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fetch_cloud_requirements_recovers_after_unauthorized_reload() {
    let auth_home = tempdir().expect("tempdir");
    write_auth_json(
        auth_home.path(),
        chatgpt_auth_json_with_last_refresh(
            "business",
            Some("user-12345"),
            Some("account-12345"),
            "stale-access-token",
            "test-refresh-token",
            // Keep auth "fresh" so the first request hits unauthorized recovery
            // instead of AuthManager::auth() proactively reloading from disk.
            "3025-01-01T00:00:00Z",
        ),
    )
    .expect("write initial auth");
    let auth_manager = Arc::new(AuthManager::new(
        auth_home.path().to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        AuthCredentialsStoreMode::File,
    ));

    write_auth_json(
        auth_home.path(),
        chatgpt_auth_json_with_last_refresh(
            "business",
            Some("user-12345"),
            Some("account-12345"),
            "fresh-access-token",
            "test-refresh-token",
            "3025-01-01T00:00:00Z",
        ),
    )
    .expect("write refreshed auth");
    let auth = ManagedAuthContext {
        _home: auth_home,
        manager: auth_manager,
    };

    let fetcher = Arc::new(TokenFetcher {
        expected_token: "fresh-access-token".to_string(),
        contents: "allowed_approval_policies = [\"never\"]".to_string(),
        request_count: AtomicUsize::new(0),
    });
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        Arc::clone(&auth.manager),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fetch_cloud_requirements_recovers_after_unauthorized_reload_updates_cache_identity() {
    let auth_home = tempdir().expect("tempdir");
    write_auth_json(
        auth_home.path(),
        chatgpt_auth_json_with_last_refresh(
            "business",
            Some("user-12345"),
            Some("account-12345"),
            "stale-access-token",
            "test-refresh-token",
            "3025-01-01T00:00:00Z",
        ),
    )
    .expect("write initial auth");
    let auth_manager = Arc::new(AuthManager::new(
        auth_home.path().to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        AuthCredentialsStoreMode::File,
    ));

    write_auth_json(
        auth_home.path(),
        chatgpt_auth_json_with_last_refresh(
            "business",
            Some("user-99999"),
            Some("account-12345"),
            "fresh-access-token",
            "test-refresh-token",
            "3025-01-01T00:00:00Z",
        ),
    )
    .expect("write refreshed auth");
    let auth = ManagedAuthContext {
        _home: auth_home,
        manager: auth_manager,
    };

    let fetcher = Arc::new(TokenFetcher {
        expected_token: "fresh-access-token".to_string(),
        contents: "allowed_approval_policies = [\"never\"]".to_string(),
        request_count: AtomicUsize::new(0),
    });
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        Arc::clone(&auth.manager),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );

    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let cache_file: CloudRequirementsCacheFile =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read cache"))
            .expect("parse cache");
    assert_eq!(
        cache_file.signed_payload.chatgpt_user_id,
        Some("user-99999".to_string())
    );
    assert_eq!(
        cache_file.signed_payload.account_id,
        Some("account-12345".to_string())
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fetch_cloud_requirements_surfaces_auth_recovery_message() {
    let auth = managed_auth_context(
        "enterprise",
        Some("user-12345"),
        Some("account-12345"),
        "stale-access-token",
        "test-refresh-token",
    );
    write_auth_json(
        auth._home.path(),
        chatgpt_auth_json(
            "enterprise",
            Some("user-12345"),
            Some("account-99999"),
            "fresh-access-token",
            "test-refresh-token",
        ),
    )
    .expect("write mismatched auth");

    let fetcher = Arc::new(UnauthorizedFetcher {
        message: "GET /config/requirements failed: 401".to_string(),
        request_count: AtomicUsize::new(0),
    });
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        Arc::clone(&auth.manager),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    let err = service
        .fetch()
        .await
        .expect_err("cloud requirements should surface auth recovery errors");
    assert_eq!(
        err.to_string(),
        "Your access token could not be refreshed because you have since logged out or signed in to another account. Please sign in again."
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_unauthorized_without_recovery_uses_generic_message() {
    let auth_home = tempdir().expect("tempdir");
    write_auth_json(
        auth_home.path(),
        chatgpt_auth_json_with_mode(
            "enterprise",
            Some("user-12345"),
            Some("account-12345"),
            "test-access-token",
            "test-refresh-token",
            "2025-01-01T00:00:00Z",
            Some("chatgptAuthTokens"),
        ),
    )
    .expect("write auth");
    let auth_manager = Arc::new(AuthManager::new(
        auth_home.path().to_path_buf(),
        /*enable_praxis_api_key_env*/ false,
        AuthCredentialsStoreMode::File,
    ));

    let fetcher = Arc::new(UnauthorizedFetcher {
        message:
            "GET https://chatgpt.com/backend-api/wham/config/requirements failed: 401; content-type=text/html; body=<html>nope</html>"
                .to_string(),
        request_count: AtomicUsize::new(0),
    });
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager,
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    let err = service
        .fetch()
        .await
        .expect_err("cloud requirements should fail closed");
    assert_eq!(
        err.to_string(),
        CLOUD_REQUIREMENTS_AUTH_RECOVERY_FAILED_MESSAGE
    );
    assert_eq!(err.code(), CloudRequirementsLoadErrorCode::Auth);
    assert_eq!(err.status_code(), Some(401));
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_parse_error_does_not_retry() {
    let fetcher = Arc::new(SequenceFetcher::new(vec![
        Ok(Some("not = [".to_string())),
        Ok(Some("allowed_approval_policies = [\"never\"]".to_string())),
    ]));
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert!(service.fetch().await.is_err());
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_uses_cache_when_valid() {
    let praxis_home = tempdir().expect("tempdir");
    let prime_service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let _ = prime_service.fetch().await;

    let fetcher = Arc::new(SequenceFetcher::new(vec![Err(request_error())]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn fetch_cloud_requirements_writes_cache_when_identity_is_incomplete() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan_and_identity(
            "business",
            /*chatgpt_user_id*/ None,
            Some("account-12345"),
        ),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );

    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let cache_file: CloudRequirementsCacheFile =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read cache"))
            .expect("parse cache");
    assert_eq!(cache_file.signed_payload.chatgpt_user_id, None);
    assert_eq!(
        cache_file.signed_payload.account_id,
        Some("account-12345".to_string())
    );
}

#[tokio::test]
async fn fetch_cloud_requirements_does_not_use_cache_when_auth_identity_is_incomplete() {
    let praxis_home = tempdir().expect("tempdir");
    let prime_service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let _ = prime_service.fetch().await;

    let fetcher = Arc::new(SequenceFetcher::new(vec![Ok(Some(
        "allowed_approval_policies = [\"on-request\"]".to_string(),
    ))]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan_and_identity(
            "business",
            /*chatgpt_user_id*/ None,
            Some("account-12345"),
        ),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_ignores_cache_for_different_auth_identity() {
    let praxis_home = tempdir().expect("tempdir");
    let prime_service = CloudRequirementsService::new(
        auth_manager_with_plan_and_identity("business", Some("user-12345"), Some("account-12345")),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let _ = prime_service.fetch().await;

    let fetcher = Arc::new(SequenceFetcher::new(vec![Ok(Some(
        "allowed_approval_policies = [\"on-request\"]".to_string(),
    ))]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan_and_identity("business", Some("user-99999"), Some("account-12345")),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_ignores_tampered_cache() {
    let praxis_home = tempdir().expect("tempdir");
    let prime_service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );
    let _ = prime_service.fetch().await;

    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let mut cache_file: CloudRequirementsCacheFile =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read cache"))
            .expect("parse cache");
    cache_file.signed_payload.contents =
        Some("allowed_approval_policies = [\"on-request\"]".to_string());
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&cache_file).expect("serialize cache"),
    )
    .expect("write cache");

    let fetcher = Arc::new(SequenceFetcher::new(vec![Ok(Some(
        "allowed_approval_policies = [\"never\"]".to_string(),
    ))]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("enterprise"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_ignores_expired_cache() {
    let praxis_home = tempdir().expect("tempdir");
    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let cache_file = CloudRequirementsCacheFile {
        signed_payload: CloudRequirementsCacheSignedPayload {
            cached_at: Utc::now(),
            expires_at: Utc::now() - ChronoDuration::seconds(1),
            chatgpt_user_id: Some("user-12345".to_string()),
            account_id: Some("account-12345".to_string()),
            contents: Some("allowed_approval_policies = [\"on-request\"]".to_string()),
        },
        signature: String::new(),
    };
    let payload_bytes = cache_payload_bytes(&cache_file.signed_payload).expect("payload");
    let signature = sign_cache_payload(&payload_bytes).expect("sign payload");
    let cache_file = CloudRequirementsCacheFile {
        signature,
        ..cache_file
    };
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&cache_file).expect("serialize cache"),
    )
    .expect("write cache");

    let fetcher = Arc::new(SequenceFetcher::new(vec![Ok(Some(
        "allowed_approval_policies = [\"never\"]".to_string(),
    ))]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("enterprise"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_cloud_requirements_writes_signed_cache() {
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        Arc::new(StaticFetcher {
            contents: Some("allowed_approval_policies = [\"never\"]".to_string()),
        }),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    let _ = service.fetch().await;

    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let cache_file: CloudRequirementsCacheFile =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read cache"))
            .expect("parse cache");
    assert!(
        cache_file.signed_payload.expires_at
            <= cache_file.signed_payload.cached_at + ChronoDuration::minutes(30)
    );
    assert!(cache_file.signed_payload.expires_at > cache_file.signed_payload.cached_at);
    assert!(cache_file.signed_payload.cached_at <= Utc::now());
    assert_eq!(
        cache_file.signed_payload.chatgpt_user_id,
        Some("user-12345".to_string())
    );
    assert_eq!(
        cache_file.signed_payload.account_id,
        Some("account-12345".to_string())
    );
    assert_eq!(
        cache_file
            .signed_payload
            .contents
            .as_deref()
            .and_then(|contents| parse_cloud_requirements(contents).ok().flatten()),
        Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        })
    );
    let payload_bytes = cache_payload_bytes(&cache_file.signed_payload).expect("payload bytes");
    assert!(verify_cache_signature(
        &payload_bytes,
        &cache_file.signature
    ));
}

#[tokio::test]
async fn fetch_cloud_requirements_none_is_success_without_retry() {
    let fetcher = Arc::new(SequenceFetcher::new(vec![Ok(None), Err(request_error())]));
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("enterprise"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(service.fetch().await, Ok(None));
    assert_eq!(fetcher.request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn fetch_cloud_requirements_stops_after_max_retries() {
    let fetcher = Arc::new(SequenceFetcher::new(vec![
        Err(request_error());
        CLOUD_REQUIREMENTS_MAX_ATTEMPTS
    ]));
    let praxis_home = tempdir().expect("tempdir");
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("enterprise"),
        fetcher.clone(),
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    let handle = tokio::spawn(async move { service.fetch().await });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;

    let err = handle
        .await
        .expect("cloud requirements task")
        .expect_err("cloud requirements retry exhaustion should fail closed");
    assert_eq!(
        err.to_string(),
        "failed to load your workspace-managed config"
    );
    assert_eq!(err.code(), CloudRequirementsLoadErrorCode::RequestFailed);
    assert_eq!(
        fetcher.request_count.load(Ordering::SeqCst),
        CLOUD_REQUIREMENTS_MAX_ATTEMPTS
    );
}

#[tokio::test]
async fn refresh_from_remote_updates_cached_cloud_requirements() {
    let praxis_home = tempdir().expect("tempdir");
    let fetcher = Arc::new(SequenceFetcher::new(vec![
        Ok(Some("allowed_approval_policies = [\"never\"]".to_string())),
        Ok(Some(
            "allowed_approval_policies = [\"on-request\"]".to_string(),
        )),
    ]));
    let service = CloudRequirementsService::new(
        auth_manager_with_plan("business"),
        fetcher,
        praxis_home.path().to_path_buf(),
        CLOUD_REQUIREMENTS_TIMEOUT,
    );

    assert_eq!(
        service.fetch().await,
        Ok(Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::Never]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        }))
    );

    assert!(service.refresh_cache().await);

    let path = praxis_home.path().join(CLOUD_REQUIREMENTS_CACHE_FILENAME);
    let cache_file: CloudRequirementsCacheFile =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read cache"))
            .expect("parse cache");
    assert_eq!(
        cache_file
            .signed_payload
            .contents
            .as_deref()
            .and_then(|contents| parse_cloud_requirements(contents).ok().flatten()),
        Some(ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        })
    );
}
