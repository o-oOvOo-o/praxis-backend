use super::super::*;

#[derive(Default)]
pub(in super::super) struct CommonThinkTagStreamState {
    pub(in super::super) mode: CommonThinkTagMode,
    pub(in super::super) pending: String,
    pub(in super::super) saw_tag: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum CommonThinkTagMode {
    #[default]
    Text,
    Reasoning,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum CommonThinkTag {
    Open,
    Close,
}

pub(in super::super) enum CommonThinkSegment {
    Text(String),
    Reasoning(String),
}

impl CommonThinkTagStreamState {
    pub(in super::super) fn push(&mut self, text: &str) -> Vec<CommonThinkSegment> {
        self.pending.push_str(text);
        self.drain(false)
    }

    pub(in super::super) fn finish(&mut self) -> Vec<CommonThinkSegment> {
        self.drain(true)
    }

    fn drain(&mut self, finish: bool) -> Vec<CommonThinkSegment> {
        let mut segments = Vec::new();
        loop {
            match self.mode {
                CommonThinkTagMode::Text => {
                    let Some((index, tag, tag_len)) = find_common_think_tag(&self.pending) else {
                        if let Some(text) = self.take_pending_text_prefix(finish) {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Text(text),
                            );
                        }
                        break;
                    };

                    let prefix = self.pending[..index].to_string();
                    self.pending.drain(..index + tag_len);
                    self.saw_tag = true;
                    match tag {
                        CommonThinkTag::Open => {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Text(prefix),
                            );
                            self.mode = CommonThinkTagMode::Reasoning;
                        }
                        CommonThinkTag::Close => {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Reasoning(prefix),
                            );
                            self.mode = CommonThinkTagMode::Text;
                        }
                    }
                }
                CommonThinkTagMode::Reasoning => {
                    let Some(index) =
                        find_ascii_case_insensitive(&self.pending, COMMON_THINK_CLOSE_TAG)
                    else {
                        if let Some(text) = self.take_pending_reasoning_prefix(finish) {
                            push_common_think_segment(
                                &mut segments,
                                CommonThinkSegment::Reasoning(text),
                            );
                        }
                        break;
                    };

                    let prefix = self.pending[..index].to_string();
                    self.pending.drain(..index + COMMON_THINK_CLOSE_TAG.len());
                    self.saw_tag = true;
                    push_common_think_segment(&mut segments, CommonThinkSegment::Reasoning(prefix));
                    self.mode = CommonThinkTagMode::Text;
                }
            }
        }
        segments
    }

    fn take_pending_text_prefix(&mut self, finish: bool) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        if finish {
            return Some(std::mem::take(&mut self.pending));
        }
        if !self.saw_tag && self.pending.len() <= COMMON_THINK_PRELUDE_BUFFER_BYTES {
            return None;
        }
        self.take_safe_pending_prefix()
    }

    fn take_pending_reasoning_prefix(&mut self, finish: bool) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        if finish {
            return Some(std::mem::take(&mut self.pending));
        }
        self.take_safe_pending_prefix()
    }

    fn take_safe_pending_prefix(&mut self) -> Option<String> {
        if self.pending.len() <= COMMON_THINK_TAG_TAIL_BYTES {
            return None;
        }
        let prefix_len = floor_char_boundary(
            self.pending.as_str(),
            self.pending.len() - COMMON_THINK_TAG_TAIL_BYTES,
        );
        if prefix_len == 0 {
            return None;
        }
        Some(self.pending.drain(..prefix_len).collect())
    }
}

pub(in super::super) fn push_common_think_segment(
    segments: &mut Vec<CommonThinkSegment>,
    segment: CommonThinkSegment,
) {
    let is_empty = match &segment {
        CommonThinkSegment::Text(text) | CommonThinkSegment::Reasoning(text) => text.is_empty(),
    };
    if !is_empty {
        segments.push(segment);
    }
}

pub(in super::super) fn find_common_think_tag(
    text: &str,
) -> Option<(usize, CommonThinkTag, usize)> {
    let open = find_ascii_case_insensitive(text, COMMON_THINK_OPEN_TAG)
        .map(|index| (index, CommonThinkTag::Open, COMMON_THINK_OPEN_TAG.len()));
    let close = find_ascii_case_insensitive(text, COMMON_THINK_CLOSE_TAG)
        .map(|index| (index, CommonThinkTag::Close, COMMON_THINK_CLOSE_TAG.len()));
    match (open, close) {
        (Some(open), Some(close)) => Some(if open.0 <= close.0 { open } else { close }),
        (Some(tag), None) | (None, Some(tag)) => Some(tag),
        (None, None) => None,
    }
}

pub(in super::super) fn find_ascii_case_insensitive(text: &str, needle: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(needle)
}

pub(in super::super) fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut index = index;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
