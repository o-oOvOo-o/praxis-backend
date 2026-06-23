use clap::Args;
use clap::Parser;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::config::find_praxis_home;
use praxis_features::FEATURES;
use praxis_features::Stage;
use praxis_features::is_known_feature_key;
use praxis_tui::Cli as TuiCli;
#[derive(Debug, Default, Args, Clone)]
pub(crate) struct FeatureToggles {
    /// Enable a feature (repeatable). Equivalent to `-c features.<name>=true`.
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    pub(crate) enable: Vec<String>,

    /// Disable a feature (repeatable). Equivalent to `-c features.<name>=false`.
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    pub(crate) disable: Vec<String>,
}

impl FeatureToggles {
    pub(crate) fn to_overrides(&self) -> anyhow::Result<Vec<String>> {
        let mut v = Vec::new();
        for feature in &self.enable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=true"));
        }
        for feature in &self.disable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=false"));
        }
        Ok(v)
    }

    pub(crate) fn validate_feature(feature: &str) -> anyhow::Result<()> {
        if is_known_feature_key(feature) {
            Ok(())
        } else {
            anyhow::bail!("Unknown feature flag: {feature}")
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct FeaturesCli {
    #[command(subcommand)]
    pub(crate) sub: FeaturesSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) enum FeaturesSubcommand {
    /// List known features with their stage and effective state.
    List,
    /// Enable a feature in config.toml.
    Enable(FeatureSetArgs),
    /// Disable a feature in config.toml.
    Disable(FeatureSetArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct FeatureSetArgs {
    /// Feature key to update (for example: unified_exec).
    pub(crate) feature: String,
}

pub(crate) async fn enable_feature_in_config(
    interactive: &TuiCli,
    feature: &str,
) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let praxis_home = find_praxis_home()?;
    ConfigEditsBuilder::new(&praxis_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ true)
        .apply()
        .await?;
    println!("Enabled feature `{feature}` in config.toml.");
    maybe_print_under_development_feature_warning(&praxis_home, interactive, feature);
    Ok(())
}

pub(crate) async fn disable_feature_in_config(
    interactive: &TuiCli,
    feature: &str,
) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let praxis_home = find_praxis_home()?;
    ConfigEditsBuilder::new(&praxis_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ false)
        .apply()
        .await?;
    println!("Disabled feature `{feature}` in config.toml.");
    Ok(())
}

pub(crate) fn maybe_print_under_development_feature_warning(
    praxis_home: &std::path::Path,
    interactive: &TuiCli,
    feature: &str,
) {
    if interactive.config_profile.is_some() {
        return;
    }

    let Some(spec) = FEATURES.iter().find(|spec| spec.key == feature) else {
        return;
    };
    if !matches!(spec.stage, Stage::UnderDevelopment) {
        return;
    }

    let config_path = praxis_home.join(praxis_config::CONFIG_TOML_FILE);
    eprintln!(
        "Under-development features enabled: {feature}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {}.",
        config_path.display()
    );
}

pub(crate) fn stage_str(stage: Stage) -> &'static str {
    match stage {
        Stage::UnderDevelopment => "under development",
        Stage::Experimental { .. } => "experimental",
        Stage::Stable => "stable",
        Stage::Deprecated => "deprecated",
        Stage::Removed => "removed",
    }
}
