use serde_json::Value as JsonValue;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use toml::Value as TomlValue;

pub(super) fn default_home() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home).join(".claude");
    }

    PathBuf::from(".claude")
}

pub(super) fn home_settings(home: &Path) -> PathBuf {
    home.join("settings.json")
}

pub(super) fn repo_settings(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("settings.json")
}

pub(super) fn home_skills(home: &Path) -> PathBuf {
    home.join("skills")
}

pub(super) fn repo_skills(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("skills")
}

pub(super) fn home_agents_md(home: &Path) -> PathBuf {
    home.join("CLAUDE.md")
}

pub(super) fn repo_agents_md_candidates(repo_root: &Path) -> [PathBuf; 2] {
    [
        repo_root.join("CLAUDE.md"),
        repo_root.join(".claude").join("CLAUDE.md"),
    ]
}

pub(super) fn load_migrated_config(source_settings: &Path) -> io::Result<Option<TomlValue>> {
    if !source_settings.is_file() {
        return Ok(None);
    }

    let raw_settings = fs::read_to_string(source_settings)?;
    let settings: JsonValue =
        serde_json::from_str(&raw_settings).map_err(|err| invalid_data_error(err.to_string()))?;
    let migrated = build_config_migration(&settings)?;
    Ok((!is_empty_toml_table(&migrated)).then_some(migrated))
}

pub(super) fn rewrite_and_copy_text_file(source: &Path, target: &Path) -> io::Result<()> {
    let source_contents = fs::read_to_string(source)?;
    let rewritten = rewrite_terms(&source_contents);
    fs::write(target, rewritten)
}

fn rewrite_terms(content: &str) -> String {
    let mut rewritten = replace_case_insensitive_with_boundaries(content, "claude.md", "AGENTS.md");
    for from in [
        "claude code",
        "claude-code",
        "claude_code",
        "claudecode",
        "claude",
    ] {
        rewritten = replace_case_insensitive_with_boundaries(&rewritten, from, "Praxis");
    }
    rewritten
}

fn replace_case_insensitive_with_boundaries(
    input: &str,
    needle: &str,
    replacement: &str,
) -> String {
    let needle_lower = needle.to_ascii_lowercase();
    if needle_lower.is_empty() {
        return input.to_string();
    }

    let haystack_lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut last_emitted = 0usize;
    let mut search_start = 0usize;

    while let Some(relative_pos) = haystack_lower[search_start..].find(&needle_lower) {
        let start = search_start + relative_pos;
        let end = start + needle_lower.len();
        let boundary_before = start == 0 || !is_word_byte(bytes[start - 1]);
        let boundary_after = end == bytes.len() || !is_word_byte(bytes[end]);

        if boundary_before && boundary_after {
            output.push_str(&input[last_emitted..start]);
            output.push_str(replacement);
            last_emitted = end;
        }

        search_start = start + 1;
    }

    if last_emitted == 0 {
        return input.to_string();
    }

    output.push_str(&input[last_emitted..]);
    output
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn build_config_migration(settings: &JsonValue) -> io::Result<TomlValue> {
    let Some(settings_obj) = settings.as_object() else {
        return Err(invalid_data_error("Claude settings root must be an object"));
    };

    let mut root = toml::map::Map::new();

    if let Some(env) = settings_obj.get("env").and_then(JsonValue::as_object)
        && !env.is_empty()
    {
        let mut shell_policy = toml::map::Map::new();
        shell_policy.insert("inherit".to_string(), TomlValue::String("core".to_string()));
        shell_policy.insert("set".to_string(), TomlValue::Table(env_to_toml_table(env)));
        root.insert(
            "shell_environment_policy".to_string(),
            TomlValue::Table(shell_policy),
        );
    }

    if let Some(sandbox_enabled) = settings_obj
        .get("sandbox")
        .and_then(JsonValue::as_object)
        .and_then(|sandbox| sandbox.get("enabled"))
        .and_then(JsonValue::as_bool)
        && sandbox_enabled
    {
        root.insert(
            "sandbox_mode".to_string(),
            TomlValue::String("workspace-write".to_string()),
        );
    }

    Ok(TomlValue::Table(root))
}

fn env_to_toml_table(
    object: &serde_json::Map<String, JsonValue>,
) -> toml::map::Map<String, TomlValue> {
    let mut table = toml::map::Map::new();
    for (key, value) in object {
        if let Some(value) = env_value_to_string(value) {
            table.insert(key.clone(), TomlValue::String(value));
        }
    }
    table
}

fn env_value_to_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Null => None,
        JsonValue::Bool(value) => Some(value.to_string()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::Array(_) | JsonValue::Object(_) => None,
    }
}

fn is_empty_toml_table(value: &TomlValue) -> bool {
    match value {
        TomlValue::Table(table) => table.is_empty(),
        TomlValue::String(_)
        | TomlValue::Integer(_)
        | TomlValue::Float(_)
        | TomlValue::Boolean(_)
        | TomlValue::Datetime(_)
        | TomlValue::Array(_) => false,
    }
}

fn invalid_data_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
