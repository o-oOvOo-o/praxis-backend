use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn push_wrapped_diff_line_inner_with_theme_and_color_level(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    syntax_spans: Option<&[RtSpan<'static>]>,
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Vec<RtLine<'static>> {
    let ln_str = line_number.to_string();

    // Reserve a fixed number of spaces (equal to the widest line number plus a
    // trailing spacer) so the sign column stays aligned across the diff block.
    let gutter_width = line_number_width.max(1);
    let prefix_cols = gutter_width + 1;

    let (sign_char, sign_style, content_style) = match kind {
        DiffLineType::Insert => (
            '+',
            style_sign_add(theme, color_level, diff_backgrounds),
            style_add(theme, color_level, diff_backgrounds),
        ),
        DiffLineType::Delete => (
            '-',
            style_sign_del(theme, color_level, diff_backgrounds),
            style_del(theme, color_level, diff_backgrounds),
        ),
        DiffLineType::Context => (' ', style_context(), style_context()),
    };

    let line_bg = style_line_bg_for(kind, diff_backgrounds);
    let gutter_style = style_gutter_for(kind, theme, color_level);

    // When we have syntax spans, compose them with the diff style for a richer
    // view. The sign character keeps the diff color; content gets syntax colors
    // with an overlay modifier for delete lines (dim).
    if let Some(syn_spans) = syntax_spans {
        let gutter = format!("{ln_str:>gutter_width$} ");
        let sign = format!("{sign_char}");
        let styled: Vec<RtSpan<'static>> = syn_spans
            .iter()
            .map(|sp| {
                let style = if matches!(kind, DiffLineType::Delete) {
                    sp.style.add_modifier(Modifier::DIM)
                } else {
                    sp.style
                };
                RtSpan::styled(sp.content.clone().into_owned(), style)
            })
            .collect();

        // Determine how many display columns remain for content after the
        // gutter and sign character.
        let available_content_cols = width.saturating_sub(prefix_cols + 1).max(1);

        // Wrap the styled content spans to fit within the available columns.
        let wrapped_chunks = wrap_styled_spans(&styled, available_content_cols);

        let mut lines: Vec<RtLine<'static>> = Vec::new();
        for (i, chunk) in wrapped_chunks.into_iter().enumerate() {
            let mut row_spans: Vec<RtSpan<'static>> = Vec::new();
            if i == 0 {
                // First line: gutter + sign + content
                row_spans.push(RtSpan::styled(gutter.clone(), gutter_style));
                row_spans.push(RtSpan::styled(sign.clone(), sign_style));
            } else {
                // Continuation: empty gutter + two-space indent (matches
                // the plain-text wrapping continuation style).
                let cont_gutter = format!("{:gutter_width$}  ", "");
                row_spans.push(RtSpan::styled(cont_gutter, gutter_style));
            }
            row_spans.extend(chunk);
            lines.push(RtLine::from(row_spans).style(line_bg));
        }
        return lines;
    }

    let available_content_cols = width.saturating_sub(prefix_cols + 1).max(1);
    let styled = vec![RtSpan::styled(text.to_string(), content_style)];
    let wrapped_chunks = wrap_styled_spans(&styled, available_content_cols);

    let mut lines: Vec<RtLine<'static>> = Vec::new();
    for (i, chunk) in wrapped_chunks.into_iter().enumerate() {
        let mut row_spans: Vec<RtSpan<'static>> = Vec::new();
        if i == 0 {
            let gutter = format!("{ln_str:>gutter_width$} ");
            let sign = format!("{sign_char}");
            row_spans.push(RtSpan::styled(gutter, gutter_style));
            row_spans.push(RtSpan::styled(sign, sign_style));
        } else {
            let cont_gutter = format!("{:gutter_width$}  ", "");
            row_spans.push(RtSpan::styled(cont_gutter, gutter_style));
        }
        row_spans.extend(chunk);
        lines.push(RtLine::from(row_spans).style(line_bg));
    }

    lines
}

/// Split styled spans into chunks that fit within `max_cols` display columns.
///
/// Returns one `Vec<RtSpan>` per output line.  Styles are preserved across
/// split boundaries so that wrapping never loses syntax coloring.
///
/// The algorithm walks characters using their Unicode display width (with tabs
/// expanded to [`TAB_WIDTH`] columns).  When a character would overflow the
/// current line, the accumulated text is flushed and a new line begins.  A
/// single character wider than the remaining space forces a line break *before*
/// the character so that progress is always made (avoiding infinite loops on
/// CJK characters or tabs at the end of a line).
fn wrap_styled_spans(spans: &[RtSpan<'static>], max_cols: usize) -> Vec<Vec<RtSpan<'static>>> {
    let mut result: Vec<Vec<RtSpan<'static>>> = Vec::new();
    let mut current_line: Vec<RtSpan<'static>> = Vec::new();
    let mut col: usize = 0;

    for span in spans {
        let style = span.style;
        let text = span.content.as_ref();
        let mut remaining = text;

        while !remaining.is_empty() {
            // Accumulate characters until we fill the line.
            let mut byte_end = 0;
            let mut chars_col = 0;

            for ch in remaining.chars() {
                // Tabs have no Unicode width; treat them as TAB_WIDTH columns.
                let w = ch.width().unwrap_or(if ch == '\t' { TAB_WIDTH } else { 0 });
                if col + chars_col + w > max_cols {
                    // Adding this character would exceed the line width.
                    // Break here; if this is the first character in `remaining`
                    // we will flush/start a new line in the `byte_end == 0`
                    // branch below before consuming it.
                    break;
                }
                byte_end += ch.len_utf8();
                chars_col += w;
            }

            if byte_end == 0 {
                // Single character wider than remaining space — force onto a
                // new line so we make progress.
                if !current_line.is_empty() {
                    result.push(std::mem::take(&mut current_line));
                }
                // Take at least one character to avoid an infinite loop.
                let Some(ch) = remaining.chars().next() else {
                    break;
                };
                let ch_len = ch.len_utf8();
                current_line.push(RtSpan::styled(remaining[..ch_len].to_string(), style));
                // Use fallback width 1 (not 0) so this branch always advances
                // even if `ch` has unknown/zero display width.
                col = ch.width().unwrap_or(if ch == '\t' { TAB_WIDTH } else { 1 });
                remaining = &remaining[ch_len..];
                continue;
            }

            let (chunk, rest) = remaining.split_at(byte_end);
            current_line.push(RtSpan::styled(chunk.to_string(), style));
            col += chars_col;
            remaining = rest;

            // If we exactly filled or exceeded the line, start a new one.
            // Do not gate on !remaining.is_empty() — the next span in the
            // outer loop may still have content that must start on a fresh line.
            if col >= max_cols {
                result.push(std::mem::take(&mut current_line));
                col = 0;
            }
        }
    }

    // Push the last line (always at least one, even if empty).
    if !current_line.is_empty() || result.is_empty() {
        result.push(current_line);
    }

    result
}

pub(crate) fn line_number_width(max_line_number: usize) -> usize {
    if max_line_number == 0 {
        1
    } else {
        max_line_number.to_string().len()
    }
}
