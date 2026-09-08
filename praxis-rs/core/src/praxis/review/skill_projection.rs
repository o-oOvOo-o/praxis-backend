use std::collections::HashSet;
use std::path::PathBuf;

use praxis_protocol::protocol::SkillErrorInfo;
use praxis_protocol::protocol::SkillMetadata as ProtocolSkillMetadata;

use crate::SkillError;
use crate::SkillMetadata;

pub(in crate::praxis) fn skills_to_info(
    skills: &[SkillMetadata],
    disabled_paths: &HashSet<PathBuf>,
) -> Vec<ProtocolSkillMetadata> {
    skills
        .iter()
        .map(|skill| {
            let mut info = ProtocolSkillMetadata::from(skill);
            info.enabled = !disabled_paths.contains(&skill.path_to_skills_md);
            info
        })
        .collect()
}

pub(in crate::praxis) fn errors_to_info(errors: &[SkillError]) -> Vec<SkillErrorInfo> {
    errors
        .iter()
        .map(|err| SkillErrorInfo {
            path: err.path.clone(),
            message: err.message.clone(),
        })
        .collect()
}
