use praxis_config::types::ModelAvailabilityNuxConfig;
use praxis_config::types::NotificationMethod;
use praxis_config::types::Notifications;
use praxis_config::types::Tui;
use praxis_core::config::Config;
use praxis_core::config::edit::ConfigEdit;
use praxis_protocol::config_types::AltScreenMode;
use std::collections::HashMap;
use toml_edit::Item as TomlItem;
use toml_edit::value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TuiRuntimeConfig {
    pub(crate) notifications: Notifications,
    pub(crate) notification_method: NotificationMethod,
    pub(crate) animations: bool,
    pub(crate) show_tooltips: bool,
    pub(crate) model_availability_nux: ModelAvailabilityNuxConfig,
    pub(crate) alternate_screen: AltScreenMode,
    pub(crate) status_line: Option<Vec<String>>,
    pub(crate) terminal_title: Option<Vec<String>>,
    pub(crate) theme: Option<String>,
    pub(crate) surface_theme: Option<String>,
}

impl Default for TuiRuntimeConfig {
    fn default() -> Self {
        Self {
            notifications: Notifications::default(),
            notification_method: NotificationMethod::default(),
            animations: true,
            show_tooltips: true,
            model_availability_nux: ModelAvailabilityNuxConfig::default(),
            alternate_screen: AltScreenMode::default(),
            status_line: None,
            terminal_title: None,
            theme: None,
            surface_theme: None,
        }
    }
}

impl From<Tui> for TuiRuntimeConfig {
    fn from(tui: Tui) -> Self {
        Self {
            notifications: tui.notifications,
            notification_method: tui.notification_method,
            animations: tui.animations,
            show_tooltips: tui.show_tooltips,
            model_availability_nux: tui.model_availability_nux,
            alternate_screen: tui.alternate_screen,
            status_line: tui.status_line,
            terminal_title: tui.terminal_title,
            theme: tui.theme,
            surface_theme: tui.surface_theme,
        }
    }
}

impl TuiRuntimeConfig {
    pub(crate) fn from_core_config(config: &Config) -> std::io::Result<Self> {
        let merged = config.config_layer_stack.effective_config();
        let Some(tui_value) = merged.get("tui").cloned() else {
            return Ok(Self::default());
        };
        let tui: Tui = tui_value.try_into().map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid [tui] config: {err}"),
            )
        })?;
        Ok(tui.into())
    }
}

pub(crate) fn syntax_theme_edit(name: &str) -> ConfigEdit {
    ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "theme".to_string()],
        value: value(name.to_string()),
    }
}

pub(crate) fn surface_theme_edit(name: &str) -> ConfigEdit {
    ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "surface_theme".to_string()],
        value: value(name.to_string()),
    }
}

pub(crate) fn status_line_items_edit(items: &[String]) -> ConfigEdit {
    let array = items.iter().cloned().collect::<toml_edit::Array>();

    ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "status_line".to_string()],
        value: TomlItem::Value(array.into()),
    }
}

pub(crate) fn terminal_title_items_edit(items: &[String]) -> ConfigEdit {
    let array = items.iter().cloned().collect::<toml_edit::Array>();

    ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "terminal_title".to_string()],
        value: TomlItem::Value(array.into()),
    }
}

pub(crate) fn model_availability_nux_count_edits(
    shown_count: &HashMap<String, u32>,
) -> Vec<ConfigEdit> {
    let mut shown_count_entries: Vec<_> = shown_count.iter().collect();
    shown_count_entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut edits = vec![ConfigEdit::ClearPath {
        segments: vec!["tui".to_string(), "model_availability_nux".to_string()],
    }];
    for (model_slug, count) in shown_count_entries {
        edits.push(ConfigEdit::SetPath {
            segments: vec![
                "tui".to_string(),
                "model_availability_nux".to_string(),
                model_slug.clone(),
            ],
            value: value(i64::from(*count)),
        });
    }

    edits
}
