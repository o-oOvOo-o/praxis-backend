use super::*;

impl ChatComposer {
    /// Return true if either the slash-command popup or the file-search popup is active.
    pub(crate) fn popup_active(&self) -> bool {
        !matches!(self.active_popup, ActivePopup::None)
    }

    /// Handle key event when the slash-command popup is visible.
    pub(super) fn handle_key_event_with_slash_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        if self.handle_shortcut_overlay_key(&key_event) {
            return (InputResult::None, true);
        }
        if key_event.code == KeyCode::Esc {
            let next_mode = esc_hint_mode(self.footer_mode, self.is_task_running);
            if next_mode != self.footer_mode {
                self.footer_mode = next_mode;
                return (InputResult::None, true);
            }
        } else {
            self.footer_mode = reset_mode_after_activity(self.footer_mode);
        }
        let ActivePopup::Command(popup) = &mut self.active_popup else {
            unreachable!();
        };

        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_up();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Dismiss the slash popup; keep the current input untouched.
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // Ensure popup filtering/selection reflects the latest composer text
                // before applying completion.
                let first_line = self.textarea.text().lines().next().unwrap_or("");
                popup.on_composer_text_change(first_line.to_string());
                if let Some(sel) = popup.selected_item() {
                    let command_name = match sel {
                        CommandItem::Builtin(cmd) => {
                            if cmd == SlashCommand::Skills {
                                self.textarea.set_text_clearing_elements("");
                                return (InputResult::Command(cmd), true);
                            }
                            cmd.command().to_string()
                        }
                        CommandItem::Plugin(command) => command.name,
                    };
                    let starts_with_cmd = first_line
                        .trim_start()
                        .starts_with(&format!("/{command_name}"));
                    if !starts_with_cmd {
                        self.textarea
                            .set_text_clearing_elements(&format!("/{command_name} "));
                    }
                    if !self.textarea.text().is_empty() {
                        self.textarea.set_cursor(self.textarea.text().len());
                    }
                }
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Some(sel) = popup.selected_item() {
                    self.textarea.set_text_clearing_elements("");
                    return match sel {
                        CommandItem::Builtin(cmd) => (InputResult::Command(cmd), true),
                        CommandItem::Plugin(command) => (
                            InputResult::PluginCommand(PluginCommandInvocation {
                                plugin_id: command.plugin_id,
                                plugin_display_name: command.plugin_display_name,
                                name: command.name,
                                args: String::new(),
                            }),
                            true,
                        ),
                    };
                }
                // Fallback to default newline handling if no command selected.
                self.handle_key_event_without_popup(key_event)
            }
            input => self.handle_input_basic(input),
        }
    }

    #[inline]
    pub(super) fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
        let mut p = pos.min(text.len());
        if p < text.len() && !text.is_char_boundary(p) {
            p = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= p)
                .last()
                .unwrap_or(0);
        }
        p
    }

    /// Handle non-ASCII character input (often IME) while still supporting paste-burst detection.
    ///
    /// This handler exists because non-ASCII input often comes from IMEs, where characters can
    /// legitimately arrive in short bursts that should **not** be treated as paste.
    ///
    /// The key differences from the ASCII path:
    ///
    /// - We never hold the first character (`PasteBurst::on_plain_char_no_hold`), because holding a
    ///   non-ASCII char can feel like dropped input.
    /// - If a burst is detected, we may need to retroactively remove already-inserted text before
    ///   the cursor and move it into the paste buffer (see `PasteBurst::decide_begin_buffer`).
    ///
    /// Because this path mixes "insert immediately" with "maybe retro-grab later", it must clamp
    /// the cursor to a UTF-8 char boundary before slicing `textarea.text()`.
    #[inline]
    pub(super) fn handle_non_ascii_char(
        &mut self,
        input: KeyEvent,
        now: Instant,
    ) -> (InputResult, bool) {
        if self.disable_paste_burst {
            // When burst detection is disabled, treat IME/non-ASCII input as normal typing.
            // In particular, do not retro-capture or buffer already-inserted prefix text.
            self.textarea.input(input);
            let text_after = self.textarea.text();
            self.pending_pastes
                .retain(|(placeholder, _)| text_after.contains(placeholder));
            return (InputResult::None, true);
        }
        if let KeyEvent {
            code: KeyCode::Char(ch),
            ..
        } = input
        {
            if self.paste_burst.try_append_char_if_active(ch, now) {
                return (InputResult::None, true);
            }
            // Non-ASCII input often comes from IMEs and can arrive in quick bursts.
            // We do not want to hold the first char (flicker suppression) on this path, but we
            // still want to detect paste-like bursts. Before applying any non-ASCII input, flush
            // any existing burst buffer (including a pending first char from the ASCII path) so
            // we don't carry that transient state forward.
            if let Some(pasted) = self.paste_burst.flush_before_modified_input() {
                self.handle_paste(pasted);
            }
            if let Some(decision) = self.paste_burst.on_plain_char_no_hold(now) {
                match decision {
                    CharDecision::BufferAppend => {
                        self.paste_burst.append_char_to_buffer(ch, now);
                        return (InputResult::None, true);
                    }
                    CharDecision::BeginBuffer { retro_chars } => {
                        // For non-ASCII we inserted prior chars immediately, so if this turns out
                        // to be paste-like we need to retroactively grab & remove the already-
                        // inserted prefix from the textarea before buffering the burst.
                        let cur = self.textarea.cursor();
                        let txt = self.textarea.text();
                        let safe_cur = Self::clamp_to_char_boundary(txt, cur);
                        let before = &txt[..safe_cur];
                        if let Some(grab) =
                            self.paste_burst
                                .decide_begin_buffer(now, before, retro_chars as usize)
                        {
                            if !grab.grabbed.is_empty() {
                                self.textarea.replace_range(grab.start_byte..safe_cur, "");
                            }
                            // seed the paste burst buffer with everything (grabbed + new)
                            self.paste_burst.append_char_to_buffer(ch, now);
                            return (InputResult::None, true);
                        }
                        // If decide_begin_buffer opted not to start buffering,
                        // fall through to normal insertion below.
                    }
                    _ => unreachable!("on_plain_char_no_hold returned unexpected variant"),
                }
            }
        }
        if let Some(pasted) = self.paste_burst.flush_before_modified_input() {
            self.handle_paste(pasted);
        }
        self.textarea.input(input);

        let text_after = self.textarea.text();
        self.pending_pastes
            .retain(|(placeholder, _)| text_after.contains(placeholder));
        (InputResult::None, true)
    }

    /// Handle key events when file search popup is visible.
    pub(super) fn handle_key_event_with_file_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        if self.handle_shortcut_overlay_key(&key_event) {
            return (InputResult::None, true);
        }
        if key_event.code == KeyCode::Esc {
            let next_mode = esc_hint_mode(self.footer_mode, self.is_task_running);
            if next_mode != self.footer_mode {
                self.footer_mode = next_mode;
                return (InputResult::None, true);
            }
        } else {
            self.footer_mode = reset_mode_after_activity(self.footer_mode);
        }
        let ActivePopup::File(popup) = &mut self.active_popup else {
            unreachable!();
        };

        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_up();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Hide popup without modifying text, remember token to avoid immediate reopen.
                if let Some(tok) = Self::current_at_token(&self.textarea) {
                    self.dismissed_file_popup_token = Some(tok);
                }
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let Some(sel) = popup.selected_match() else {
                    self.active_popup = ActivePopup::None;
                    return if key_event.code == KeyCode::Enter {
                        self.handle_key_event_without_popup(key_event)
                    } else {
                        (InputResult::None, true)
                    };
                };

                let sel_path = sel.to_string_lossy().to_string();
                // If selected path looks like an image (png/jpeg), attach as image instead of inserting text.
                let is_image = Self::is_image_path(&sel_path);
                if is_image {
                    // Determine dimensions; if that fails fall back to normal path insertion.
                    let path_buf = PathBuf::from(&sel_path);
                    match image::image_dimensions(&path_buf) {
                        Ok((width, height)) => {
                            tracing::debug!("selected image dimensions={}x{}", width, height);
                            // Remove the current @token (mirror logic from insert_selected_path without inserting text)
                            // using the flat text and byte-offset cursor API.
                            let cursor_offset = self.textarea.cursor();
                            let text = self.textarea.text();
                            // Clamp to a valid char boundary to avoid panics when slicing.
                            let safe_cursor = Self::clamp_to_char_boundary(text, cursor_offset);
                            let before_cursor = &text[..safe_cursor];
                            let after_cursor = &text[safe_cursor..];

                            // Determine token boundaries in the full text.
                            let start_idx = before_cursor
                                .char_indices()
                                .rfind(|(_, c)| c.is_whitespace())
                                .map(|(idx, c)| idx + c.len_utf8())
                                .unwrap_or(0);
                            let end_rel_idx = after_cursor
                                .char_indices()
                                .find(|(_, c)| c.is_whitespace())
                                .map(|(idx, _)| idx)
                                .unwrap_or(after_cursor.len());
                            let end_idx = safe_cursor + end_rel_idx;

                            self.textarea.replace_range(start_idx..end_idx, "");
                            self.textarea.set_cursor(start_idx);

                            self.attach_image(path_buf);
                            // Add a trailing space to keep typing fluid.
                            self.textarea.insert_str(" ");
                        }
                        Err(err) => {
                            tracing::trace!("image dimensions lookup failed: {err}");
                            // Fallback to plain path insertion if metadata read fails.
                            self.insert_selected_path(&sel_path);
                        }
                    }
                } else {
                    // Non-image: inserting file path.
                    self.insert_selected_path(&sel_path);
                }
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            input => self.handle_input_basic(input),
        }
    }

    pub(super) fn handle_key_event_with_skill_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        if self.handle_shortcut_overlay_key(&key_event) {
            return (InputResult::None, true);
        }
        self.footer_mode = reset_mode_after_activity(self.footer_mode);

        let ActivePopup::Skill(popup) = &mut self.active_popup else {
            unreachable!();
        };

        let mut selected_mention: Option<(String, Option<String>)> = None;
        let mut close_popup = false;

        let result = match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_up();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                if let Some(tok) = self.current_mention_token() {
                    self.dismissed_mention_popup_token = Some(tok);
                }
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Some(mention) = popup.selected_mention() {
                    selected_mention = Some((mention.insert_text.clone(), mention.path.clone()));
                }
                close_popup = true;
                (InputResult::None, true)
            }
            input => self.handle_input_basic(input),
        };

        if close_popup {
            if let Some((insert_text, path)) = selected_mention {
                self.insert_selected_mention(&insert_text, path.as_deref());
            }
            self.active_popup = ActivePopup::None;
        }

        result
    }

    fn is_image_path(path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
    }

    pub(super) fn trim_text_elements(
        original: &str,
        trimmed: &str,
        elements: Vec<TextElement>,
    ) -> Vec<TextElement> {
        if trimmed.is_empty() || elements.is_empty() {
            return Vec::new();
        }
        let trimmed_start = original.len().saturating_sub(original.trim_start().len());
        let trimmed_end = trimmed_start.saturating_add(trimmed.len());

        elements
            .into_iter()
            .filter_map(|elem| {
                let start = elem.byte_range.start;
                let end = elem.byte_range.end;
                if end <= trimmed_start || start >= trimmed_end {
                    return None;
                }
                let new_start = start.saturating_sub(trimmed_start);
                let new_end = end.saturating_sub(trimmed_start).min(trimmed.len());
                if new_start >= new_end {
                    return None;
                }
                let placeholder = trimmed.get(new_start..new_end).map(str::to_string);
                Some(TextElement::new(
                    ByteRange {
                        start: new_start,
                        end: new_end,
                    },
                    placeholder,
                ))
            })
            .collect()
    }

    /// Expand large-paste placeholders using element ranges and rebuild other element spans.
    pub(crate) fn expand_pending_pastes(
        text: &str,
        mut elements: Vec<TextElement>,
        pending_pastes: &[(String, String)],
    ) -> (String, Vec<TextElement>) {
        if pending_pastes.is_empty() || elements.is_empty() {
            return (text.to_string(), elements);
        }

        // Stage 1: index pending paste payloads by placeholder for deterministic replacements.
        let mut pending_by_placeholder: HashMap<&str, VecDeque<&str>> = HashMap::new();
        for (placeholder, actual) in pending_pastes {
            pending_by_placeholder
                .entry(placeholder.as_str())
                .or_default()
                .push_back(actual.as_str());
        }

        // Stage 2: walk elements in order and rebuild text/spans in a single pass.
        elements.sort_by_key(|elem| elem.byte_range.start);

        let mut rebuilt = String::with_capacity(text.len());
        let mut rebuilt_elements = Vec::with_capacity(elements.len());
        let mut cursor = 0usize;

        for elem in elements {
            let start = elem.byte_range.start.min(text.len());
            let end = elem.byte_range.end.min(text.len());
            if start > end {
                continue;
            }
            if start > cursor {
                rebuilt.push_str(&text[cursor..start]);
            }
            let elem_text = &text[start..end];
            let placeholder = elem.placeholder(text).map(str::to_string);
            let replacement = placeholder
                .as_deref()
                .and_then(|ph| pending_by_placeholder.get_mut(ph))
                .and_then(VecDeque::pop_front);
            if let Some(actual) = replacement {
                // Stage 3: inline actual paste payloads and drop their placeholder elements.
                rebuilt.push_str(actual);
            } else {
                // Stage 4: keep non-paste elements, updating their byte ranges for the new text.
                let new_start = rebuilt.len();
                rebuilt.push_str(elem_text);
                let new_end = rebuilt.len();
                let placeholder = placeholder.or_else(|| Some(elem_text.to_string()));
                rebuilt_elements.push(TextElement::new(
                    ByteRange {
                        start: new_start,
                        end: new_end,
                    },
                    placeholder,
                ));
            }
            cursor = end;
        }

        // Stage 5: append any trailing text that followed the last element.
        if cursor < text.len() {
            rebuilt.push_str(&text[cursor..]);
        }

        (rebuilt, rebuilt_elements)
    }

    pub fn skills(&self) -> Option<&Vec<SkillMetadata>> {
        self.skills.as_ref()
    }

    pub fn plugins(&self) -> Option<&Vec<PluginCapabilitySummary>> {
        self.plugins.as_ref()
    }

    fn mentions_enabled(&self) -> bool {
        let skills_ready = self
            .skills
            .as_ref()
            .is_some_and(|skills| !skills.is_empty());
        let plugins_ready = self
            .plugins
            .as_ref()
            .is_some_and(|plugins| !plugins.is_empty());
        let connectors_ready = self.connectors_enabled
            && self
                .connectors_snapshot
                .as_ref()
                .is_some_and(|snapshot| !snapshot.connectors.is_empty());
        skills_ready || plugins_ready || connectors_ready
    }

    /// Extract a token prefixed with `prefix` under the cursor, if any.
    ///
    /// The returned string **does not** include the prefix.
    ///
    /// Behavior:
    /// - The cursor may be anywhere *inside* the token (including on the
    ///   leading prefix). It does **not** need to be at the end of the line.
    /// - A token is delimited by ASCII whitespace (space, tab, newline).
    /// - If the cursor is on `prefix` inside an existing token (for example the
    ///   second `@` in `@scope/pkg@latest`), keep treating the surrounding
    ///   whitespace-delimited token as the active token rather than starting a
    ///   new token at that nested prefix.
    /// - If the token under the cursor starts with `prefix`, that token is
    ///   returned without the leading prefix. When `allow_empty` is true, a
    ///   lone prefix character yields `Some(String::new())` to surface hints.
    fn current_prefixed_token(
        textarea: &TextArea,
        prefix: char,
        allow_empty: bool,
    ) -> Option<String> {
        let cursor_offset = textarea.cursor();
        let text = textarea.text();

        // Adjust the provided byte offset to the nearest valid char boundary at or before it.
        let mut safe_cursor = cursor_offset.min(text.len());
        // If we're not on a char boundary, move back to the start of the current char.
        if safe_cursor < text.len() && !text.is_char_boundary(safe_cursor) {
            // Find the last valid boundary <= cursor_offset.
            safe_cursor = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= cursor_offset)
                .last()
                .unwrap_or(0);
        }

        // Split the line around the (now safe) cursor position.
        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        // Detect whether we're on whitespace at the cursor boundary.
        let at_whitespace = if safe_cursor < text.len() {
            text[safe_cursor..]
                .chars()
                .next()
                .map(char::is_whitespace)
                .unwrap_or(false)
        } else {
            false
        };

        // Left candidate: token containing the cursor position.
        let start_left = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        let end_left_rel = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_left = safe_cursor + end_left_rel;
        let token_left = if start_left < end_left {
            Some(&text[start_left..end_left])
        } else {
            None
        };

        // Right candidate: token immediately after any whitespace from the cursor.
        let ws_len_right: usize = after_cursor
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum();
        let start_right = safe_cursor + ws_len_right;
        let end_right_rel = text[start_right..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(text.len() - start_right);
        let end_right = start_right + end_right_rel;
        let token_right = if start_right < end_right {
            Some(&text[start_right..end_right])
        } else {
            None
        };

        let prefix_str = prefix.to_string();
        let left_match = token_left.filter(|t| t.starts_with(prefix));
        let right_match = token_right.filter(|t| t.starts_with(prefix));

        let left_prefixed = left_match.map(|t| t[prefix.len_utf8()..].to_string());
        let right_prefixed = right_match.map(|t| t[prefix.len_utf8()..].to_string());

        if at_whitespace {
            if right_prefixed.is_some() {
                return right_prefixed;
            }
            if token_left.is_some_and(|t| t == prefix_str) {
                return allow_empty.then(String::new);
            }
            return left_prefixed;
        }
        if after_cursor.starts_with(prefix) {
            let prefix_starts_token = before_cursor
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
            return if prefix_starts_token {
                right_prefixed.or(left_prefixed)
            } else {
                left_prefixed
            };
        }
        left_prefixed.or(right_prefixed)
    }

    /// Extract the `@token` that the cursor is currently positioned on, if any.
    ///
    /// The returned string **does not** include the leading `@`.
    pub(super) fn current_at_token(textarea: &TextArea) -> Option<String> {
        Self::current_prefixed_token(textarea, '@', /*allow_empty*/ false)
    }

    pub(super) fn current_mention_token(&self) -> Option<String> {
        if !self.mentions_enabled() {
            return None;
        }
        Self::current_prefixed_token(&self.textarea, '$', /*allow_empty*/ true)
    }

    /// Replace the active `@token` (the one under the cursor) with `path`.
    ///
    /// The algorithm mirrors `current_at_token` so replacement works no matter
    /// where the cursor is within the token and regardless of how many
    /// `@tokens` exist in the line.
    fn insert_selected_path(&mut self, path: &str) {
        let cursor_offset = self.textarea.cursor();
        let text = self.textarea.text();
        // Clamp to a valid char boundary to avoid panics when slicing.
        let safe_cursor = Self::clamp_to_char_boundary(text, cursor_offset);

        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        // Determine token boundaries.
        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);

        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        // If the path contains whitespace, wrap it in double quotes so the
        // local prompt arg parser treats it as a single argument. Avoid adding
        // quotes when the path already contains one to keep behavior simple.
        let needs_quotes = path.chars().any(char::is_whitespace);
        let inserted = if needs_quotes && !path.contains('"') {
            format!("\"{path}\"")
        } else {
            path.to_string()
        };

        // Replace just the active `@token` so unrelated text elements, such as
        // large-paste placeholders, remain atomic and can still expand on submit.
        self.textarea
            .replace_range(start_idx..end_idx, &format!("{inserted} "));
        let new_cursor = start_idx.saturating_add(inserted.len()).saturating_add(1);
        self.textarea.set_cursor(new_cursor);
    }

    fn insert_selected_mention(&mut self, insert_text: &str, path: Option<&str>) {
        let cursor_offset = self.textarea.cursor();
        let text = self.textarea.text();
        let safe_cursor = Self::clamp_to_char_boundary(text, cursor_offset);

        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);

        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        // Remove the active token and insert the selected mention as an atomic element.
        self.textarea.replace_range(start_idx..end_idx, "");
        self.textarea.set_cursor(start_idx);
        let id = self.textarea.insert_element(insert_text);

        if let (Some(path), Some(mention)) =
            (path, Self::mention_name_from_insert_text(insert_text))
        {
            self.mention_bindings.insert(
                id,
                ComposerMentionBinding {
                    mention,
                    path: path.to_string(),
                },
            );
        }

        self.textarea.insert_str(" ");
        let new_cursor = start_idx
            .saturating_add(insert_text.len())
            .saturating_add(1);
        self.textarea.set_cursor(new_cursor);
    }

    fn mention_name_from_insert_text(insert_text: &str) -> Option<String> {
        let name = insert_text.strip_prefix('$')?;
        if name.is_empty() {
            return None;
        }
        if name
            .as_bytes()
            .iter()
            .all(|byte| is_mention_name_char(*byte))
        {
            Some(name.to_string())
        } else {
            None
        }
    }

    pub(super) fn current_mention_elements(&self) -> Vec<(u64, String)> {
        self.textarea
            .text_element_snapshots()
            .into_iter()
            .filter_map(|snapshot| {
                Self::mention_name_from_insert_text(snapshot.text.as_str())
                    .map(|mention| (snapshot.id, mention))
            })
            .collect()
    }

    pub(super) fn snapshot_mention_bindings(&self) -> Vec<MentionBinding> {
        let mut ordered = Vec::new();
        for (id, mention) in self.current_mention_elements() {
            if let Some(binding) = self.mention_bindings.get(&id)
                && binding.mention == mention
            {
                ordered.push(MentionBinding {
                    mention: binding.mention.clone(),
                    path: binding.path.clone(),
                });
            }
        }
        ordered
    }

    pub(super) fn bind_mentions_from_snapshot(&mut self, mention_bindings: Vec<MentionBinding>) {
        self.mention_bindings.clear();
        if mention_bindings.is_empty() {
            return;
        }

        let text = self.textarea.text().to_string();
        let mut scan_from = 0usize;
        for binding in mention_bindings {
            let token = format!("${}", binding.mention);
            let Some(range) =
                find_next_mention_token_range(text.as_str(), token.as_str(), scan_from)
            else {
                continue;
            };

            let id = if let Some(id) = self.textarea.add_element_range(range.clone()) {
                Some(id)
            } else {
                self.textarea.element_id_for_exact_range(range.clone())
            };

            if let Some(id) = id {
                self.mention_bindings.insert(
                    id,
                    ComposerMentionBinding {
                        mention: binding.mention,
                        path: binding.path,
                    },
                );
                scan_from = range.end;
            }
        }
    }
}
