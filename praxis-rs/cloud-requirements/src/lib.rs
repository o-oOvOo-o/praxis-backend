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

pub use loader::cloud_config_bundle_loader;
pub use loader::cloud_config_bundle_loader_for_config_toml;
pub use loader::cloud_config_bundle_loader_for_storage;
pub use loader::cloud_config_bundle_loader_from_provider;
pub use loader::cloud_requirements_loader;
pub use loader::cloud_requirements_loader_for_storage;
pub use provider::ConfigBundleProvider;
pub use provider::LocalFileConfigBundleProvider;
pub use provider::NoopConfigBundleProvider;
pub use provider::OpenAiHostedConfigBundleProvider;

#[cfg(test)]
mod tests;
