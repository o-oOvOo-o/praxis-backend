use super::*;

impl TextArea {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            wrap_cache: RefCell::new(None),
            preferred_col: None,
            elements: Vec::new(),
            next_element_id: 1,
            kill_buffer: String::new(),
        }
    }

    /// Replace the visible textarea text and clear any existing text elements.
    ///
    /// This is the "fresh buffer" path for callers that want plain text with no placeholder
    /// ranges. It intentionally preserves the current kill buffer, because higher-level flows such
    /// as submit or slash-command dispatch clear the draft through this method and still want
    /// `Ctrl+Y` to recover the user's most recent kill.
    pub fn set_text_clearing_elements(&mut self, text: &str) {
        self.set_text_inner(text, /*elements*/ None);
    }

    /// Replace the visible textarea text and rebuild the provided text elements.
    ///
    /// As with [`Self::set_text_clearing_elements`], this resets only state derived from the
    /// visible buffer. The kill buffer survives so callers restoring drafts or external edits do
    /// not silently discard a pending yank target.
    pub fn set_text_with_elements(&mut self, text: &str, elements: &[UserTextElement]) {
        self.set_text_inner(text, Some(elements));
    }

    pub(super) fn set_text_inner(&mut self, text: &str, elements: Option<&[UserTextElement]>) {
        // Stage 1: replace the raw text and keep the cursor in a safe byte range.
        self.text = text.to_string();
        self.cursor_pos = self.cursor_pos.clamp(0, self.text.len());
        // Stage 2: rebuild element ranges from scratch against the new text.
        self.elements.clear();
        if let Some(elements) = elements {
            for elem in elements {
                let mut start = elem.byte_range.start.min(self.text.len());
                let mut end = elem.byte_range.end.min(self.text.len());
                start = self.clamp_pos_to_char_boundary(start);
                end = self.clamp_pos_to_char_boundary(end);
                if start >= end {
                    continue;
                }
                let id = self.next_element_id();
                self.elements.push(TextElement {
                    id,
                    range: start..end,
                    name: None,
                });
            }
            self.elements.sort_by_key(|e| e.range.start);
        }
        // Stage 3: clamp the cursor and reset derived state tied to the prior content.
        // The kill buffer is editing history rather than visible-buffer state, so full-buffer
        // replacements intentionally leave it alone.
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
        self.wrap_cache.replace(None);
        self.preferred_col = None;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn zeroize_contents(&mut self) {
        use zeroize::Zeroize;

        self.text.zeroize();
        self.kill_buffer.zeroize();
        self.cursor_pos = 0;
        self.elements.clear();
        self.wrap_cache.replace(None);
        self.preferred_col = None;
    }

    pub fn insert_str(&mut self, text: &str) {
        self.insert_str_at(self.cursor_pos, text);
    }

    pub fn insert_str_at(&mut self, pos: usize, text: &str) {
        let pos = self.clamp_pos_for_insertion(pos);
        self.text.insert_str(pos, text);
        self.wrap_cache.replace(None);
        if pos <= self.cursor_pos {
            self.cursor_pos += text.len();
        }
        self.shift_elements(pos, /*removed*/ 0, text.len());
        self.preferred_col = None;
    }

    pub fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) {
        let range = self.expand_range_to_element_boundaries(range);
        self.replace_range_raw(range, text);
    }

    pub(super) fn replace_range_raw(&mut self, range: std::ops::Range<usize>, text: &str) {
        assert!(range.start <= range.end);
        let start = range.start.clamp(0, self.text.len());
        let end = range.end.clamp(0, self.text.len());
        let removed_len = end - start;
        let inserted_len = text.len();
        if removed_len == 0 && inserted_len == 0 {
            return;
        }
        let diff = inserted_len as isize - removed_len as isize;

        self.text.replace_range(range, text);
        self.wrap_cache.replace(None);
        self.preferred_col = None;
        self.update_elements_after_replace(start, end, inserted_len);

        // Update the cursor position to account for the edit.
        self.cursor_pos = if self.cursor_pos < start {
            // Cursor was before the edited range – no shift.
            self.cursor_pos
        } else if self.cursor_pos <= end {
            // Cursor was inside the replaced range – move to end of the new text.
            start + inserted_len
        } else {
            // Cursor was after the replaced range – shift by the length diff.
            ((self.cursor_pos as isize) + diff) as usize
        }
        .min(self.text.len());

        // Ensure cursor is not inside an element
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
    }
}
