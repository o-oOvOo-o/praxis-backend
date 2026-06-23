use super::*;

pub(super) fn render_change(
    change: &FileChange,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    lang: Option<&str>,
) {
    let style_context = current_diff_render_style_context();
    match change {
        FileChange::Add { content } => {
            // Pre-highlight the entire file content as a whole.
            let syntax_lines = lang.and_then(|l| highlight_code_to_styled_spans(content, l));
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                let syn = syntax_lines.as_ref().and_then(|sl| sl.get(i));
                if let Some(spans) = syn {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Insert,
                        raw,
                        width,
                        line_number_width,
                        Some(spans),
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                } else {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Insert,
                        raw,
                        width,
                        line_number_width,
                        /*syntax_spans*/ None,
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                }
            }
        }
        FileChange::Delete { content } => {
            let syntax_lines = lang.and_then(|l| highlight_code_to_styled_spans(content, l));
            let line_number_width = line_number_width(content.lines().count());
            for (i, raw) in content.lines().enumerate() {
                let syn = syntax_lines.as_ref().and_then(|sl| sl.get(i));
                if let Some(spans) = syn {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Delete,
                        raw,
                        width,
                        line_number_width,
                        Some(spans),
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                } else {
                    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
                        i + 1,
                        DiffLineType::Delete,
                        raw,
                        width,
                        line_number_width,
                        /*syntax_spans*/ None,
                        style_context.theme,
                        style_context.color_level,
                        style_context.diff_backgrounds,
                    ));
                }
            }
        }
        FileChange::Update { unified_diff, .. } => {
            if let Ok(patch) = diffy::Patch::from_str(unified_diff) {
                let mut max_line_number = 0;
                let mut total_diff_bytes: usize = 0;
                let mut total_diff_lines: usize = 0;
                for h in patch.hunks() {
                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for l in h.lines() {
                        let text = match l {
                            diffy::Line::Insert(t)
                            | diffy::Line::Delete(t)
                            | diffy::Line::Context(t) => t,
                        };
                        total_diff_bytes += text.len();
                        total_diff_lines += 1;
                        match l {
                            diffy::Line::Insert(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                new_ln += 1;
                            }
                            diffy::Line::Delete(_) => {
                                max_line_number = max_line_number.max(old_ln);
                                old_ln += 1;
                            }
                            diffy::Line::Context(_) => {
                                max_line_number = max_line_number.max(new_ln);
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }

                // Skip per-line syntax highlighting when the patch is too
                // large — avoids thousands of parser initializations that
                // would stall rendering on big diffs.
                let diff_lang = if exceeds_highlight_limits(total_diff_bytes, total_diff_lines) {
                    None
                } else {
                    lang
                };

                let line_number_width = line_number_width(max_line_number);
                let mut is_first_hunk = true;
                for h in patch.hunks() {
                    if !is_first_hunk {
                        let spacer = format!("{:width$} ", "", width = line_number_width.max(1));
                        let spacer_span = RtSpan::styled(
                            spacer,
                            style_gutter_for(
                                DiffLineType::Context,
                                style_context.theme,
                                style_context.color_level,
                            ),
                        );
                        out.push(RtLine::from(vec![spacer_span, "⋮".dim()]));
                    }
                    is_first_hunk = false;

                    // Highlight each hunk as a single block so syntect parser
                    // state is preserved across consecutive lines.
                    let hunk_syntax_lines = diff_lang.and_then(|language| {
                        let hunk_text: String = h
                            .lines()
                            .iter()
                            .map(|line| match line {
                                diffy::Line::Insert(text)
                                | diffy::Line::Delete(text)
                                | diffy::Line::Context(text) => *text,
                            })
                            .collect();
                        let syntax_lines = highlight_code_to_styled_spans(&hunk_text, language)?;
                        (syntax_lines.len() == h.lines().len()).then_some(syntax_lines)
                    });

                    let mut old_ln = h.old_range().start();
                    let mut new_ln = h.new_range().start();
                    for (line_idx, l) in h.lines().iter().enumerate() {
                        let syntax_spans = hunk_syntax_lines
                            .as_ref()
                            .and_then(|syntax_lines| syntax_lines.get(line_idx));
                        match l {
                            diffy::Line::Insert(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Insert,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Insert,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                new_ln += 1;
                            }
                            diffy::Line::Delete(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            old_ln,
                                            DiffLineType::Delete,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            old_ln,
                                            DiffLineType::Delete,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                old_ln += 1;
                            }
                            diffy::Line::Context(text) => {
                                let s = text.trim_end_matches('\n');
                                if let Some(syn) = syntax_spans {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Context,
                                            s,
                                            width,
                                            line_number_width,
                                            Some(syn),
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                } else {
                                    out.extend(
                                        push_wrapped_diff_line_inner_with_theme_and_color_level(
                                            new_ln,
                                            DiffLineType::Context,
                                            s,
                                            width,
                                            line_number_width,
                                            /*syntax_spans*/ None,
                                            style_context.theme,
                                            style_context.color_level,
                                            style_context.diff_backgrounds,
                                        ),
                                    );
                                }
                                old_ln += 1;
                                new_ln += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}
