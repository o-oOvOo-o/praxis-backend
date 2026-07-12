use super::*;
use keyring::Error as KeyringError;
use praxis_keyring_store::tests::MockKeyringStore;
use tempfile::tempdir;

const FIRST_KEY: &str = "sk-ant-api03-first-secret-value";
const SECOND_KEY: &str = "sk-ant-api03-second-secret-value";

#[test]
fn provider_id_maps_to_a_stable_versioned_credential_id() {
    assert_eq!(
        provider_api_key_credential_id("anthropic"),
        Ok("model-provider/v1/anthropic".to_string())
    );
    assert_eq!(
        provider_api_key_credential_id("  anthropic  "),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    assert_eq!(
        provider_api_key_credential_id(""),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    assert_eq!(
        provider_api_key_credential_id(" \t\r\n"),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    assert_eq!(
        provider_api_key_credential_id("anthropic shared"),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
}

#[test]
fn mock_keyring_round_trip_uses_no_files() -> anyhow::Result<()> {
    let praxis_home = tempdir()?;
    let auth_file = praxis_home.path().join("auth.json");
    std::fs::write(&auth_file, "sentinel-auth-config")?;
    let keyring = MockKeyringStore::default();

    save_provider_api_key_with_store(praxis_home.path(), "anthropic/default", FIRST_KEY, &keyring)?;
    let loaded =
        load_provider_api_key_with_store(praxis_home.path(), "anthropic/default", &keyring)?
            .expect("saved provider key should load");
    assert_eq!(loaded.expose_secret(), FIRST_KEY);
    assert_eq!(std::fs::read_to_string(&auth_file)?, "sentinel-auth-config");

    assert!(delete_provider_api_key_with_store(
        praxis_home.path(),
        "anthropic/default",
        &keyring,
    )?);
    assert!(
        load_provider_api_key_with_store(praxis_home.path(), "anthropic/default", &keyring)?
            .is_none()
    );
    assert_eq!(std::fs::read_to_string(&auth_file)?, "sentinel-auth-config");
    Ok(())
}

#[test]
fn credentials_are_isolated_by_home_and_credential_id() -> anyhow::Result<()> {
    let first_home = tempdir()?;
    let second_home = tempdir()?;
    let keyring = MockKeyringStore::default();

    save_provider_api_key_with_store(first_home.path(), "anthropic/default", FIRST_KEY, &keyring)?;
    save_provider_api_key_with_store(first_home.path(), "openai/default", SECOND_KEY, &keyring)?;
    save_provider_api_key_with_store(
        second_home.path(),
        "anthropic/default",
        SECOND_KEY,
        &keyring,
    )?;

    let first_anthropic =
        load_provider_api_key_with_store(first_home.path(), "anthropic/default", &keyring)?
            .expect("first Anthropic credential should exist");
    let first_openai =
        load_provider_api_key_with_store(first_home.path(), "openai/default", &keyring)?
            .expect("first OpenAI credential should exist");
    let second_anthropic =
        load_provider_api_key_with_store(second_home.path(), "anthropic/default", &keyring)?
            .expect("second Anthropic credential should exist");

    assert_eq!(first_anthropic.expose_secret(), FIRST_KEY);
    assert_eq!(first_openai.expose_secret(), SECOND_KEY);
    assert_eq!(second_anthropic.expose_secret(), SECOND_KEY);
    assert_ne!(
        keyring_account(first_home.path(), "anthropic/default")?,
        keyring_account(first_home.path(), "openai/default")?
    );
    assert_ne!(
        keyring_account(first_home.path(), "anthropic/default")?,
        keyring_account(second_home.path(), "anthropic/default")?
    );
    Ok(())
}

#[test]
fn validation_rejects_ambiguous_or_unsafe_inputs() -> anyhow::Result<()> {
    let praxis_home = tempdir()?;
    let keyring = MockKeyringStore::default();

    assert_eq!(
        save_provider_api_key_with_store(praxis_home.path(), "", FIRST_KEY, &keyring),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    assert_eq!(
        save_provider_api_key_with_store(
            praxis_home.path(),
            "anthropic default",
            FIRST_KEY,
            &keyring,
        ),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    assert_eq!(
        save_provider_api_key_with_store(praxis_home.path(), "anthropic/default", "  ", &keyring),
        Err(ProviderApiKeyError::InvalidApiKey)
    );
    assert_eq!(
        save_provider_api_key_with_store(
            praxis_home.path(),
            "anthropic/default",
            "secret\nheader-injection",
            &keyring,
        ),
        Err(ProviderApiKeyError::InvalidApiKey)
    );
    let oversized_credential_id = "a".repeat(MAX_CREDENTIAL_ID_BYTES + 1);
    assert_eq!(
        save_provider_api_key_with_store(
            praxis_home.path(),
            &oversized_credential_id,
            FIRST_KEY,
            &keyring,
        ),
        Err(ProviderApiKeyError::InvalidCredentialId)
    );
    let oversized_api_key = "a".repeat(MAX_API_KEY_BYTES + 1);
    assert_eq!(
        save_provider_api_key_with_store(
            praxis_home.path(),
            "anthropic/default",
            &oversized_api_key,
            &keyring,
        ),
        Err(ProviderApiKeyError::InvalidApiKey)
    );
    let error =
        load_provider_api_key_with_store(std::path::Path::new(""), "anthropic/default", &keyring)
            .expect_err("empty praxis_home should be rejected");
    assert_eq!(error, ProviderApiKeyError::InvalidPraxisHome);
    Ok(())
}

#[test]
fn secret_is_redacted_from_debug_and_keyring_errors() -> anyhow::Result<()> {
    let wrapped = ProviderApiKey::new(FIRST_KEY)?;
    assert_eq!(format!("{wrapped:?}"), "ProviderApiKey([REDACTED])");
    assert!(!format!("{wrapped:?}").contains(FIRST_KEY));
    let invalid_error = ProviderApiKey::new(format!("{FIRST_KEY}\n"))
        .expect_err("control characters should be rejected");
    assert!(!format!("{invalid_error:?}").contains(FIRST_KEY));
    assert!(!invalid_error.to_string().contains(FIRST_KEY));

    let praxis_home = tempdir()?;
    let keyring = MockKeyringStore::default();
    let account = keyring_account(praxis_home.path(), "anthropic/default")?;
    keyring.set_error(
        &account,
        KeyringError::Invalid("mock failure".into(), FIRST_KEY.into()),
    );
    let error = save_provider_api_key_with_store(
        praxis_home.path(),
        "anthropic/default",
        FIRST_KEY,
        &keyring,
    )
    .expect_err("mock keyring save should fail");

    assert_eq!(error, ProviderApiKeyError::SaveFailed);
    assert!(!format!("{error:?}").contains(FIRST_KEY));
    assert!(!error.to_string().contains(FIRST_KEY));
    Ok(())
}

#[test]
fn load_delete_and_invalid_stored_value_errors_are_redacted() -> anyhow::Result<()> {
    let praxis_home = tempdir()?;
    let account = keyring_account(praxis_home.path(), "anthropic/default")?;

    let load_keyring = MockKeyringStore::default();
    load_keyring.set_error(
        &account,
        KeyringError::Invalid("mock load failure".into(), FIRST_KEY.into()),
    );
    let load_error =
        load_provider_api_key_with_store(praxis_home.path(), "anthropic/default", &load_keyring)
            .expect_err("mock keyring load should fail");
    assert_eq!(load_error, ProviderApiKeyError::LoadFailed);
    assert!(!format!("{load_error:?}").contains(FIRST_KEY));

    let delete_keyring = MockKeyringStore::default();
    delete_keyring.set_error(
        &account,
        KeyringError::Invalid("mock delete failure".into(), FIRST_KEY.into()),
    );
    let delete_error = delete_provider_api_key_with_store(
        praxis_home.path(),
        "anthropic/default",
        &delete_keyring,
    )
    .expect_err("mock keyring delete should fail");
    assert_eq!(delete_error, ProviderApiKeyError::DeleteFailed);
    assert!(!format!("{delete_error:?}").contains(FIRST_KEY));

    let invalid_keyring = MockKeyringStore::default();
    let invalid_stored_key = format!("{FIRST_KEY} invalid");
    praxis_keyring_store::KeyringStore::save(
        &invalid_keyring,
        KEYRING_SERVICE,
        &account,
        &invalid_stored_key,
    )?;
    let invalid_error =
        load_provider_api_key_with_store(praxis_home.path(), "anthropic/default", &invalid_keyring)
            .expect_err("invalid stored key should fail validation");
    assert_eq!(invalid_error, ProviderApiKeyError::InvalidStoredApiKey);
    assert!(!format!("{invalid_error:?}").contains(FIRST_KEY));
    assert!(!invalid_error.to_string().contains(FIRST_KEY));
    Ok(())
}

#[test]
fn lexically_equivalent_home_paths_share_one_credential() -> anyhow::Result<()> {
    let praxis_home = tempdir()?;
    let keyring = MockKeyringStore::default();
    let equivalent_home = praxis_home.path().join("nested").join("..");

    save_provider_api_key_with_store(praxis_home.path(), "anthropic/default", FIRST_KEY, &keyring)?;
    let loaded = load_provider_api_key_with_store(&equivalent_home, "anthropic/default", &keyring)?
        .expect("equivalent home path should resolve the same credential");

    assert_eq!(loaded.expose_secret(), FIRST_KEY);
    Ok(())
}
