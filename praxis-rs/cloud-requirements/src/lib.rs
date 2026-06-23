//! Cloud-hosted config requirements for Praxis.
//!
//! This crate fetches cloud-managed config bundles from hosted or local sources and adapts the
//! legacy `requirements.toml` payload into the Praxis config loader. Hosted OpenAI paths are
//! compatibility providers, not the identity of the crate.

mod cache;
mod constants;
mod fetcher;
mod loader;
mod metrics;
mod parsing;
mod provider;
mod service;

pub use loader::{
    cloud_config_bundle_loader, cloud_config_bundle_loader_for_storage,
    cloud_config_bundle_loader_from_provider, cloud_requirements_loader,
    cloud_requirements_loader_for_storage,
};
pub use provider::{
    ConfigBundleProvider, LocalFileConfigBundleProvider, NoopConfigBundleProvider,
    OpenAiHostedConfigBundleProvider,
};

#[cfg(test)]
mod tests;
