pub(crate) use crate::surface::SurfaceTheme as WorkspaceTheme;
pub(crate) use crate::surface::SurfaceThemeKind as WorkspaceThemeKind;

pub(crate) fn for_preference(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> WorkspaceTheme {
    crate::surface::resolve_theme(preference, provider_id, model_label)
}

pub(crate) fn kind_for_preference(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> WorkspaceThemeKind {
    crate::surface::resolve_kind(preference, provider_id, model_label)
}
