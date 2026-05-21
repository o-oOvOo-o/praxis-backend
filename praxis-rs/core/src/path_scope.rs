use std::path::Path;

/// Normalize a real path into the canonical string form used by Praxis scope
/// checks. This is intentionally lightweight: callers are responsible for
/// resolving repo/worktree roots before policy checks when root-relative
/// semantics matter.
pub(crate) fn normalize_path_for_scope(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn normalize_scope_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("repo:")
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// Segment-aware glob/path matcher for Task.scope, CapabilityProfile path
/// scopes, dirty-file audit, and AgentOS resource checks.
///
/// This deliberately does not use substring matching. A pattern like `app` must
/// not match `myapp2`, and `tui/src/**` must not match `tui/src_backup`.
pub(crate) fn scope_matches(pattern: &str, value: &str) -> bool {
    let pattern = normalize_scope_pattern(pattern);
    let value = value.replace('\\', "/").to_ascii_lowercase();
    if pattern == "*" || pattern == "**" {
        return true;
    }
    let pattern_is_absolute = pattern.starts_with('/');
    let has_wildcards = pattern.contains('*');
    let pattern_segments = path_segments(pattern.as_str());
    let value_segments = path_segments(value.as_str());
    if pattern_segments.is_empty() {
        return value_segments.is_empty();
    }
    if !has_wildcards {
        if pattern_is_absolute {
            return value_segments.starts_with(&pattern_segments);
        }
        return (0..value_segments.len())
            .any(|start| value_segments[start..].starts_with(&pattern_segments));
    }
    if pattern_is_absolute {
        return glob_segments_match(&pattern_segments, &value_segments);
    }
    (0..=value_segments.len())
        .any(|start| glob_segments_match(&pattern_segments, &value_segments[start..]))
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn glob_segments_match(pattern: &[&str], value: &[&str]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if pattern[0] == "**" {
        return glob_segments_match(&pattern[1..], value)
            || (!value.is_empty() && glob_segments_match(pattern, &value[1..]));
    }
    if value.is_empty() {
        return false;
    }
    segment_matches(pattern[0], value[0]) && glob_segments_match(&pattern[1..], &value[1..])
}

fn segment_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }
    wildcard_match(pattern, value)
}

pub(crate) fn wildcard_match(pattern: &str, value: &str) -> bool {
    wildcard_match_bytes(pattern.as_bytes(), value.as_bytes())
}

fn wildcard_match_bytes(pattern: &[u8], value: &[u8]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if pattern[0] == b'*' {
        return wildcard_match_bytes(&pattern[1..], value)
            || (!value.is_empty() && wildcard_match_bytes(pattern, &value[1..]));
    }
    !value.is_empty() && pattern[0] == value[0] && wildcard_match_bytes(&pattern[1..], &value[1..])
}
