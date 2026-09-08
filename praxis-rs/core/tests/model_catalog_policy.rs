use praxis_core::ANTHROPIC_PROVIDER_ID;
use praxis_core::KIMI_PROVIDER_ID;
use praxis_core::OPENAI_PROVIDER_ID;
use praxis_core::config::ConfigBuilder;
use praxis_core::models_manager::catalog::*;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::default_input_modalities;

fn test_preset(model: &str) -> ModelPreset {
    ModelPreset {
        id: model.to_string(),
        model: model.to_string(),
        display_name: model.to_string(),
        description: String::new(),
        default_reasoning_effort: ReasoningEffort::Medium,
        supported_reasoning_efforts: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            display_name: None,
            description: "medium".to_string(),
        }],
        supports_personality: false,
        is_default: false,
        upgrade: None,
        show_in_picker: true,
        availability_nux: None,
        supported_in_api: true,
        input_modalities: default_input_modalities(),
    }
}

#[tokio::test]
async fn model_catalog_backfills_gpt55_when_current_provider_is_openai() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    assert_eq!(config.model_provider_id, OPENAI_PROVIDER_ID);

    let catalog = build_model_catalog(&config, Vec::new());
    let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
    let gpt55 = catalog
        .models
        .iter()
        .find(|preset| preset.id == preset_id)
        .expect("GPT-5.5 should be locally backfilled for the OpenAI picker catalog");

    assert_eq!(gpt55.model, "gpt-5.5");
    assert!(gpt55.show_in_picker);
    assert!(
        gpt55
            .supported_reasoning_efforts
            .iter()
            .any(|preset| preset.effort == ReasoningEffort::XHigh)
    );
    let metadata = catalog
        .metadata_by_preset_id
        .get(&preset_id)
        .expect("GPT-5.5 should carry provider selection metadata");
    assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
}

#[tokio::test]
async fn model_catalog_backfills_anthropic_models_when_current_provider_is_openai() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    assert_eq!(config.model_provider_id, OPENAI_PROVIDER_ID);
    assert!(config.model_providers.contains_key(ANTHROPIC_PROVIDER_ID));

    let catalog = build_model_catalog(&config, Vec::new());
    for model in [
        "claude-sonnet-5",
        "claude-opus-4-8",
        "claude-fable-5",
        "claude-haiku-4-5",
    ] {
        let preset_id = provider_scoped_preset_id(ANTHROPIC_PROVIDER_ID, model);
        let preset = catalog
            .models
            .iter()
            .find(|preset| preset.id == preset_id)
            .unwrap_or_else(|| panic!("{model} should be present in the TUI model picker"));
        assert!(preset.show_in_picker);
        let metadata = catalog
            .metadata_by_preset_id
            .get(&preset_id)
            .unwrap_or_else(|| panic!("{model} should carry Anthropic provider metadata"));
        assert_eq!(metadata.provider_id, ANTHROPIC_PROVIDER_ID);
        assert!(metadata.provider.is_anthropic());
    }
}

#[tokio::test]
async fn model_catalog_backfills_kimi_models_when_current_provider_is_openai() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    assert!(config.model_providers.contains_key(KIMI_PROVIDER_ID));

    let catalog = build_model_catalog(&config, Vec::new());
    for model in [
        "k3[1m]",
        "k3",
        "kimi-for-coding",
        "kimi-for-coding-highspeed",
    ] {
        let preset_id = provider_scoped_preset_id(KIMI_PROVIDER_ID, model);
        let preset = catalog
            .models
            .iter()
            .find(|preset| preset.id == preset_id)
            .unwrap_or_else(|| panic!("{model} should be present in the TUI model picker"));
        assert!(preset.show_in_picker);
        let metadata = catalog
            .metadata_by_preset_id
            .get(&preset_id)
            .unwrap_or_else(|| panic!("{model} should carry Kimi provider metadata"));
        assert_eq!(metadata.provider_id, KIMI_PROVIDER_ID);
        assert!(metadata.provider.is_kimi());
    }
}

#[tokio::test]
async fn model_catalog_includes_gpt55_when_current_provider_is_not_openai() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let mut config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    assert!(
        config.model_providers.contains_key(OPENAI_PROVIDER_ID),
        "built-in OpenAI provider should be available for cross-provider model picking"
    );
    config.model_provider_id = "deepseek".to_string();
    config.model_provider = config.model_providers["deepseek"].clone();

    let catalog = build_model_catalog(&config, Vec::new());
    let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
    let gpt55 = catalog
        .models
        .iter()
        .find(|preset| preset.id == preset_id)
        .expect("GPT-5.5 should be present in the TUI model picker catalog");

    assert_eq!(gpt55.model, "gpt-5.5");
    assert!(gpt55.show_in_picker);
    assert!(
        gpt55
            .supported_reasoning_efforts
            .iter()
            .any(|preset| preset.effort == ReasoningEffort::XHigh)
    );
    let metadata = catalog
        .metadata_by_preset_id
        .get(&preset_id)
        .expect("GPT-5.5 should carry provider selection metadata");
    assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
}

