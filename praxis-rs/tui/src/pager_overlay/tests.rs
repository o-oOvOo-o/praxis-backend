use super::pager_view::*;
use super::renderables::*;
use super::transcript::*;
use super::*;
use insta::assert_snapshot;
use praxis_protocol::protocol::ExecCommandSource;
use praxis_protocol::protocol::ReviewDecision;
use pretty_assertions::assert_eq;
use std::cell::Cell as CounterCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::exec_cell::CommandOutput;
use crate::history_cell;
use crate::history_cell::HistoryCell;
use crate::history_cell::new_patch_event;
use praxis_protocol::parse_command::ParsedCommand;
use praxis_protocol::protocol::FileChange;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::Text;

#[derive(Debug)]
struct TestCell {
    lines: Vec<Line<'static>>,
}

impl crate::history_cell::HistoryCell for TestCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines.clone()
    }

    fn transcript_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines.clone()
    }
}

fn paragraph_block(label: &str, lines: usize) -> Box<dyn Renderable> {
    let text = Text::from(
        (0..lines)
            .map(|i| Line::from(format!("{label}{i}")))
            .collect::<Vec<_>>(),
    );
    Box::new(Paragraph::new(text)) as Box<dyn Renderable>
}

#[derive(Default)]
struct RenderCounters {
    desired: CounterCell<usize>,
    rendered: CounterCell<usize>,
}

struct CountingRenderable {
    counters: Rc<RenderCounters>,
    height: u16,
}

impl Renderable for CountingRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.counters
            .rendered
            .set(self.counters.rendered.get().saturating_add(1));
        if area.width > 0 && area.height > 0 {
            buf[(area.x, area.y)] = Cell::from('x');
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        self.counters
            .desired
            .set(self.counters.desired.get().saturating_add(1));
        self.height
    }
}

#[test]
fn edit_prev_hint_is_visible() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("hello")],
    })]);

    // Render into a wide buffer so the footer hints aren't truncated.
    let area = Rect::new(0, 0, 120, 10);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let s = buffer_to_text(&buf, area);
    assert!(
        s.contains("edit prev"),
        "expected 'edit prev' hint in overlay footer, got: {s:?}"
    );
}

#[test]
fn edit_next_hint_is_visible_when_highlighted() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("hello")],
    })]);
    overlay.set_highlight_cell(Some(0));

    // Render into a wide buffer so the footer hints aren't truncated.
    let area = Rect::new(0, 0, 120, 10);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let s = buffer_to_text(&buf, area);
    assert!(
        s.contains("edit next"),
        "expected 'edit next' hint in overlay footer, got: {s:?}"
    );
}

#[test]
fn transcript_overlay_snapshot_basic() {
    // Prepare a transcript overlay with a few lines
    let mut overlay = TranscriptOverlay::new(vec![
        Arc::new(TestCell {
            lines: vec![Line::from("alpha")],
        }),
        Arc::new(TestCell {
            lines: vec![Line::from("beta")],
        }),
        Arc::new(TestCell {
            lines: vec![Line::from("gamma")],
        }),
    ]);
    let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");
    assert_snapshot!(term.backend());
}

#[test]
fn transcript_overlay_renders_live_tail() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("alpha")],
    })]);
    overlay.sync_live_tail(
        /*width*/ 40,
        Some(ActiveCellTranscriptKey {
            revision: 1,
            is_stream_continuation: false,
            animation_tick: None,
            presentation_revision: 0,
        }),
        |_| Some(vec![Line::from("tail")]),
    );

    let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");
    assert_snapshot!(term.backend());
}

#[test]
fn transcript_overlay_sync_live_tail_is_noop_for_identical_key() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("alpha")],
    })]);

    let calls = std::cell::Cell::new(0usize);
    let key = ActiveCellTranscriptKey {
        revision: 1,
        is_stream_continuation: false,
        animation_tick: None,
        presentation_revision: 0,
    };

    overlay.sync_live_tail(/*width*/ 40, Some(key), |_| {
        calls.set(calls.get() + 1);
        Some(vec![Line::from("tail")])
    });
    overlay.sync_live_tail(/*width*/ 40, Some(key), |_| {
        calls.set(calls.get() + 1);
        Some(vec![Line::from("tail2")])
    });

    assert_eq!(calls.get(), 1);
}

