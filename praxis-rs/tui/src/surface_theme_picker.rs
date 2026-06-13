use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::render::renderable::Renderable;
use crate::surface;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

struct SurfaceThemePreview;

pub(crate) fn build_surface_theme_picker_params(
    current_name: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> SelectionViewParams {
    let previous_name = current_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let current_id = previous_name
        .as_deref()
        .unwrap_or("auto")
        .to_ascii_lowercase();
    let auto_kind = surface::resolve_kind(None, provider_id, model_label);

    let mut items = Vec::new();
    let mut initial_selected_idx = None;
    let auto_is_current = current_id == "auto" || previous_name.is_none();
    if auto_is_current {
        initial_selected_idx = Some(0);
    }
    let auto_previous = previous_name.clone();
    items.push(SelectionItem {
        name: "Auto".to_string(),
        description: Some(format!(
            "Dark normally, {} for this model",
            auto_kind.label()
        )),
        is_current: auto_is_current,
        dismiss_on_select: true,
        search_value: Some("auto model default deepseek dark".to_string()),
        actions: vec![Box::new(move |tx| {
            tx.send(AppEvent::SurfaceThemeSelected {
                name: "auto".to_string(),
                previous_name: auto_previous.clone(),
            });
        })],
        ..Default::default()
    });

    for kind in surface::all_theme_kinds() {
        let idx = items.len();
        let id = kind.id();
        let is_current = current_id == id;
        if is_current {
            initial_selected_idx = Some(idx);
        }
        let previous = previous_name.clone();
        let action_name = id.to_string();
        items.push(SelectionItem {
            name: kind.label().to_string(),
            description: Some(kind.description().to_string()),
            is_current,
            dismiss_on_select: true,
            search_value: Some(format!("{} {}", id, kind.description())),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::SurfaceThemeSelected {
                    name: action_name.clone(),
                    previous_name: previous.clone(),
                });
            })],
            ..Default::default()
        });
    }

    let preview_names = ["auto", "dark", "classic", "deepseek"]
        .into_iter()
        .map(|value| Some(value.to_string()))
        .collect::<Vec<_>>();
    let on_selection_changed = Some(Box::new(move |idx: usize, tx: &AppEventSender| {
        if let Some(name) = preview_names.get(idx).cloned().flatten() {
            tx.send(AppEvent::SurfaceThemePreview { name: Some(name) });
        }
    })
        as Box<dyn Fn(usize, &crate::app_event_sender::AppEventSender) + Send + Sync>);

    let cancel_name = previous_name.clone();
    let on_cancel = Some(Box::new(move |tx: &AppEventSender| {
        tx.send(AppEvent::SurfaceThemePreview {
            name: cancel_name.clone(),
        });
    })
        as Box<dyn Fn(&crate::app_event_sender::AppEventSender) + Send + Sync>);

    SelectionViewParams {
        title: Some("Select Surface Theme".to_string()),
        subtitle: Some(
            "Changes Praxis chrome only. Syntax highlighting stays under /theme.".to_string(),
        ),
        footer_hint: Some(standard_popup_hint_line()),
        items,
        is_searchable: true,
        search_placeholder: Some("Type to filter surface themes...".to_string()),
        initial_selected_idx,
        side_content: Box::new(SurfaceThemePreview),
        side_content_width: crate::bottom_pane::SideContentWidth::Fixed(34),
        side_content_min_width: 28,
        stacked_side_content: Some(Box::new(SurfaceThemePreview)),
        preserve_side_content_bg: true,
        on_selection_changed,
        on_cancel,
        ..Default::default()
    }
}

impl Renderable for SurfaceThemePreview {
    fn desired_height(&self, _width: u16) -> u16 {
        7
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let theme = surface::runtime_theme();
        let title = Span::styled(
            format!(" {} ", theme.kind.label()),
            Style::default()
                .fg(theme.title_fg)
                .add_modifier(Modifier::BOLD),
        );
        let lines = vec![
            Line::from(title),
            Line::from(""),
            Line::from(vec![
                Span::styled("Panel ", Style::default().fg(theme.text).bg(theme.panel_bg)),
                Span::styled(
                    "Selected ",
                    Style::default().fg(theme.text_strong).bg(theme.selected_bg),
                ),
            ]),
            Line::from(vec![
                Span::styled("Input ", Style::default().fg(theme.text).bg(theme.input_bg)),
                Span::styled(
                    "Accent",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "This is the shared Praxis Surface structure.",
                Style::default().fg(theme.muted),
            )),
        ];
        Paragraph::new(lines)
            .style(Style::default().fg(theme.text).bg(theme.panel_bg))
            .render(area, buf);
    }
}