#[tokio::test]
async fn provider_scoped_server_model_preserves_provider_metadata() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let mut config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    config.model_provider_id = "deepseek".to_string();
    config.model_provider = config.model_providers["deepseek"].clone();

    let mut server_model = test_preset("gpt-5.5");
    server_model.id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
    let catalog = build_model_catalog(&config, vec![server_model]);
    let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
    let metadata = catalog
        .metadata_by_preset_id
        .get(&preset_id)
        .expect("server-scoped GPT-5.5 should keep OpenAI metadata");

    assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
}

#[tokio::test]
async fn model_catalog_prioritizes_gpt55_before_remote_server_models() {
    let praxis_home = tempfile::tempdir().expect("temp praxis home");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home.path().to_path_buf())
        .build()
        .await
        .expect("config");

    let catalog = build_model_catalog(
        &config,
        vec![
            test_preset("remote-model-a"),
            test_preset("remote-model-b"),
            test_preset("remote-model-c"),
        ],
    );
    let gpt55_index = catalog
        .models
        .iter()
        .position(|preset| preset.model == "gpt-5.5")
        .expect("GPT-5.5 should be locally backfilled");
    let remote_index = catalog
        .models
        .iter()
        .position(|preset| preset.model == "remote-model-a")
        .expect("remote server model should still be present");

    assert!(
        gpt55_index < remote_index,
        "GPT-5.5 should stay near the top of the picker even when remote models arrive first"
    );
}

#[tokio::test]
async fn projection_does_not_backfill_models_or_read_other_home_configs() {
    let home = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .build()
        .await
        .unwrap();
    let catalog = project_model_catalog(&config, Vec::new());
    assert!(catalog.models.is_empty());
    assert!(catalog.metadata_by_preset_id.is_empty());
}

#[test]
fn provider_ids_are_preserved_without_lossy_sanitization() {
    assert_eq!(
        provider_scoped_preset_id("my-provider", "model"),
        "my-provider::model"
    );
    assert_ne!(
        provider_scoped_preset_id("my-provider", "model"),
        provider_scoped_preset_id("my_provider", "model")
    );
}

#[tokio::test]
async fn active_configuration_profiles_are_preserved_and_deduplicated() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        r#"
[model_providers.example-provider]
name = "Example"
base_url = "http://127.0.0.1:9"
wire_api = "responses"
[profiles.first]
model = "example-model"
model_provider = "example-provider"
[profiles.second]
model = "example-model"
model_provider = "example-provider"
"#,
    )
    .unwrap();
    let config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .build()
        .await
        .unwrap();
    let catalog = build_model_catalog(&config, Vec::new());
    let matches = catalog
        .models
        .iter()
        .filter(|model| model.id == "example-provider::example-model")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].default_reasoning_effort, ReasoningEffort::Medium);
    let projected = project_model_catalog(&config, catalog.models.clone());
    assert_eq!(projected.models, catalog.models);
    assert_eq!(
        projected.metadata_by_preset_id["example-provider::example-model"].provider_id,
        "example-provider"
    );
}

#[tokio::test]
async fn explicit_custom_catalog_is_authoritative_for_its_provider() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "[profiles.outside]\nmodel = 'profile-outside-catalog'\nmodel_provider = 'openai'\n",
    )
    .unwrap();
    let mut config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .build()
        .await
        .unwrap();
    config.model = Some("outside-custom-catalog".to_owned());
    config.model_catalog = Some(praxis_protocol::openai_models::ModelsResponse::default());
    let catalog = build_model_catalog(&config, vec![test_preset("custom-model")]);
    let current = catalog
        .models
        .iter()
        .filter(|model| {
            catalog.metadata_by_preset_id[&model.id].provider_id == config.model_provider_id
        })
        .map(|model| model.model.as_str())
        .collect::<Vec<_>>();
    assert_eq!(current, vec!["custom-model"]);
    assert!(
        catalog
            .models
            .iter()
            .all(|model| model.default_reasoning_effort == ReasoningEffort::Medium)
    );
}

#[tokio::test]
async fn same_model_under_two_providers_keeps_both_identities() {
    let home = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .praxis_home(home.path().to_path_buf())
        .build()
        .await
        .unwrap();
    let mut first = test_preset("shared-model");
    first.id = provider_scoped_preset_id("openai", "shared-model");
    let mut second = first.clone();
    second.id = provider_scoped_preset_id("ollama", "shared-model");
    let catalog = build_model_catalog(&config, vec![first, second]);
    let matches = catalog
        .models
        .iter()
        .filter(|preset| preset.model == "shared-model")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 2);
    assert_ne!(matches[0].id, matches[1].id);
    let projected = project_model_catalog(&config, catalog.models);
    assert_eq!(
        projected.metadata_by_preset_id["openai::shared-model"].provider_id,
        "openai"
    );
    assert_eq!(
        projected.metadata_by_preset_id["ollama::shared-model"].provider_id,
        "ollama"
    );
}
