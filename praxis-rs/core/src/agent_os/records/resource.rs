use praxis_loop::tool::EffectKey;
use praxis_loop::tool::ToolEffect;
use praxis_loop::tool::ToolEffects;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ResourceRequirement {
    CpuHeavy,
    BuildCache { scope: String },
    AppRuntime { scope: String },
    Port { port: u16 },
    RepoWrite { scope: String },
    LlmBudget { scope: String },
    Gpu { scope: String },
    Network { scope: String },
    GitIndex { scope: String },
}

impl ResourceRequirement {
    pub(crate) fn key(&self) -> String {
        match self {
            Self::CpuHeavy => "cpu_heavy:global".to_string(),
            Self::BuildCache { scope } => format!("build_cache:{scope}"),
            Self::AppRuntime { scope } => format!("app_runtime:{scope}"),
            Self::Port { port } => format!("port:{port}"),
            Self::RepoWrite { scope } => format!("repo_write:{scope}"),
            Self::LlmBudget { scope } => format!("llm_budget:{scope}"),
            Self::Gpu { scope } => format!("gpu:{scope}"),
            Self::Network { scope } => format!("network:{scope}"),
            Self::GitIndex { scope } => format!("git_index:{scope}"),
        }
    }

    pub(crate) fn parse_spec(resource: &str) -> Result<Self, String> {
        resource.parse()
    }

    pub(in crate::agent_os) fn resource_type(&self) -> &'static str {
        match self {
            Self::CpuHeavy => "cpu_heavy",
            Self::BuildCache { .. } => "build_cache",
            Self::AppRuntime { .. } => "app_runtime",
            Self::Port { .. } => "port",
            Self::RepoWrite { .. } => "repo_write",
            Self::LlmBudget { .. } => "llm_budget",
            Self::Gpu { .. } => "gpu",
            Self::Network { .. } => "network",
            Self::GitIndex { .. } => "git_index",
        }
    }

    pub(in crate::agent_os) fn mode(&self) -> LeaseMode {
        match self {
            Self::CpuHeavy | Self::LlmBudget { .. } => LeaseMode::Capacity,
            _ => LeaseMode::Exclusive,
        }
    }

    pub(in crate::agent_os) fn conflicts_with(&self, other: &Self) -> bool {
        match (self.mode(), other.mode()) {
            (LeaseMode::Capacity, _) | (_, LeaseMode::Capacity) => self.key() == other.key(),
            (LeaseMode::Advisory | LeaseMode::Shared, _)
            | (_, LeaseMode::Advisory | LeaseMode::Shared) => false,
            (LeaseMode::Exclusive, LeaseMode::Exclusive) => {
                self.effects().conflicts(&other.effects())
            }
        }
    }

    fn effects(&self) -> ToolEffects {
        let (domain, scope) = match self {
            Self::CpuHeavy => ("agent_os.cpu", "global".to_string()),
            Self::BuildCache { scope } => ("agent_os.build_cache", scope.clone()),
            Self::AppRuntime { scope } => ("agent_os.app_runtime", scope.clone()),
            Self::Port { port } => ("agent_os.port", port.to_string()),
            Self::RepoWrite { scope } => ("agent_os.repo", scope.clone()),
            Self::LlmBudget { scope } => ("agent_os.llm_budget", scope.clone()),
            Self::Gpu { scope } => ("agent_os.gpu", scope.clone()),
            Self::Network { scope } => ("agent_os.network", scope.clone()),
            Self::GitIndex { scope } => ("agent_os.git_index", scope.clone()),
        };
        let key = if matches!(scope.as_str(), "*" | "**")
            || (matches!(self, Self::Gpu { .. } | Self::Network { .. }) && scope == "default")
        {
            EffectKey::root(domain)
        } else {
            EffectKey::hierarchical(domain, [normalize_effect_scope(&scope)])
        };
        ToolEffects::new([ToolEffect::exclusive(key)])
    }
}

