use super::*;

pub(super) struct CachedRenderable {
    renderable: Box<dyn Renderable>,
    height: std::cell::Cell<Option<u16>>,
    last_width: std::cell::Cell<Option<u16>>,
}

impl CachedRenderable {
    pub(super) fn new(renderable: impl Into<Box<dyn Renderable>>) -> Self {
        Self {
            renderable: renderable.into(),
            height: std::cell::Cell::new(None),
            last_width: std::cell::Cell::new(None),
        }
    }
}

impl Renderable for CachedRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.renderable.render(area, buf);
    }
    fn render_window(&self, area: Rect, buf: &mut Buffer, scroll_offset: u16) {
        self.renderable.render_window(area, buf, scroll_offset);
    }
    fn desired_height(&self, width: u16) -> u16 {
        if self.last_width.get() != Some(width) {
            let height = self.renderable.desired_height(width);
            self.height.set(Some(height));
            self.last_width.set(Some(width));
        }
        self.height.get().unwrap_or(0)
    }
}

pub(super) struct CellRenderable {
    pub(super) cell: Arc<dyn HistoryCell>,
    pub(super) style: Style,
    pub(super) search_match: Option<SearchChunkHighlight>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchMatchHighlight {
    chunk_index: usize,
    query: String,
    line_index: usize,
    match_index_in_line: usize,
}

impl SearchMatchHighlight {
    fn from_status(status: &TranscriptSearchStatus) -> Option<Self> {
        let target = status.current_target?;
        (!status.query.is_empty()).then(|| Self {
            chunk_index: target.chunk_index,
            query: status.query.clone(),
            line_index: target.line_index,
            match_index_in_line: target.match_index_in_line,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchChunkHighlight {
    query: String,
    current_match: Option<SearchMatchHighlight>,
}

impl SearchChunkHighlight {
    pub(super) fn from_status(
        status: &TranscriptSearchStatus,
        chunk_index: usize,
    ) -> Option<Self> {
        (!status.query.is_empty()).then(|| Self {
            query: status.query.clone(),
            current_match: SearchMatchHighlight::from_status(status)
                .filter(|search_match| search_match.chunk_index == chunk_index),
        })
    }
}

pub(super) struct TranscriptLinesRenderable {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) search_match: Option<SearchChunkHighlight>,
}

pub(super) fn stylize_transcript_lines(
    lines: Vec<Line<'static>>,
    base_style: Style,
    search_match: Option<&SearchChunkHighlight>,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(line_index, line)| {
            let line = materialize_line_style(line, base_style);
            let current_match_index = search_match.and_then(|search_match| {
                search_match
                    .current_match
                    .as_ref()
                    .filter(|current_match| current_match.line_index == line_index)
                    .map(|current_match| current_match.match_index_in_line)
            });
            highlight_line_matches(
                line,
                search_match.map(|search_match| search_match.query.as_str()),
                current_match_index,
            )
        })
        .collect()
}

pub(super) fn materialize_line_style(line: Line<'static>, base_style: Style) -> Line<'static> {
    let Line {
        spans,
        style,
        alignment,
    } = line;
    let merged_line_style = style.patch(base_style);
    if spans.is_empty() {
        return Line {
            spans,
            style: merged_line_style,
            alignment,
        };
    }

    let spans = spans
        .into_iter()
        .map(|span| Span {
            style: span.style.patch(merged_line_style),
            content: span.content,
        })
        .collect();

    Line {
        spans,
        style: Style::default(),
        alignment,
    }
}

pub(super) fn highlight_line_matches(
    line: Line<'static>,
    query: Option<&str>,
    current_match_index: Option<usize>,
) -> Line<'static> {
    let Some(query) = query else {
        return line;
    };
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let target_ranges = line_search_match_ranges(&text, query);
    if target_ranges.is_empty() {
        return line;
    }

    let Line {
        spans,
        style,
        alignment,
    } = line;
    let mut out = Vec::with_capacity(spans.len().saturating_add(target_ranges.len() * 2));
    let mut cursor = 0usize;

    for span in spans {
        let content = span.content.into_owned();
        let span_start = cursor;
        let span_end = span_start + content.len();
        cursor = span_end;

        let intersecting_ranges = target_ranges
            .iter()
            .enumerate()
            .filter(|(_, target_range)| {
                target_range.start < span_end && target_range.end > span_start
            })
            .collect::<Vec<_>>();
        if intersecting_ranges.is_empty() {
            out.push(Span::styled(content, span.style));
            continue;
        }

        let mut local_cursor = 0usize;
        for (range_index, target_range) in intersecting_ranges {
            let local_start = target_range.start.saturating_sub(span_start);
            let local_end = target_range.end.min(span_end).saturating_sub(span_start);

            if local_start > local_cursor {
                out.push(Span::styled(
                    content[local_cursor..local_start].to_string(),
                    span.style,
                ));
            }
            if local_end > local_start {
                let highlight_style = if current_match_index == Some(range_index) {
                    search_highlight_overlay(span.style).add_modifier(Modifier::REVERSED)
                } else {
                    search_highlight_overlay(span.style)
                };
                out.push(Span::styled(
                    content[local_start..local_end].to_string(),
                    highlight_style,
                ));
            }
            local_cursor = local_end;
        }
        if local_cursor < content.len() {
            out.push(Span::styled(
                content[local_cursor..].to_string(),
                span.style,
            ));
        }
    }

    Line {
        spans: out,
        style,
        alignment,
    }
}

pub(super) fn line_search_match_ranges(line: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let normalized_query = query.to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let mut normalized_line = String::new();
    let mut normalized_byte_to_char_index = Vec::new();
    let mut original_char_ranges = Vec::new();
    for (char_index, (orig_start, ch)) in line.char_indices().enumerate() {
        let orig_end = orig_start + ch.len_utf8();
        original_char_ranges.push(orig_start..orig_end);
        let lower = ch.to_lowercase().collect::<String>();
        normalized_byte_to_char_index.extend(std::iter::repeat(char_index).take(lower.len()));
        normalized_line.push_str(&lower);
    }

    let mut ranges = Vec::new();
    let mut offset = 0usize;
    while let Some(relative) = normalized_line[offset..].find(&normalized_query) {
        let normalized_start = offset + relative;
        let normalized_end = normalized_start + normalized_query.len();
        let start_char_index = normalized_byte_to_char_index[normalized_start];
        let end_char_index = normalized_byte_to_char_index[normalized_end - 1];
        ranges.push(
            original_char_ranges[start_char_index].start..original_char_ranges[end_char_index].end,
        );
        offset = normalized_end;
    }

    ranges
}

impl Renderable for CellRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = stylize_transcript_lines(
            self.cell.transcript_lines(area.width),
            self.style,
            self.search_match.as_ref(),
        );
        let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        p.render(area, buf);
    }

    fn render_window(&self, area: Rect, buf: &mut Buffer, scroll_offset: u16) {
        let lines = stylize_transcript_lines(
            self.cell.transcript_lines(area.width),
            self.style,
            self.search_match.as_ref(),
        );
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
            .render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.cell.desired_transcript_height(width)
    }
}

impl Renderable for TranscriptLinesRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = stylize_transcript_lines(
            self.lines.clone(),
            Style::default(),
            self.search_match.as_ref(),
        );
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_window(&self, area: Rect, buf: &mut Buffer, scroll_offset: u16) {
        let lines = stylize_transcript_lines(
            self.lines.clone(),
            Style::default(),
            self.search_match.as_ref(),
        );
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
            .render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        if let [line] = &self.lines[..]
            && line
                .spans
                .iter()
                .all(|span| span.content.chars().all(char::is_whitespace))
        {
            return 1;
        }

        Paragraph::new(Text::from(self.lines.clone()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }
}
