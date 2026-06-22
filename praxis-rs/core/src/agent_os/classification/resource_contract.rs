use crate::agent_os::ResourceRequirement;
use crate::agent_os::TaskRecord;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::path_scope::wildcard_match;
use std::collections::HashSet;

pub(in crate::agent_os) fn validate_task_action_contract(
    task: &TaskRecord,
    required_capabilities: &[String],
    required_resources: &[ResourceRequirement],
) -> PraxisResult<()> {
    if !task.required_capabilities.is_empty() {
        let declared = task
            .required_capabilities
            .iter()
            .map(|capability| capability.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if let Some(missing) = required_capabilities
            .iter()
            .find(|capability| !declared.contains(&capability.to_ascii_lowercase()))
        {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "action rejected: required capability `{missing}` is outside task capability contract"
            )));
        }
    }
    if !task.required_resources.is_empty() {
        for resource in required_resources {
            if !task
                .required_resources
                .iter()
                .any(|declared| task_resource_allows(declared, resource))
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "action rejected: required resource `{}` is outside task resource contract",
                    resource.key()
                )));
            }
        }
    }
    Ok(())
}

pub(in crate::agent_os) fn task_resource_allows(
    declared: &ResourceRequirement,
    required: &ResourceRequirement,
) -> bool {
    if declared.key() == required.key() {
        return true;
    }
    match (declared, required) {
        (ResourceRequirement::CpuHeavy, ResourceRequirement::CpuHeavy) => true,
        (ResourceRequirement::Port { port: declared }, ResourceRequirement::Port { port }) => {
            declared == port
        }
        (ResourceRequirement::Gpu { scope: declared }, ResourceRequirement::Gpu { scope }) => {
            scoped_resource_allows(declared, scope, true)
        }
        (
            ResourceRequirement::Network { scope: declared },
            ResourceRequirement::Network { scope },
        ) => scoped_resource_allows(declared, scope, true),
        (
            ResourceRequirement::LlmBudget { scope: declared },
            ResourceRequirement::LlmBudget { scope },
        ) => scoped_resource_allows(declared, scope, true),
        (
            ResourceRequirement::BuildCache { scope: declared },
            ResourceRequirement::BuildCache { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::AppRuntime { scope: declared },
            ResourceRequirement::AppRuntime { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::GitIndex { scope: declared },
            ResourceRequirement::GitIndex { scope },
        ) => scoped_resource_allows(declared, scope, false),
        (
            ResourceRequirement::RepoWrite { scope: declared },
            ResourceRequirement::RepoWrite { scope },
        ) => repo_write_resource_allows(declared, scope),
        _ => false,
    }
}

fn scoped_resource_allows(declared: &str, required: &str, allow_default: bool) -> bool {
    let declared = normalize_resource_scope(declared);
    let required = normalize_resource_scope(required);
    declared == required
        || declared == "*"
        || declared == "**"
        || (allow_default && declared == "default")
        || (declared.contains('*') && wildcard_match(declared.as_str(), required.as_str()))
}

fn repo_write_resource_allows(declared: &str, required: &str) -> bool {
    let declared = normalize_resource_scope(declared);
    let required = normalize_resource_scope(required);
    if declared == required || declared == "*" || declared == "**" {
        return true;
    }
    if declared.starts_with("repo:") {
        return scoped_resource_allows(declared.as_str(), required.as_str(), false);
    }
    // Path-scoped repo_write contracts are finalized after dirty-file audit.
    required.starts_with("repo:")
}

fn normalize_resource_scope(scope: &str) -> String {
    scope.trim().replace('\\', "/").to_ascii_lowercase()
}
