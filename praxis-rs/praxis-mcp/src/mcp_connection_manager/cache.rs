use super::*;

const PRAXIS_APPS_TOOLS_CACHE_SCHEMA_VERSION: u8 = 1;
const PRAXIS_APPS_TOOLS_CACHE_DIR: &str = "cache/praxis_apps_tools";
pub(super) const MCP_TOOLS_LIST_DURATION_METRIC: &str = "praxis.mcp.tools.list.duration_ms";
pub(super) const MCP_TOOLS_FETCH_UNCACHED_DURATION_METRIC: &str =
    "praxis.mcp.tools.fetch_uncached.duration_ms";
pub(super) const MCP_TOOLS_CACHE_WRITE_DURATION_METRIC: &str =
    "praxis.mcp.tools.cache_write.duration_ms";

pub fn praxis_apps_tools_cache_key(auth: Option<&OpenAiAccountAuth>) -> PraxisAppsToolsCacheKey {
    let token_data = auth.and_then(|auth| auth.get_token_data().ok());
    let account_id = token_data
        .as_ref()
        .and_then(|token_data| token_data.account_id.clone());
    let chatgpt_user_id = token_data
        .as_ref()
        .and_then(|token_data| token_data.id_token.chatgpt_user_id.clone());
    let is_workspace_account = token_data
        .as_ref()
        .is_some_and(|token_data| token_data.id_token.is_workspace_account());

    PraxisAppsToolsCacheKey {
        account_id,
        chatgpt_user_id,
        is_workspace_account,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PraxisAppsToolsCacheKey {
    pub(super) account_id: Option<String>,
    pub(super) chatgpt_user_id: Option<String>,
    pub(super) is_workspace_account: bool,
}

#[derive(Clone)]
pub(super) struct PraxisAppsToolsCacheContext {
    pub(super) praxis_home: PathBuf,
    pub(super) user_key: PraxisAppsToolsCacheKey,
}

impl PraxisAppsToolsCacheContext {
    fn cache_path(&self) -> PathBuf {
        let user_key_json = serde_json::to_string(&self.user_key).unwrap_or_default();
        let user_key_hash = sha1_hex(&user_key_json);
        self.praxis_home
            .join(PRAXIS_APPS_TOOLS_CACHE_DIR)
            .join(format!("{user_key_hash}.json"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PraxisAppsToolsDiskCache {
    schema_version: u8,
    tools: Vec<ToolInfo>,
}

pub(super) enum CachedPraxisAppsToolsLoad {
    Hit(Vec<ToolInfo>),
    Missing,
    Invalid,
}

pub(super) fn write_cached_praxis_apps_tools_if_needed(
    server_name: &str,
    cache_context: Option<&PraxisAppsToolsCacheContext>,
    tools: &[ToolInfo],
) {
    if server_name != PRAXIS_APPS_MCP_SERVER_NAME {
        return;
    }

    if let Some(cache_context) = cache_context {
        let cache_write_start = Instant::now();
        write_cached_praxis_apps_tools(cache_context, tools);
        emit_duration(
            MCP_TOOLS_CACHE_WRITE_DURATION_METRIC,
            cache_write_start.elapsed(),
            &[],
        );
    }
}

pub(super) fn load_startup_cached_praxis_apps_tools_snapshot(
    server_name: &str,
    cache_context: Option<&PraxisAppsToolsCacheContext>,
) -> Option<Vec<ToolInfo>> {
    if server_name != PRAXIS_APPS_MCP_SERVER_NAME {
        return None;
    }

    let cache_context = cache_context?;

    match load_cached_praxis_apps_tools(cache_context) {
        CachedPraxisAppsToolsLoad::Hit(tools) => Some(tools),
        CachedPraxisAppsToolsLoad::Missing | CachedPraxisAppsToolsLoad::Invalid => None,
    }
}

#[cfg(test)]
pub(super) fn read_cached_praxis_apps_tools(
    cache_context: &PraxisAppsToolsCacheContext,
) -> Option<Vec<ToolInfo>> {
    match load_cached_praxis_apps_tools(cache_context) {
        CachedPraxisAppsToolsLoad::Hit(tools) => Some(tools),
        CachedPraxisAppsToolsLoad::Missing | CachedPraxisAppsToolsLoad::Invalid => None,
    }
}

pub(super) fn load_cached_praxis_apps_tools(
    cache_context: &PraxisAppsToolsCacheContext,
) -> CachedPraxisAppsToolsLoad {
    let cache_path = cache_context.cache_path();
    let bytes = match std::fs::read(cache_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return CachedPraxisAppsToolsLoad::Missing;
        }
        Err(_) => return CachedPraxisAppsToolsLoad::Invalid,
    };
    let cache: PraxisAppsToolsDiskCache = match serde_json::from_slice(&bytes) {
        Ok(cache) => cache,
        Err(_) => return CachedPraxisAppsToolsLoad::Invalid,
    };
    if cache.schema_version != PRAXIS_APPS_TOOLS_CACHE_SCHEMA_VERSION {
        return CachedPraxisAppsToolsLoad::Invalid;
    }
    CachedPraxisAppsToolsLoad::Hit(filter_disallowed_praxis_apps_tools(cache.tools))
}

pub(super) fn write_cached_praxis_apps_tools(
    cache_context: &PraxisAppsToolsCacheContext,
    tools: &[ToolInfo],
) {
    let cache_path = cache_context.cache_path();
    if let Some(parent) = cache_path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let tools = filter_disallowed_praxis_apps_tools(tools.to_vec());
    let Ok(bytes) = serde_json::to_vec_pretty(&PraxisAppsToolsDiskCache {
        schema_version: PRAXIS_APPS_TOOLS_CACHE_SCHEMA_VERSION,
        tools,
    }) else {
        return;
    };
    let _ = std::fs::write(cache_path, bytes);
}
