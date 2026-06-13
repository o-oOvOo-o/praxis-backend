pub(crate) use crate::surface::SurfaceTheme as CenterTheme;
pub(crate) use crate::surface::SurfaceThemeKind as CenterThemeKind;

pub(crate) fn for_preference(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> CenterTheme {
    crate::surface::resolve_theme(preference, provider_id, model_label)
}

pub(crate) fn kind_for_preference(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> CenterThemeKind {
    crate::surface::resolve_kind(preference, provider_id, model_label)
}