#[test]
fn transcript_overlay_renders_search_status_line() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("alpha")],
    })]);
    overlay.set_search_state(Some(TranscriptSearchOverlayState {
        status: TranscriptSearchStatus {
            query: "alpha".to_string(),
            result_count: 1,
            current_ordinal: Some(1),
            current_target: None,
            wrapped: false,
        },
        current_chunk: Some(0),
        highlight_cell: Some(0),
    }));

    let area = Rect::new(0, 0, 120, 10);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let s = buffer_to_text(&buf, area);
    assert!(
        s.contains("Search: alpha  1/1"),
        "expected transcript search status in overlay footer, got: {s:?}"
    );
}

#[test]
fn transcript_overlay_highlights_current_search_match_in_committed_cell() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from(vec![
            Span::raw("alpha "),
            Span::raw("be"),
            Span::raw("ta gamma"),
        ])],
    })]);
    overlay.set_search_state(Some(TranscriptSearchOverlayState {
        status: TranscriptSearchStatus {
            query: "beta".to_string(),
            result_count: 1,
            current_ordinal: Some(1),
            current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                chunk_index: 0,
                cell_index: Some(0),
                line_index: 0,
                match_index_in_line: 0,
                is_live_tail: false,
            }),
            wrapped: false,
        },
        current_chunk: Some(0),
        highlight_cell: None,
    }));

    let area = Rect::new(0, 0, 80, 10);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let content_area = overlay.view.content_area(Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(3),
    ));
    assert_text_has_search_highlight(&buf, content_area, "beta");
}

#[test]
fn transcript_overlay_highlights_current_search_match_in_live_tail() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("alpha")],
    })]);
    overlay.sync_live_tail(
        /*width*/ 80,
        Some(ActiveCellTranscriptKey {
            revision: 1,
            is_stream_continuation: false,
            animation_tick: None,
            presentation_revision: 0,
        }),
        |_| Some(vec![Line::from("tail beta")]),
    );
    overlay.set_search_state(Some(TranscriptSearchOverlayState {
        status: TranscriptSearchStatus {
            query: "beta".to_string(),
            result_count: 1,
            current_ordinal: Some(1),
            current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                chunk_index: 1,
                cell_index: None,
                line_index: 0,
                match_index_in_line: 0,
                is_live_tail: true,
            }),
            wrapped: false,
        },
        current_chunk: Some(1),
        highlight_cell: None,
    }));

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let content_area = overlay.view.content_area(Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(3),
    ));
    assert_text_has_search_highlight(&buf, content_area, "beta");
}

#[test]
fn transcript_overlay_highlights_all_search_matches_and_emphasizes_current_one() {
    let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
        lines: vec![Line::from("beta beta")],
    })]);
    overlay.set_search_state(Some(TranscriptSearchOverlayState {
        status: TranscriptSearchStatus {
            query: "beta".to_string(),
            result_count: 2,
            current_ordinal: Some(2),
            current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                chunk_index: 0,
                cell_index: Some(0),
                line_index: 0,
                match_index_in_line: 1,
                is_live_tail: false,
            }),
            wrapped: false,
        },
        current_chunk: Some(0),
        highlight_cell: None,
    }));

    let area = Rect::new(0, 0, 80, 10);
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let content_area = overlay.view.content_area(Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(3),
    ));
    let expected_bg = crate::style::search_highlight_style()
        .bg
        .expect("search highlight style should set a background");
    let (y, row) = rendered_row_containing(&buf, content_area, "beta beta")
        .expect("expected rendered row with repeated match");
    let first_col = row.find("beta").expect("first beta");
    let second_col = row[first_col + 4..]
        .find("beta")
        .map(|col| first_col + 4 + col)
        .expect("second beta");

    for offset in 0..4u16 {
        assert_eq!(
            buf[(content_area.x + first_col as u16 + offset, y)].bg,
            expected_bg
        );
        assert_eq!(
            buf[(content_area.x + second_col as u16 + offset, y)].bg,
            expected_bg
        );
    }
    assert!(
        buf[(content_area.x + second_col as u16, y)]
            .modifier
            .contains(Modifier::REVERSED),
        "expected current search match to carry extra emphasis"
    );
    assert!(
        !buf[(content_area.x + first_col as u16, y)]
            .modifier
            .contains(Modifier::REVERSED),
        "expected non-current matches to omit current-match emphasis"
    );
}

