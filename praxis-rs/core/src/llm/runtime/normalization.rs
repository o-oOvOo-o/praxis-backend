pub(super) fn selector_eq(left: &str, right: &str) -> bool {
    normalize_selector(left) == normalize_selector(right)
}

pub(super) fn normalize_profile_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn normalize_selector(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = false;
    let mut previous_was_lowercase = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_uppercase() {
            if previous_was_lowercase && !previous_was_separator {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            previous_was_lowercase = false;
        } else if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            previous_was_lowercase = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !previous_was_separator && !normalized.is_empty() {
            normalized.push('_');
            previous_was_separator = true;
            previous_was_lowercase = false;
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    normalized
}

pub(super) fn normalize_non_empty_tool_name(value: &str) -> Option<String> {
    let value = normalize_tool_name(value);
    (!value.is_empty()).then_some(value)
}

pub(super) fn normalize_non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(super) fn normalize_tool_name(value: &str) -> String {
    value.trim().to_string()
}
