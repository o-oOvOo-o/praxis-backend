use super::*;
use praxis_config::CONFIG_TOML_FILE;
use praxis_config::ConfigLayerEntry;
use praxis_config::ConfigLayerStack;
use praxis_config::ConfigRequirements;
use praxis_config::ConfigRequirementsToml;
use praxis_protocol::protocol::Product;
use praxis_protocol::protocol::SkillScope;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::path::Path;
use tempfile::TempDir;
use toml::Value as TomlValue;

const REPO_ROOT_CONFIG_DIR_NAME: &str = ".praxis";

struct TestConfig {
    cwd: PathBuf,
    config_layer_stack: ConfigLayerStack,
}

async fn make_config(praxis_home: &TempDir) -> TestConfig {
    make_config_for_cwd(praxis_home, praxis_home.path().to_path_buf()).await
}

fn config_file(path: PathBuf) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("config file path should be absolute")
}

fn project_layers_for_cwd(cwd: &Path) -> Vec<ConfigLayerEntry> {
    let cwd_dir = if cwd.is_dir() {
        cwd.to_path_buf()
    } else {
        cwd.parent()
            .expect("file cwd should have a parent directory")
            .to_path_buf()
    };
    let project_root = cwd_dir
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .unwrap_or(cwd_dir.as_path())
        .to_path_buf();

    let mut layers = cwd_dir
        .ancestors()
        .scan(false, |done, dir| {
            if *done {
                None
            } else {
                if dir == project_root {
                    *done = true;
                }
                Some(dir.to_path_buf())
            }
        })
        .collect::<Vec<_>>();
    layers.reverse();

    layers
        .into_iter()
        .filter_map(|dir| {
            let dot_praxis = dir.join(REPO_ROOT_CONFIG_DIR_NAME);
            dot_praxis.is_dir().then(|| {
                ConfigLayerEntry::new(
                    ConfigLayerSource::Project {
                        dot_praxis_folder: AbsolutePathBuf::from_absolute_path(dot_praxis)
                            .expect("project .praxis path should be absolute"),
                    },
                    TomlValue::Table(toml::map::Map::new()),
                )
            })
        })
        .collect()
}

async fn make_config_for_cwd(praxis_home: &TempDir, cwd: PathBuf) -> TestConfig {
    let user_config_path = praxis_home.path().join(CONFIG_TOML_FILE);
    let system_config_path = praxis_home.path().join("etc/praxis/config.toml");
    fs::create_dir_all(
        system_config_path
            .parent()
            .expect("system config path should have a parent"),
    )
    .expect("create fake system config dir");

    let mut layers = vec![
        ConfigLayerEntry::new(
            ConfigLayerSource::System {
                file: config_file(system_config_path),
            },
            TomlValue::Table(toml::map::Map::new()),
        ),
        ConfigLayerEntry::new(
            ConfigLayerSource::User {
                file: config_file(user_config_path),
            },
            TomlValue::Table(toml::map::Map::new()),
        ),
    ];
    layers.extend(project_layers_for_cwd(&cwd));

    TestConfig {
        cwd,
        config_layer_stack: ConfigLayerStack::new(
            layers,
            ConfigRequirements::default(),
            ConfigRequirementsToml::default(),
        )
        .expect("valid config layer stack"),
    }
}

fn load_skills_for_test(config: &TestConfig) -> SkillLoadOutcome {
    // Keep unit tests hermetic by never scanning the real `$HOME/.agents/skills`.
    super::load_skills_from_roots(super::skill_roots_with_home_dir(
        &config.config_layer_stack,
        &config.cwd,
        /*home_dir*/ None,
        Vec::new(),
    ))
}

fn mark_as_git_repo(dir: &Path) {
    // Config/project-root discovery only checks for the presence of `.git` (file or dir),
    // so we can avoid shelling out to `git init` in tests.
    fs::write(dir.join(".git"), "gitdir: fake\n").unwrap();
}

fn normalized(path: &Path) -> PathBuf {
    canonicalize_path(path).unwrap_or_else(|_| path.to_path_buf())
}

fn write_skill(praxis_home: &TempDir, dir: &str, name: &str, description: &str) -> PathBuf {
    write_skill_at(&praxis_home.path().join("skills"), dir, name, description)
}

fn write_system_skill(praxis_home: &TempDir, dir: &str, name: &str, description: &str) -> PathBuf {
    write_skill_at(
        &praxis_home.path().join("skills/.system"),
        dir,
        name,
        description,
    )
}

fn write_skill_at(root: &Path, dir: &str, name: &str, description: &str) -> PathBuf {
    let skill_dir = root.join(dir);
    fs::create_dir_all(&skill_dir).unwrap();
    let indented_description = description.replace('\n', "\n  ");
    let content =
        format!("---\nname: {name}\ndescription: |-\n  {indented_description}\n---\n\n# Body\n");
    let path = skill_dir.join(SKILLS_FILENAME);
    fs::write(&path, content).unwrap();
    path
}

fn write_raw_skill_at(root: &Path, dir: &str, frontmatter: &str) -> PathBuf {
    let skill_dir = root.join(dir);
    fs::create_dir_all(&skill_dir).unwrap();
    let path = skill_dir.join(SKILLS_FILENAME);
    let content = format!("---\n{frontmatter}\n---\n\n# Body\n");
    fs::write(&path, content).unwrap();
    path
}

fn write_skill_metadata_at(skill_dir: &Path, contents: &str) -> PathBuf {
    let path = skill_dir
        .join(SKILLS_METADATA_DIR)
        .join(SKILLS_METADATA_FILENAME);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, contents).unwrap();
    path
}

fn write_skill_interface_at(skill_dir: &Path, contents: &str) -> PathBuf {
    write_skill_metadata_at(skill_dir, contents)
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[path = "loader_tests/metadata_and_policy.rs"]
mod metadata_and_policy;
#[path = "loader_tests/repo_project_layers.rs"]
mod repo_project_layers;
#[path = "loader_tests/root_resolution.rs"]
mod root_resolution;
#[path = "loader_tests/skill_validation.rs"]
mod skill_validation;
#[path = "loader_tests/symlink_scanning.rs"]
mod symlink_scanning;
#[path = "loader_tests/system_admin_scope.rs"]
mod system_admin_scope;
