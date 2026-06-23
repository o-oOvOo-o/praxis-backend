use crate::wrapping::text_contains_url_like;
use dirs::home_dir;
use praxis_utils_string::normalize_markdown_hash_location_suffix;
use regex_lite::Regex;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;
use url::Url;

static COLON_LOCATION_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r":\d+(?::\d+)?(?:[-–]\d+(?::\d+)?)?$") {
            Ok(regex) => regex,
            Err(error) => panic!("invalid location suffix regex: {error}"),
        },
    );

// Covered by load_location_suffix_regexes.
static HASH_LOCATION_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"^L\d+(?:C\d+)?(?:-L\d+(?:C\d+)?)?$") {
        Ok(regex) => regex,
        Err(error) => panic!("invalid hash location regex: {error}"),
    });

pub(crate) fn is_explicit_local_link_target(dest_url: &str) -> bool {
    dest_url.starts_with("file://")
        || dest_url.starts_with('/')
        || dest_url.starts_with("~/")
        || dest_url.starts_with("./")
        || dest_url.starts_with("../")
        || dest_url.starts_with("\\\\")
        || matches!(
            dest_url.as_bytes(),
            [drive, b':', separator, ..]
                if drive.is_ascii_alphabetic() && matches!(separator, b'/' | b'\\')
        )
}

fn is_probable_relative_local_link_target(dest_url: &str) -> bool {
    if dest_url.is_empty()
        || dest_url.contains(char::is_whitespace)
        || text_contains_url_like(dest_url)
        || is_explicit_local_link_target(dest_url)
    {
        return false;
    }

    let Some((path_text, location_suffix)) = parse_local_link_target(dest_url) else {
        return false;
    };
    if is_absolute_local_link_path(&path_text) {
        return false;
    }

    let path_text = trim_trailing_local_path_separator(&path_text);
    let last_segment = path_text.rsplit('/').next().unwrap_or(path_text);
    location_suffix.is_some() || looks_like_file_name(last_segment)
}

fn looks_like_file_name(segment: &str) -> bool {
    segment.contains('.') && segment.chars().any(char::is_alphanumeric)
}

pub(super) fn is_local_path_like_link(dest_url: &str) -> bool {
    is_explicit_local_link_target(dest_url) || is_probable_relative_local_link_target(dest_url)
}

/// Parse a local link target into normalized path text plus an optional location suffix.
///
/// This accepts the path shapes Praxis emits today: `file://` URLs, absolute and relative paths,
/// `~/...`, Windows paths, and `#L..C..` or `:line:col` suffixes.
pub(super) fn render_local_link_target(dest_url: &str, cwd: Option<&Path>) -> Option<String> {
    let (path_text, location_suffix) = parse_local_link_target(dest_url)?;
    let mut rendered = display_local_link_path(&path_text, cwd);
    if let Some(location_suffix) = location_suffix {
        rendered.push_str(&location_suffix);
    }
    Some(rendered)
}

/// Split a local-link destination into `(normalized_path_text, location_suffix)`.
///
/// The returned path text never includes a trailing `#L..` or `:line[:col]` suffix. Path
/// normalization expands `~/...` when possible and rewrites path separators into display-stable
/// forward slashes. The suffix, when present, is returned separately in normalized markdown form.
///
/// Returns `None` only when the destination looks like a `file://` URL but cannot be parsed into a
/// local path. Plain path-like inputs always return `Some(...)` even if they are relative.
fn parse_local_link_target(dest_url: &str) -> Option<(String, Option<String>)> {
    if dest_url.starts_with("file://") {
        let url = Url::parse(dest_url).ok()?;
        let path_text = file_url_to_local_path_text(&url)?;
        let location_suffix = url
            .fragment()
            .and_then(normalize_hash_location_suffix_fragment);
        return Some((path_text, location_suffix));
    }

    let mut path_text = dest_url;
    let mut location_suffix = None;
    // Prefer `#L..` style fragments when both forms are present so URLs like `path#L10` do not
    // get misparsed as a plain path ending in `:10`.
    if let Some((candidate_path, fragment)) = dest_url.rsplit_once('#')
        && let Some(normalized) = normalize_hash_location_suffix_fragment(fragment)
    {
        path_text = candidate_path;
        location_suffix = Some(normalized);
    }
    if location_suffix.is_none()
        && let Some(suffix) = extract_colon_location_suffix(path_text)
    {
        let path_len = path_text.len().saturating_sub(suffix.len());
        path_text = &path_text[..path_len];
        location_suffix = Some(suffix);
    }

    Some((expand_local_link_path(path_text), location_suffix))
}