fn buffer_to_text(buf: &Buffer, area: Rect) -> String {
    let mut out = String::new();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let symbol = buf[(x, y)].symbol();
            if symbol.is_empty() {
                out.push(' ');
            } else {
                out.push(symbol.chars().next().unwrap_or(' '));
            }
        }
        // Trim trailing spaces for stability.
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
    out
}

fn assert_text_has_search_highlight(buf: &Buffer, area: Rect, needle: &str) {
    let expected_bg = crate::style::search_highlight_style()
        .bg
        .expect("search highlight style should set a background");

    for y in area.y..area.bottom() {
        let mut row = String::new();
        for x in area.x..area.right() {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        if let Some(col) = row.find(needle) {
            for offset in 0..needle.len() as u16 {
                assert_eq!(
                    buf[(area.x + col as u16 + offset, y)].bg,
                    expected_bg,
                    "expected highlighted background for {needle:?} at ({}, {})",
                    area.x + col as u16 + offset,
                    y
                );
            }
            return;
        }
    }

    panic!(
        "did not find {needle:?} in rendered area: {:?}",
        buffer_to_text(buf, area)
    );
}

fn rendered_row_containing(buf: &Buffer, area: Rect, needle: &str) -> Option<(u16, String)> {
    for y in area.y..area.bottom() {
        let mut row = String::new();
        for x in area.x..area.right() {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        if row.contains(needle) {
            return Some((y, row));
        }
    }
    None
}

#[test]
fn transcript_overlay_apply_patch_scroll_vt100_clears_previous_page() {
    let cwd = PathBuf::from("/repo");
    let mut cells: Vec<Arc<dyn HistoryCell>> = Vec::new();

    let mut approval_changes = HashMap::new();
    approval_changes.insert(
        PathBuf::from("foo.txt"),
        FileChange::Add {
            content: "hello\nworld\n".to_string(),
        },
    );
    let approval_cell: Arc<dyn HistoryCell> = Arc::new(new_patch_event(approval_changes, &cwd));
    cells.push(approval_cell);

    let mut apply_changes = HashMap::new();
    apply_changes.insert(
        PathBuf::from("foo.txt"),
        FileChange::Add {
            content: "hello\nworld\n".to_string(),
        },
    );
    let apply_begin_cell: Arc<dyn HistoryCell> = Arc::new(new_patch_event(apply_changes, &cwd));
    cells.push(apply_begin_cell);

    let apply_end_cell: Arc<dyn HistoryCell> = history_cell::new_approval_decision_cell(
        vec!["ls".into()],
        ReviewDecision::Approved,
        history_cell::ApprovalDecisionActor::User,
    )
    .into();
    cells.push(apply_end_cell);

    let mut exec_cell = crate::exec_cell::new_active_exec_command(
        "exec-1".into(),
        vec!["bash".into(), "-lc".into(), "ls".into()],
        vec![ParsedCommand::Unknown { cmd: "ls".into() }],
        ExecCommandSource::Agent,
        /*interaction_input*/ None,
        /*animations_enabled*/ true,
    );
    exec_cell.complete_call(
        "exec-1",
        CommandOutput {
            exit_code: 0,
            aggregated_output: "src\nREADME.md\n".into(),
            formatted_output: "src\nREADME.md\n".into(),
        },
        Duration::from_millis(420),
    );
    let exec_cell: Arc<dyn HistoryCell> = Arc::new(exec_cell);
    cells.push(exec_cell);

    let mut overlay = TranscriptOverlay::new(cells);
    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);

    overlay.render(area, &mut buf);
    overlay.view.scroll_offset = 0;
    overlay.render(area, &mut buf);

    let snapshot = buffer_to_text(&buf, area);
    assert_snapshot!("transcript_overlay_apply_patch_scroll_vt100", snapshot);
}

#[test]
fn transcript_overlay_keeps_scroll_pinned_at_bottom() {
    let mut overlay = TranscriptOverlay::new(
        (0..20)
            .map(|i| {
                Arc::new(TestCell {
                    lines: vec![Line::from(format!("line{i}"))],
                }) as Arc<dyn HistoryCell>
            })
            .collect(),
    );
    let mut term = Terminal::new(TestBackend::new(40, 12)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");

    assert!(
        overlay.view.is_scrolled_to_bottom(),
        "expected initial render to leave view at bottom"
    );

    overlay.insert_cell(Arc::new(TestCell {
        lines: vec!["tail".into()],
    }));

    assert_eq!(overlay.view.scroll_offset, usize::MAX);
}

#[test]
fn transcript_overlay_preserves_manual_scroll_position() {
    let mut overlay = TranscriptOverlay::new(
        (0..20)
            .map(|i| {
                Arc::new(TestCell {
                    lines: vec![Line::from(format!("line{i}"))],
                }) as Arc<dyn HistoryCell>
            })
            .collect(),
    );
    let mut term = Terminal::new(TestBackend::new(40, 12)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");

    overlay.view.scroll_offset = 0;

    overlay.insert_cell(Arc::new(TestCell {
        lines: vec!["tail".into()],
    }));

    assert_eq!(overlay.view.scroll_offset, 0);
}

#[test]
fn static_overlay_snapshot_basic() {
    // Prepare a static overlay with a few lines and a title
    let mut overlay = StaticOverlay::with_title(
        vec!["one".into(), "two".into(), "three".into()],
        "S T A T I C".to_string(),
    );
    let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");
    assert_snapshot!(term.backend());
}

/// Render transcript overlay and return visible line numbers (`line-NN`) in order.
fn transcript_line_numbers(overlay: &mut TranscriptOverlay, area: Rect) -> Vec<usize> {
    let mut buf = Buffer::empty(area);
    overlay.render(area, &mut buf);

    let top_h = area.height.saturating_sub(3);
    let top = Rect::new(area.x, area.y, area.width, top_h);
    let content_area = overlay.view.content_area(top);

    let mut nums = Vec::new();
    for y in content_area.y..content_area.bottom() {
        let mut line = String::new();
        for x in content_area.x..content_area.right() {
            line.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        if let Some(n) = line
            .split_whitespace()
            .find_map(|w| w.strip_prefix("line-"))
            .and_then(|s| s.parse().ok())
        {
            nums.push(n);
        }
    }
    nums
}

#[test]
fn transcript_overlay_paging_is_continuous_and_round_trips() {
    let mut overlay = TranscriptOverlay::new(
        (0..50)
            .map(|i| {
                Arc::new(TestCell {
                    lines: vec![Line::from(format!("line-{i:02}"))],
                }) as Arc<dyn HistoryCell>
            })
            .collect(),
    );
    let area = Rect::new(0, 0, 40, 15);

    // Prime layout so last_content_height is populated and paging uses the real content height.
    let mut buf = Buffer::empty(area);
    overlay.view.scroll_offset = 0;
    overlay.render(area, &mut buf);
    let page_height = overlay.view.page_height(area);

    // Scenario 1: starting from the top, PageDown should show the next page of content.
    overlay.view.scroll_offset = 0;
    let page1 = transcript_line_numbers(&mut overlay, area);
    let page1_len = page1.len();
    let expected_page1: Vec<usize> = (0..page1_len).collect();
    assert_eq!(
        page1, expected_page1,
        "first page should start at line-00 and show a full page of content"
    );

    overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
    let page2 = transcript_line_numbers(&mut overlay, area);
    assert_eq!(
        page2.len(),
        page1_len,
        "second page should have the same number of visible lines as the first page"
    );
    let expected_page2_first = *page1.last().unwrap() + 1;
    assert_eq!(
        page2[0], expected_page2_first,
        "second page after PageDown should immediately follow the first page"
    );

    // Scenario 2: from an interior offset (start=3), PageDown then PageUp should round-trip.
    let interior_offset = 3usize;
    overlay.view.scroll_offset = interior_offset;
    let before = transcript_line_numbers(&mut overlay, area);
    overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
    let _ = transcript_line_numbers(&mut overlay, area);
    overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_sub(page_height);
    let after = transcript_line_numbers(&mut overlay, area);
    assert_eq!(
        before, after,
        "PageDown+PageUp from interior offset ({interior_offset}) should round-trip"
    );

    // Scenario 3: from the top of the second page, PageUp then PageDown should round-trip.
    overlay.view.scroll_offset = page_height;
    let before2 = transcript_line_numbers(&mut overlay, area);
    overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_sub(page_height);
    let _ = transcript_line_numbers(&mut overlay, area);
    overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
    let after2 = transcript_line_numbers(&mut overlay, area);
    assert_eq!(
        before2, after2,
        "PageUp+PageDown from the top of the second page should round-trip"
    );
}

#[test]
fn static_overlay_wraps_long_lines() {
    let mut overlay = StaticOverlay::with_title(
        vec![
            "a very long line that should wrap when rendered within a narrow pager overlay width"
                .into(),
        ],
        "S T A T I C".to_string(),
    );
    let mut term = Terminal::new(TestBackend::new(24, 8)).expect("term");
    term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
        .expect("draw");
    assert_snapshot!(term.backend());
}

#[test]
fn pager_view_content_height_counts_renderables() {
    let pv = PagerView::new(
        vec![
            paragraph_block("a", /*lines*/ 2),
            paragraph_block("b", /*lines*/ 3),
        ],
        "T".to_string(),
        /*scroll_offset*/ 0,
    );

    assert_eq!(pv.content_height(/*width*/ 80), 5);
}

#[test]
fn pager_view_reuses_layout_and_renders_visible_chunks_only() {
    let counters = (0..100)
        .map(|_| Rc::new(RenderCounters::default()))
        .collect::<Vec<_>>();
    let renderables = counters
        .iter()
        .map(|counters| {
            Box::new(CountingRenderable {
                counters: Rc::clone(counters),
                height: 1,
            }) as Box<dyn Renderable>
        })
        .collect::<Vec<_>>();
    let mut pv = PagerView::new(renderables, "T".to_string(), /*scroll_offset*/ 50);
    let area = Rect::new(0, 0, 20, 7);
    let mut buf = Buffer::empty(area);

    pv.render(area, &mut buf);
    assert_eq!(
        counters
            .iter()
            .map(|counters| counters.desired.get())
            .sum::<usize>(),
        100,
        "initial layout should compute each chunk height once"
    );

    for counters in &counters {
        counters.desired.set(0);
        counters.rendered.set(0);
    }
    pv.render(area, &mut buf);

    assert_eq!(
        counters
            .iter()
            .map(|counters| counters.desired.get())
            .sum::<usize>(),
        0,
        "same-width redraw should reuse the pager layout cache"
    );
    for idx in 0..100 {
        let expected = if (50..55).contains(&idx) { 1 } else { 0 };
        assert_eq!(
            counters[idx].rendered.get(),
            expected,
            "unexpected render count for chunk {idx}"
        );
    }
}

#[test]
fn pager_view_bottom_fast_path_measures_visible_suffix_only() {
    let counters = (0..100)
        .map(|_| Rc::new(RenderCounters::default()))
        .collect::<Vec<_>>();
    let renderables = counters
        .iter()
        .map(|counters| {
            Box::new(CountingRenderable {
                counters: Rc::clone(counters),
                height: 1,
            }) as Box<dyn Renderable>
        })
        .collect::<Vec<_>>();
    let mut pv = PagerView::new(renderables, "T".to_string(), usize::MAX);
    let area = Rect::new(0, 0, 20, 7);
    let mut buf = Buffer::empty(area);

    pv.render(area, &mut buf);

    assert_eq!(
        counters
            .iter()
            .map(|counters| counters.desired.get())
            .sum::<usize>(),
        5,
        "bottom-first render should only measure the visible suffix"
    );
    assert!(
        pv.max_scroll_for_known_layout().is_none(),
        "fast bottom render should not materialize full layout"
    );
    for idx in 0..100 {
        let expected = if (95..100).contains(&idx) { 1 } else { 0 };
        assert_eq!(
            counters[idx].rendered.get(),
            expected,
            "unexpected render count for chunk {idx}"
        );
    }
}

#[test]
fn pager_view_ensure_chunk_visible_scrolls_down_when_needed() {
    let mut pv = PagerView::new(
        vec![
            paragraph_block("a", /*lines*/ 1),
            paragraph_block("b", /*lines*/ 3),
            paragraph_block("c", /*lines*/ 3),
        ],
        "T".to_string(),
        /*scroll_offset*/ 0,
    );
    let area = Rect::new(0, 0, 20, 8);

    pv.scroll_offset = 0;
    let content_area = pv.content_area(area);
    pv.ensure_chunk_visible(/*idx*/ 2, content_area);

    let mut buf = Buffer::empty(area);
    pv.render(area, &mut buf);
    let rendered = buffer_to_text(&buf, area);

    assert!(
        rendered.contains("c0"),
        "expected chunk top in view: {rendered:?}"
    );
    assert!(
        rendered.contains("c1"),
        "expected chunk middle in view: {rendered:?}"
    );
    assert!(
        rendered.contains("c2"),
        "expected chunk bottom in view: {rendered:?}"
    );
}

#[test]
fn pager_view_ensure_chunk_visible_scrolls_up_when_needed() {
    let mut pv = PagerView::new(
        vec![
            paragraph_block("a", /*lines*/ 2),
            paragraph_block("b", /*lines*/ 3),
            paragraph_block("c", /*lines*/ 3),
        ],
        "T".to_string(),
        /*scroll_offset*/ 0,
    );
    let area = Rect::new(0, 0, 20, 3);

    pv.scroll_offset = 6;
    pv.ensure_chunk_visible(/*idx*/ 0, area);

    assert_eq!(pv.scroll_offset, 0);
}

#[test]
fn pager_view_is_scrolled_to_bottom_accounts_for_wrapped_height() {
    let mut pv = PagerView::new(
        vec![paragraph_block("a", /*lines*/ 10)],
        "T".to_string(),
        /*scroll_offset*/ 0,
    );
    let area = Rect::new(0, 0, 20, 8);
    let mut buf = Buffer::empty(area);

    pv.render(area, &mut buf);

    assert!(
        !pv.is_scrolled_to_bottom(),
        "expected view to report not at bottom when offset < max"
    );

    pv.scroll_offset = usize::MAX;
    pv.render(area, &mut buf);

    assert!(
        pv.is_scrolled_to_bottom(),
        "expected view to report at bottom after scrolling to end"
    );
}

#[test]
fn pager_view_scroll_up_from_bottom_uses_concrete_bottom_offset() {
    let mut pv = PagerView::new(
        (0..20)
            .map(|i| paragraph_block(&format!("line-{i:02}-"), /*lines*/ 1))
            .collect(),
        "T".to_string(),
        /*scroll_offset*/ 0,
    );
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);

    pv.render(area, &mut buf);

    let max_scroll = pv
        .max_scroll_for_known_layout()
        .expect("render should populate layout heights");
    pv.scroll_offset = usize::MAX;
    pv.scroll_up(3);

    assert_eq!(pv.scroll_offset, max_scroll.saturating_sub(3));
}

#[test]
fn pager_view_scroll_up_after_bottom_fast_materializes_layout() {
    let mut pv = PagerView::new(
        (0..20)
            .map(|i| paragraph_block(&format!("line-{i:02}-"), /*lines*/ 1))
            .collect(),
        "T".to_string(),
        usize::MAX,
    );
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);

    pv.render(area, &mut buf);
    assert!(
        pv.max_scroll_for_known_layout().is_none(),
        "initial bottom render should use the fast path"
    );

    pv.scroll_up(3);
    pv.render(area, &mut buf);

    let max_scroll = pv
        .max_scroll_for_known_layout()
        .expect("scrolling up should materialize the full layout");
    assert_eq!(pv.scroll_offset, max_scroll.saturating_sub(3));
}
