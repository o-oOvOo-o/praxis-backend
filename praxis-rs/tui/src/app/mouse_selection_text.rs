use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::mouse_interaction::MousePane;

#[derive(Debug, Clone, Copy)]
pub(super) struct MouseDragSelection {
    pub(super) pane: MousePane,
    pub(super) mode: MouseSelectionMode,
    pub(super) start_column: u16,
    pub(super) start_row: u16,
    pub(super) end_column: u16,
    pub(super) end_row: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MouseSelectionMode {
    Range,
    FullPane,
}

pub(super) fn selection_blocks_pane_scroll(
    selection: Option<MouseDragSelection>,
    drag: Option<MouseDragSelection>,
    pane: MousePane,
) -> bool {
    selection
        .or(drag)
        .is_some_and(|selection| selection.pane == pane)
}

#[derive(Debug, Clone)]
pub(super) struct PaneTextSnapshot {
    pub(super) area: Rect,
    pub(super) lines: Vec<String>,
    pub(super) row_ranges: Vec<Option<PaneTextRowRange>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PaneTextRowRange {
    pub(super) start: u16,
    pub(super) end: u16,
}

impl PaneTextSnapshot {
    fn empty(area: Rect) -> Self {
        Self {
            area,
            lines: Vec::new(),
            row_ranges: Vec::new(),
        }
    }

