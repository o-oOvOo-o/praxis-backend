pub(crate) use praxis_skills::install_system_skills;
pub(crate) use praxis_skills::system_cache_root_dir;

use std::path::Path;

pub(crate) fn uninstall_system_skills(praxis_home: &Path) {
    let system_skills_dir = system_cache_root_dir(praxis_home);
    let _ = std::fs::remove_dir_all(&system_skills_dir);
}
