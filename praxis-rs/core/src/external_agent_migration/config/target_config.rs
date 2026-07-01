use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use toml::Value as TomlValue;

pub(super) fn home_path(praxis_home: &Path) -> PathBuf {
    praxis_home.join("config.toml")
}

pub(super) fn repo_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".codex").join("config.toml")
}

pub(super) fn needs_values(target_config: &Path, migrated: &TomlValue) -> io::Result<bool> {
    if !target_config.exists() {
        return Ok(true);
    }

    let existing_raw = fs::read_to_string(target_config)?;
    let mut existing = parse_existing_toml_config(&existing_raw)?;
    merge_missing_toml_values(&mut existing, migrated)
}

pub(super) fn merge_or_create(target_config: &Path, migrated: &TomlValue) -> io::Result<()> {
    let Some(target_parent) = target_config.parent() else {
        return Err(super::invalid_data_error(
            "config target path has no parent",
        ));
    };
    fs::create_dir_all(target_parent)?;

    if !target_config.exists() {
        write_toml_file(target_config, migrated)?;
        return Ok(());
    }

    let existing_raw = fs::read_to_string(target_config)?;
    let mut existing = parse_existing_toml_config(&existing_raw)?;

    if !merge_missing_toml_values(&mut existing, migrated)? {
        return Ok(());
    }

    write_toml_file(target_config, &existing)
}

fn parse_existing_toml_config(existing_raw: &str) -> io::Result<TomlValue> {
    if existing_raw.trim().is_empty() {
        return Ok(TomlValue::Table(Default::default()));
    }

    toml::from_str::<TomlValue>(existing_raw)
        .map_err(|err| super::invalid_data_error(format!("invalid existing config.toml: {err}")))
}

fn merge_missing_toml_values(existing: &mut TomlValue, incoming: &TomlValue) -> io::Result<bool> {
    match (existing, incoming) {
        (TomlValue::Table(existing_table), TomlValue::Table(incoming_table)) => {
            let mut changed = false;
            for (key, incoming_value) in incoming_table {
                match existing_table.get_mut(key) {
                    Some(existing_value) => {
                        if matches!(
                            (&*existing_value, incoming_value),
                            (TomlValue::Table(_), TomlValue::Table(_))
                        ) && merge_missing_toml_values(existing_value, incoming_value)?
                        {
                            changed = true;
                        }
                    }
                    None => {
                        existing_table.insert(key.clone(), incoming_value.clone());
                        changed = true;
                    }
                }
            }
            Ok(changed)
        }
        _ => Err(super::invalid_data_error(
            "expected TOML table while merging migrated config values",
        )),
    }
}

fn write_toml_file(path: &Path, value: &TomlValue) -> io::Result<()> {
    let serialized = toml::to_string_pretty(value)
        .map_err(|err| super::invalid_data_error(format!("failed to serialize config.toml: {err}")))?;
    fs::write(path, format!("{}\n", serialized.trim_end()))
}