    pub(super) fn row_range_at(&self, row: u16) -> Option<PaneTextRowRange> {
        if row < self.area.y || row >= self.area.bottom() {
            return None;
        }
        self.row_ranges
            .get(usize::from(row.saturating_sub(self.area.y)))
            .copied()
            .flatten()
    }
}

fn symbol_has_visible_text(symbol: &str) -> bool {
    symbol.chars().any(|ch| !ch.is_whitespace())
}

pub(super) fn capture_pane_text(buf: &Buffer, area: Rect) -> PaneTextSnapshot {
    if area.is_empty() {
        return PaneTextSnapshot::empty(area);
    }

    let mut lines = Vec::with_capacity(area.height as usize);
    let mut row_ranges = Vec::with_capacity(area.height as usize);
    for y in area.y..area.bottom() {
        let mut line = String::new();
        let mut first_visible = None;
        let mut last_visible = None;
        for x in area.x..area.right() {
            let symbol = buf[(x, y)].symbol();
            if symbol_has_visible_text(symbol) {
                first_visible.get_or_insert(x);
                last_visible = Some(x);
            }
            line.push_str(symbol);
        }
        lines.push(line.trim_end().to_string());
        row_ranges.push(
            first_visible
                .zip(last_visible)
                .map(|(start, end)| PaneTextRowRange { start, end }),
        );
    }
    PaneTextSnapshot {
        area,
        lines,
        row_ranges,
    }
}

fn ordered_selection_points(
    area: Rect,
    selection: MouseDragSelection,
) -> Option<((u16, u16), (u16, u16))> {
    if area.is_empty() {
        return None;
    }

    let clamp_x = |x: u16| x.clamp(area.x, area.right().saturating_sub(1));
    let clamp_y = |y: u16| y.clamp(area.y, area.bottom().saturating_sub(1));
    let start = (
        clamp_x(selection.start_column),
        clamp_y(selection.start_row),
    );
    let end = (clamp_x(selection.end_column), clamp_y(selection.end_row));
    if (start.1, start.0) <= (end.1, end.0) {
        Some((start, end))
    } else {
        Some((end, start))
    }
}

pub(super) fn selected_snapshot_cells(
    snapshot: &PaneTextSnapshot,
    selection: MouseDragSelection,
) -> Vec<(u16, u16)> {
    let Some(((start_x, start_y), (end_x, end_y))) =
        ordered_selection_points(snapshot.area, selection)
    else {
        return Vec::new();
    };

    let mut cells = Vec::new();
    for y in start_y..=end_y {
        let Some(range) = snapshot.row_range_at(y) else {
            continue;
        };
        let row_start = if y == start_y {
            start_x
        } else {
            snapshot.area.x
        };
        let row_end = if y == end_y {
            end_x
        } else {
            snapshot.area.right().saturating_sub(1)
        };
        let row_start = row_start.max(range.start);
        let row_end = row_end.min(range.end);
        if row_start > row_end {
            continue;
        }
        for x in row_start..=row_end {
            cells.push((x, y));
        }
    }
    cells
}

fn extract_line_range(line: &str, start: usize, end_inclusive: usize) -> String {
    let width = end_inclusive.saturating_sub(start).saturating_add(1);
    line.chars().skip(start).take(width).collect()
}

pub(super) fn extract_pane_selection(
    snapshot: &PaneTextSnapshot,
    selection: MouseDragSelection,
) -> String {
    let Some(((start_x, start_y), (end_x, end_y))) =
        ordered_selection_points(snapshot.area, selection)
    else {
        return String::new();
    };

    let start_row = usize::from(start_y.saturating_sub(snapshot.area.y));
    let end_row = usize::from(end_y.saturating_sub(snapshot.area.y));
    let mut selected = Vec::new();
    for row in start_row..=end_row {
        let Some(line) = snapshot.lines.get(row) else {
            continue;
        };
        let absolute_y = snapshot.area.y.saturating_add(row as u16);
        let Some(range) = snapshot.row_range_at(absolute_y) else {
            selected.push(String::new());
            continue;
        };
        let row_start = if row == start_row {
            start_x
        } else {
            snapshot.area.x
        }
        .max(range.start);
        let row_end = if row == end_row {
            end_x
        } else {
            snapshot.area.right().saturating_sub(1)
        }
        .min(range.end);
        if row_start > row_end {
            continue;
        }
        let row_start = usize::from(row_start.saturating_sub(snapshot.area.x));
        let row_end = usize::from(row_end.saturating_sub(snapshot.area.x));
        selected.push(
            extract_line_range(line, row_start, row_end)
                .trim_end()
                .to_string(),
        );
    }

    if selection.mode == MouseSelectionMode::FullPane {
        extract_full_pane_content(selected)
    } else {
        selected.join("\n").trim_end().to_string()
    }
}

fn is_horizontal_border_line(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty()
        && line.chars().all(|ch| {
            matches!(
                ch,
                '─' | '━'
                    | '═'
                    | '┄'
                    | '┅'
                    | '┈'
                    | '┉'
                    | '╭'
                    | '╮'
                    | '╰'
                    | '╯'
                    | '┌'
                    | '┐'
                    | '└'
                    | '┘'
                    | '╔'
                    | '╗'
                    | '╚'
                    | '╝'
            )
        })
}

fn strip_outer_vertical_borders(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    let last = chars.next_back()?;
    let is_vertical = |ch| matches!(ch, '│' | '┃' | '║' | '┆' | '┇' | '┊' | '┋');
    (is_vertical(first) && is_vertical(last)).then(|| chars.as_str().trim_end().to_string())
}

fn dedent_box(lines: &mut [String]) {
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
        .min()
        .unwrap_or(0);
    for line in lines {
        *line = line.chars().skip(indent).collect::<String>();
    }
}

fn flush_box(output: &mut Vec<String>, boxed: &mut Vec<String>) {
    dedent_box(boxed);
    output.append(boxed);
    if output.last().is_some_and(|line| !line.is_empty()) {
        output.push(String::new());
    }
}

fn extract_full_pane_content(lines: Vec<String>) -> String {
    let mut output = Vec::new();
    let mut boxed = Vec::new();
    let mut inside_box = false;

    for line in lines {
        if is_horizontal_border_line(&line) {
            if inside_box {
                flush_box(&mut output, &mut boxed);
            }
            inside_box = !inside_box;
            continue;
        }
        if inside_box {
            if let Some(content) = strip_outer_vertical_borders(&line) {
                boxed.push(content);
                continue;
            }
            flush_box(&mut output, &mut boxed);
            inside_box = false;
        }
        output.push(line.trim_end().to_string());
    }
    if !boxed.is_empty() {
        flush_box(&mut output, &mut boxed);
    }

    let mut normalized = Vec::new();
    for line in output {
        if !line.is_empty()
            || normalized
                .last()
                .is_some_and(|line: &String| !line.is_empty())
        {
            normalized.push(line);
        }
    }
    while normalized.last().is_some_and(String::is_empty) {
        normalized.pop();
    }
    normalized.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(lines: &[&str]) -> PaneTextSnapshot {
        let width = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let area = Rect::new(0, 0, width, lines.len() as u16);
        PaneTextSnapshot {
            area,
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            row_ranges: lines
                .iter()
                .map(|line| {
                    let start = line.chars().position(|ch| !ch.is_whitespace())? as u16;
                    let end = line.chars().rposition(|ch| !ch.is_whitespace())? as u16;
                    Some(PaneTextRowRange { start, end })
                })
                .collect(),
        }
    }

    fn selection(snapshot: &PaneTextSnapshot, mode: MouseSelectionMode) -> MouseDragSelection {
        MouseDragSelection {
            pane: MousePane::Chat,
            mode,
            start_column: snapshot.area.x,
            start_row: snapshot.area.y,
            end_column: snapshot.area.right().saturating_sub(1),
            end_row: snapshot.area.bottom().saturating_sub(1),
        }
    }

    #[test]
    fn full_pane_copy_removes_tui_card_borders_and_layout_indent() {
        let snapshot = snapshot(&[
            "  ╭──────────╮",
            "  │  hello   │",
            "  │    world │",
            "  ╰──────────╯",
            "",
            "╭──────╮",
            "│ next │",
            "╰──────╯",
        ]);

        assert_eq!(
            extract_pane_selection(
                &snapshot,
                selection(&snapshot, MouseSelectionMode::FullPane)
            ),
            "hello\n  world\n\nnext"
        );
    }

    #[test]
    fn range_copy_preserves_rendered_border_characters() {
        let snapshot = snapshot(&["╭──╮", "│ok│", "╰──╯"]);

        assert_eq!(
            extract_pane_selection(&snapshot, selection(&snapshot, MouseSelectionMode::Range)),
            "╭──╮\n│ok│\n╰──╯"
        );
    }

    #[test]
    fn full_pane_copy_preserves_nested_box_drawing_content() {
        let snapshot = snapshot(&[
            "╭────────╮",
            "│  ┌──┐  │",
            "│  │ok│  │",
            "│  └──┘  │",
            "╰────────╯",
        ]);

        assert_eq!(
            extract_pane_selection(
                &snapshot,
                selection(&snapshot, MouseSelectionMode::FullPane)
            ),
            "┌──┐\n│ok│\n└──┘"
        );
    }

    #[test]
    fn active_selection_blocks_only_its_own_pane_scroll() {
        let selection = MouseDragSelection {
            pane: MousePane::Chat,
            mode: MouseSelectionMode::Range,
            start_column: 2,
            start_row: 3,
            end_column: 8,
            end_row: 5,
        };

        assert!(selection_blocks_pane_scroll(
            Some(selection),
            None,
            MousePane::Chat
        ));
        assert!(!selection_blocks_pane_scroll(
            Some(selection),
            None,
            MousePane::WorkspaceList
        ));
    }
}