/// Normalize a hash fragment like `L12` or `L12C3-L14C9` into the display suffix we render.
///
/// Returns `None` for fragments that are not location references. This deliberately ignores other
/// `#...` fragments so non-location hashes stay part of the path text.
fn normalize_hash_location_suffix_fragment(fragment: &str) -> Option<String> {
    HASH_LOCATION_SUFFIX_RE
        .is_match(fragment)
        .then(|| format!("#{fragment}"))
        .and_then(|suffix| normalize_markdown_hash_location_suffix(&suffix))
}

/// Extract a trailing `:line`, `:line:col`, or range suffix from a plain path-like string.
///
/// The suffix must occur at the end of the input; embedded colons elsewhere in the path are left
/// alone. This is what keeps Windows drive letters like `C:/...` from being misread as locations.
fn extract_colon_location_suffix(path_text: &str) -> Option<String> {
    COLON_LOCATION_SUFFIX_RE
        .find(path_text)
        .filter(|matched| matched.end() == path_text.len())
        .map(|matched| matched.as_str().to_string())
}

/// Expand home-relative paths and normalize separators for display.
///
/// If `~/...` cannot be expanded because the home directory is unavailable, the original text still
/// goes through separator normalization and is returned as-is otherwise.
fn expand_local_link_path(path_text: &str) -> String {
    // Expand `~/...` eagerly so home-relative links can participate in the same normalization and
    // cwd-relative shortening path as absolute links.
    if let Some(rest) = path_text.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return normalize_local_link_path_text(&home.join(rest).to_string_lossy());
    }

    normalize_local_link_path_text(path_text)
}

/// Convert a `file://` URL into the normalized local-path text used for transcript rendering.
///
/// This prefers `Url::to_file_path()` for standard file URLs. When that rejects Windows-oriented
/// encodings, we reconstruct a display path from the host/path parts so UNC paths and drive-letter
/// URLs still render sensibly.
fn file_url_to_local_path_text(url: &Url) -> Option<String> {
    if let Ok(path) = url.to_file_path() {
        return Some(normalize_local_link_path_text(&path.to_string_lossy()));
    }

    // Fall back to string reconstruction for cases `to_file_path()` rejects, especially UNC-style
    // hosts and Windows drive paths encoded in URL form.
    let mut path_text = url.path().to_string();
    if let Some(host) = url.host_str()
        && !host.is_empty()
        && host != "localhost"
    {
        path_text = format!("//{host}{path_text}");
    } else if matches!(
        path_text.as_bytes(),
        [b'/', drive, b':', b'/', ..] if drive.is_ascii_alphabetic()
    ) {
        path_text.remove(0);
    }

    Some(normalize_local_link_path_text(&path_text))
}

/// Normalize local-path text into the transcript display form.
///
/// Display normalization is intentionally lexical: it does not touch the filesystem, resolve
/// symlinks, or collapse `.` / `..`. It only converts separators to forward slashes and rewrites
/// UNC-style `\\\\server\\share` inputs into `//server/share` so later prefix checks operate on a
/// stable representation.
fn normalize_local_link_path_text(path_text: &str) -> String {
    // Render all local link paths with forward slashes so display and prefix stripping are stable
    // across mixed Windows and Unix-style inputs.
    if let Some(rest) = path_text.strip_prefix("\\\\") {
        format!("//{}", rest.replace('\\', "/").trim_start_matches('/'))
    } else {
        path_text.replace('\\', "/")
    }
}

fn is_absolute_local_link_path(path_text: &str) -> bool {
    path_text.starts_with('/')
        || path_text.starts_with("//")
        || matches!(
            path_text.as_bytes(),
            [drive, b':', b'/', ..] if drive.is_ascii_alphabetic()
        )
}

/// Remove trailing separators from a local path without destroying root semantics.
///
/// Roots like `/`, `//`, and `C:/` stay intact so callers can still distinguish "the root itself"
/// from "a path under the root".
fn trim_trailing_local_path_separator(path_text: &str) -> &str {
    if path_text == "/" || path_text == "//" {
        return path_text;
    }
    if matches!(path_text.as_bytes(), [drive, b':', b'/'] if drive.is_ascii_alphabetic()) {
        return path_text;
    }
    path_text.trim_end_matches('/')
}

