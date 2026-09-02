#[derive(Debug)]
pub(crate) struct LiteralMatcher<T> {
    entries: Box<[Literal<T>]>,
}

#[derive(Debug)]
struct Literal<T> {
    text: &'static str,
    value: T,
}

impl<T> LiteralMatcher<T> {
    pub(crate) fn new(entries: impl IntoIterator<Item = (&'static str, T)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(text, value)| Literal { text, value })
                .collect(),
        }
    }

    pub(crate) fn exact(&self, candidate: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|entry| entry.text == candidate)
            .map(|entry| &entry.value)
    }

    pub(crate) fn could_match_prefix(&self, candidate: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.text.starts_with(candidate))
    }

    pub(crate) fn could_extend_match(&self, candidate: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.text.len() > candidate.len() && entry.text.starts_with(candidate))
    }

    pub(crate) fn earliest(&self, text: &str) -> Option<(usize, &T)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(order, entry)| {
                text.find(entry.text)
                    .map(|offset| (offset, entry.text.len(), order, &entry.value))
            })
            .min_by_key(|(offset, len, order, _)| (*offset, std::cmp::Reverse(*len), *order))
            .map(|(offset, _, _, value)| (offset, value))
    }

    pub(crate) fn retained_suffix_len(&self, text: &str) -> usize {
        self.entries
            .iter()
            .map(|entry| partial_suffix_len(text, entry.text))
            .max()
            .unwrap_or_default()
    }
}

pub(crate) fn partial_suffix_len(text: &str, delimiter: &str) -> usize {
    let max = text.len().min(delimiter.len().saturating_sub(1));
    (1..=max)
        .rev()
        .find(|&len| delimiter.is_char_boundary(len) && text.ends_with(&delimiter[..len]))
        .unwrap_or_default()
}
