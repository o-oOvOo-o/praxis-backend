#[cfg(test)]
use crate::constants::CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE;
use crate::constants::{
    OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_ID, OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_NAME,
};
use praxis_core::config_loader::{
    CloudConfigBundle, CloudConfigBundleLoadError, CloudConfigBundleLoadErrorCode,
    CloudConfigTomlBundle, CloudRequirementsFragment, CloudRequirementsLoadError,
    CloudRequirementsLoadErrorCode, CloudRequirementsTomlBundle, ConfigRequirementsToml,
};

pub(crate) fn parse_cloud_requirements(
    contents: &str,
) -> Result<Option<ConfigRequirementsToml>, toml::de::Error> {
    if contents.trim().is_empty() {
        return Ok(None);
    }

    let requirements: ConfigRequirementsToml = toml::from_str(contents)?;
    if requirements.is_empty() {
        Ok(None)
    } else {
        Ok(Some(requirements))
    }
}

pub(crate) fn bundle_from_requirements_contents(
    contents: Option<String>,
) -> Result<Option<CloudConfigBundle>, toml::de::Error> {
    let Some(contents) = contents else {
        return Ok(None);
    };
    if parse_cloud_requirements(&contents)?.is_none() {
        return Ok(None);
    }

    Ok(Some(CloudConfigBundle {
        config_toml: CloudConfigTomlBundle::default(),
        requirements_toml: CloudRequirementsTomlBundle {
            enterprise_managed: vec![CloudRequirementsFragment {
                id: OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_ID.to_string(),
                name: OPENAI_PRAXIS_REQUIREMENTS_FRAGMENT_NAME.to_string(),
                contents,
            }],
            ..Default::default()
        },
    }))
}

pub(crate) fn requirements_from_bundle_option(
    bundle: Option<CloudConfigBundle>,
) -> Result<Option<ConfigRequirementsToml>, toml::de::Error> {
    match bundle {
        Some(bundle) => requirements_from_bundle(&bundle),
        None => Ok(None),
    }
}

pub(crate) fn requirements_from_bundle(
    bundle: &CloudConfigBundle,
) -> Result<Option<ConfigRequirementsToml>, toml::de::Error> {
    if let Some(fragment) = bundle.requirements_toml.parsed_enterprise_managed.first() {
        return Ok(Some(fragment.requirements.clone()));
    }

    bundle
        .requirements_toml
        .enterprise_managed
        .first()
        .map(|fragment| parse_cloud_requirements(&fragment.contents))
        .transpose()
        .map(Option::flatten)
}

pub(crate) fn cloud_bundle_error_to_requirements_error(
    err: CloudConfigBundleLoadError,
) -> CloudRequirementsLoadError {
    let code = match err.code() {
        CloudConfigBundleLoadErrorCode::Auth => CloudRequirementsLoadErrorCode::Auth,
        CloudConfigBundleLoadErrorCode::Timeout => CloudRequirementsLoadErrorCode::Timeout,
        CloudConfigBundleLoadErrorCode::RequestFailed => {
            CloudRequirementsLoadErrorCode::RequestFailed
        }
        CloudConfigBundleLoadErrorCode::InvalidBundle => CloudRequirementsLoadErrorCode::Parse,
        CloudConfigBundleLoadErrorCode::Internal => CloudRequirementsLoadErrorCode::Internal,
    };
    CloudRequirementsLoadError::new(code, err.status_code(), err.to_string())
}

#[cfg(test)]
pub(crate) fn requirements_parse_error() -> CloudRequirementsLoadError {
    CloudRequirementsLoadError::new(
        CloudRequirementsLoadErrorCode::Parse,
        None,
        CLOUD_REQUIREMENTS_LOAD_FAILED_MESSAGE,
    )
}
