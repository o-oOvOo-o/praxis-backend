use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMarketplaceRef {
    pub name: String,
    pub display_name: Option<String>,
    pub provider: PluginMarketplaceProviderSource,
    pub enabled: bool,
    pub sync_on_startup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginMarketplaceProviderSource {
    Local {
        path: PathBuf,
    },
    Git {
        repo: String,
        reference: Option<String>,
        path: Option<PathBuf>,
    },
    Http {
        url: String,
    },
}

impl PluginMarketplaceProviderSource {
    pub fn kind(&self) -> PluginMarketplaceProviderKind {
        match self {
            Self::Local { .. } => PluginMarketplaceProviderKind::Local,
            Self::Git { .. } => PluginMarketplaceProviderKind::Git,
            Self::Http { .. } => PluginMarketplaceProviderKind::Http,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginMarketplaceProviderKind {
    Local,
    Git,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMarketplaceSyncRequest {
    pub marketplace: PluginMarketplaceRef,
    pub force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginMarketplaceSyncOutcome {
    pub marketplace_name: String,
    pub changed: bool,
    pub local_root: Option<PathBuf>,
    pub version: Option<String>,
    pub diagnostics: Vec<crate::PluginDiagnostic>,
}
