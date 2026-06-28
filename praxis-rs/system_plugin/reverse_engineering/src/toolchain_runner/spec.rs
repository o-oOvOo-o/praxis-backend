use crate::ReverseError;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ToolRegistry {
    pub schema_version: u32,
    #[serde(default)]
    pub cache_subdir: Option<String>,
    #[serde(default)]
    pub archive_subdirs: Vec<String>,
    #[serde(default)]
    pub tools: Vec<ToolDescriptor>,
}

impl ToolRegistry {
    pub fn builtin() -> Result<Self, ReverseError> {
        serde_json::from_str(registry::BUILTIN_REVERSE_TOOLCHAINS).map_err(|source| {
            ReverseError::Registry {
                source: Box::new(source),
            }
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ToolDescriptor> {
        self.tools.iter()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ToolDescriptor {
    pub id: String,
    pub label: String,
    pub category: ToolCategory,
    pub required: bool,
    #[serde(default)]
    pub env_vars: Vec<String>,
    #[serde(default)]
    pub path_candidates: Vec<String>,
    #[serde(default)]
    pub cache_candidates: Vec<String>,
    #[serde(default)]
    pub which: Vec<String>,
    #[serde(default)]
    pub archive_names: Vec<String>,
    #[serde(default)]
    pub version_args: Vec<String>,
    #[serde(default)]
    pub sync_hint: Option<String>,
    #[serde(default)]
    pub python_modules: Vec<String>,
    pub redistribution: String,
    pub license_policy: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    NativeDecompiler,
    ManagedDecompiler,
    ShaderReverse,
    NativeDebugger,
    GpuDebugger,
    BinaryInspection,
    ManagedInspection,
    UnityReverse,
    Runtime,
    TargetRuntime,
    RepoScript,
    RepoHarness,
    BuiltHarness,
}

mod registry {
    pub const BUILTIN_REVERSE_TOOLCHAINS: &str = include_str!("registry/reverse_toolchains.json");
}