fn normalize_effect_scope(scope: &str) -> String {
    scope.trim().replace('\\', "/").to_ascii_lowercase()
}

impl fmt::Display for ResourceRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CpuHeavy => f.write_str("cpu_heavy"),
            Self::BuildCache { scope } => write!(f, "build_cache:{scope}"),
            Self::AppRuntime { scope } => write!(f, "app_runtime:{scope}"),
            Self::Port { port } => write!(f, "port:{port}"),
            Self::RepoWrite { scope } => write!(f, "repo_write:{scope}"),
            Self::LlmBudget { scope } => write!(f, "llm_budget:{scope}"),
            Self::Gpu { scope } => write!(f, "gpu:{scope}"),
            Self::Network { scope } => write!(f, "network:{scope}"),
            Self::GitIndex { scope } => write!(f, "git_index:{scope}"),
        }
    }
}

impl FromStr for ResourceRequirement {
    type Err = String;

    fn from_str(resource: &str) -> Result<Self, Self::Err> {
        let resource = resource.trim();
        if resource.is_empty() {
            return Err("resource requirement cannot be empty".to_string());
        }
        let (kind, scope) = resource
            .split_once(':')
            .map(|(kind, scope)| (kind.trim(), Some(scope.trim())))
            .unwrap_or((resource, None));
        match kind {
            "cpu_heavy" => Ok(Self::CpuHeavy),
            "build_cache" => Ok(Self::BuildCache {
                scope: required_resource_scope(resource, scope)?,
            }),
            "app_runtime" => Ok(Self::AppRuntime {
                scope: required_resource_scope(resource, scope)?,
            }),
            "port" => {
                let port = required_resource_scope(resource, scope)?
                    .parse::<u16>()
                    .map_err(|_| format!("port resource must use a u16 port: `{resource}`"))?;
                Ok(Self::Port { port })
            }
            "repo_write" | "file_write" => Ok(Self::RepoWrite {
                scope: required_resource_scope(resource, scope)?,
            }),
            "llm_budget" => Ok(Self::LlmBudget {
                scope: optional_resource_scope(scope, "task"),
            }),
            "gpu" => Ok(Self::Gpu {
                scope: optional_resource_scope(scope, "default"),
            }),
            "network" => Ok(Self::Network {
                scope: optional_resource_scope(scope, "default"),
            }),
            "git_index" => Ok(Self::GitIndex {
                scope: required_resource_scope(resource, scope)?,
            }),
            _ => Err(format!("unknown resource requirement `{resource}`")),
        }
    }
}

fn required_resource_scope(resource: &str, scope: Option<&str>) -> Result<String, String> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return Err(format!(
            "resource requirement `{resource}` requires a scope"
        ));
    };
    Ok(scope.to_string())
}

fn optional_resource_scope(scope: Option<&str>, default_scope: &str) -> String {
    scope
        .filter(|scope| !scope.is_empty())
        .unwrap_or(default_scope)
        .to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LeaseMode {
    Exclusive,
    Shared,
    Capacity,
    Advisory,
}

#[cfg(test)]
mod tests {
    use super::ResourceRequirement;

    #[test]
    fn default_network_scope_conflicts_with_named_network_scope() {
        let default = ResourceRequirement::Network {
            scope: "default".to_string(),
        };
        let named = ResourceRequirement::Network {
            scope: "external_tool".to_string(),
        };
        assert!(default.conflicts_with(&named));
    }

    #[test]
    fn distinct_repo_scopes_do_not_conflict() {
        let left = ResourceRequirement::RepoWrite {
            scope: "repo:a".to_string(),
        };
        let right = ResourceRequirement::RepoWrite {
            scope: "repo:b".to_string(),
        };
        assert!(!left.conflicts_with(&right));
    }
}

impl LeaseMode {
    pub(in crate::agent_os) fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
            Self::Capacity => "capacity",
            Self::Advisory => "advisory",
        }
    }
}
