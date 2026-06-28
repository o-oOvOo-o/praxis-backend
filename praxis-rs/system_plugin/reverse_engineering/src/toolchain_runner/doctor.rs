use crate::ReverseError;
use crate::toolchain_runner::spec::ToolCategory;
use crate::toolchain_runner::spec::ToolDescriptor;
use crate::toolchain_runner::spec::ToolRegistry;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub entries: Vec<DoctorEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DoctorEntry {
    pub id: String,
    pub label: String,
    pub category: ToolCategory,
    pub required: bool,
    pub available: bool,
    pub resolution: Option<String>,
    pub version: Option<String>,
    pub license_policy: String,
    pub redistribution: String,
    pub sync_hint: Option<String>,
}

pub fn diagnose_default_registry() -> Result<DoctorReport, ReverseError> {
    diagnose(&ToolRegistry::builtin()?)
}

pub fn diagnose(registry: &ToolRegistry) -> Result<DoctorReport, ReverseError> {
    let entries = registry.iter().map(diagnose_one).collect();
    Ok(DoctorReport {
        schema_version: registry.schema_version,
        entries,
    })
}

fn diagnose_one(desc: &ToolDescriptor) -> DoctorEntry {
    let resolution = resolve_one(desc);
    DoctorEntry {
        id: desc.id.clone(),
        label: desc.label.clone(),
        category: desc.category,
        required: desc.required,
        available: resolution.is_some(),
        resolution,
        version: None,
        license_policy: desc.license_policy.clone(),
        redistribution: desc.redistribution.clone(),
        sync_hint: desc.sync_hint.clone(),
    }
}

fn resolve_one(desc: &ToolDescriptor) -> Option<String> {
    for env_var in &desc.env_vars {
        let Ok(value) = std::env::var(env_var) else {
            continue;
        };
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(format!("env:{env_var}"));
        }
    }

    for candidate in &desc.path_candidates {
        if candidate_exists(candidate) {
            return Some("path_candidate".to_string());
        }
    }

    for candidate in &desc.cache_candidates {
        if candidate_exists(candidate) {
            return Some("cache_candidate".to_string());
        }
    }

    for binary in &desc.which {
        if which::which(binary).is_ok() {
            return Some(format!("which:{binary}"));
        }
    }

    None
}

fn candidate_exists(candidate: &str) -> bool {
    if candidate.contains('*') {
        return resolve_glob_like(candidate).is_some();
    }
    Path::new(candidate).exists()
}

fn resolve_glob_like(candidate: &str) -> Option<PathBuf> {
    let star = candidate.find('*')?;
    let segment_start = candidate[..star]
        .rfind(['\\', '/'])
        .map(|index| index + 1)
        .unwrap_or(0);
    let segment_end = candidate[star..]
        .find(['\\', '/'])
        .map(|offset| star + offset)
        .unwrap_or(candidate.len());
    let parent = if segment_start == 0 {
        PathBuf::from(".")
    } else {
        PathBuf::from(&candidate[..segment_start - 1])
    };
    let pattern = &candidate[segment_start..segment_end];
    let suffix_tail = candidate[segment_end..].trim_start_matches(['\\', '/']);
    let (prefix, suffix) = pattern.split_once('*').unwrap_or((pattern, ""));
    let entries = std::fs::read_dir(parent).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let path = if suffix_tail.is_empty() {
            entry.path()
        } else {
            entry.path().join(suffix_tail)
        };
        if path.exists() {
            return Some(path);
        }
        if path.to_string_lossy().contains('*')
            && let Some(path) = resolve_glob_like(&path.to_string_lossy())
        {
            return Some(path);
        }
    }
    None
}