/// Strip `cwd_text` from the start of `path_text` when `path_text` is strictly underneath it.
///
/// Returns the relative remainder without a leading slash. If the path equals the cwd exactly, this
/// returns `None` so callers can keep rendering the full path instead of collapsing it to an empty
/// string.
fn strip_local_path_prefix<'a>(path_text: &'a str, cwd_text: &str) -> Option<&'a str> {
    let path_text = trim_trailing_local_path_separator(path_text);
    let cwd_text = trim_trailing_local_path_separator(cwd_text);
    if path_text == cwd_text {
        return None;
    }

    // Treat filesystem roots specially so `/tmp/x` under `/` becomes `tmp/x` instead of being
    // left unchanged by the generic prefix-stripping branch.
    if cwd_text == "/" || cwd_text == "//" {
        return path_text.strip_prefix('/');
    }

    path_text
        .strip_prefix(cwd_text)
        .and_then(|rest| rest.strip_prefix('/'))
}

/// Choose the visible path text for a local link after normalization.
///
/// Relative paths stay relative. Absolute paths are shortened against `cwd` only when they are
/// lexically underneath it; otherwise the absolute path is preserved. This is display logic only,
/// not filesystem canonicalization.
fn display_local_link_path(path_text: &str, cwd: Option<&Path>) -> String {
    let path_text = normalize_local_link_path_text(path_text);
    if !is_absolute_local_link_path(&path_text) {
        return path_text;
    }

    if let Some(cwd) = cwd {
        // Only shorten absolute paths that are under the provided session cwd; otherwise preserve
        // the original absolute target for clarity.
        let cwd_text = normalize_local_link_path_text(&cwd.to_string_lossy());
        if let Some(stripped) = strip_local_path_prefix(&path_text, &cwd_text) {
            return stripped.to_string();
        }
    }

    path_text
}

pub(crate) fn hyperlink_target_for_local_link_text(
    dest_url: &str,
    cwd: Option<&Path>,
) -> Option<String> {
    if !is_local_path_like_link(dest_url) {
        return None;
    }

    let (path_text, location_suffix) = parse_local_link_target(dest_url)?;
    let resolved_path_text = resolve_local_link_path_text(&path_text, cwd)?;
    let mut hyperlink = local_path_text_to_file_url(&resolved_path_text)?;
    if let Some(fragment) = location_suffix
        .as_deref()
        .and_then(location_suffix_to_file_url_fragment)
    {
        hyperlink.push('#');
        hyperlink.push_str(&fragment);
    }
    Some(hyperlink)
}

fn resolve_local_link_path_text(path_text: &str, cwd: Option<&Path>) -> Option<String> {
    let path_text = normalize_local_link_path_text(path_text);
    if is_absolute_local_link_path(&path_text) {
        return Some(path_text);
    }

    let cwd = cwd?;
    Some(normalize_local_link_path_text(
        &cwd.join(path_text).to_string_lossy(),
    ))
}

fn local_path_text_to_file_url(path_text: &str) -> Option<String> {
    if let Ok(url) = Url::from_file_path(PathBuf::from(path_text)) {
        return Some(url.to_string());
    }

    let encoded = percent_encode_file_url_path_text(path_text);
    if path_text.starts_with("//") {
        Some(format!("file:{encoded}"))
    } else if path_text.starts_with('/') {
        Some(format!("file://{encoded}"))
    } else if matches!(
        path_text.as_bytes(),
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic()
    ) {
        Some(format!("file:///{encoded}"))
    } else {
        None
    }
}

fn percent_encode_file_url_path_text(path_text: &str) -> String {
    let mut encoded = String::with_capacity(path_text.len());
    for ch in path_text.chars() {
        match ch {
            '%' => encoded.push_str("%25"),
            ' ' => encoded.push_str("%20"),
            '#' => encoded.push_str("%23"),
            '?' => encoded.push_str("%3F"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

fn location_suffix_to_file_url_fragment(location_suffix: &str) -> Option<String> {
    let body = location_suffix.strip_prefix(':')?;
    let (start, end) = match body.split_once(['-', '–']) {
        Some((start, end)) => (start, Some(end)),
        None => (body, None),
    };

    let mut fragment = location_point_to_file_url_fragment(start)?;
    if let Some(end) = end {
        fragment.push('-');
        fragment.push_str(&location_point_to_file_url_fragment(end)?);
    }
    Some(fragment)
}

fn location_point_to_file_url_fragment(point: &str) -> Option<String> {
    let (line, column) = match point.split_once(':') {
        Some((line, column)) => (line, Some(column)),
        None => (point, None),
    };
    if line.is_empty() || !line.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let mut fragment = format!("L{line}");
    if let Some(column) = column {
        if column.is_empty() || !column.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        fragment.push('C');
        fragment.push_str(column);
    }
    Some(fragment)
}
