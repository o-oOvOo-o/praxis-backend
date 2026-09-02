use std::borrow::Cow;
use std::path::Path;

/// Returns the platform lookup identity of an executable path.
pub(super) fn executable_name(raw: &str) -> Option<Cow<'_, str>> {
    let name = Path::new(raw).file_name()?.to_str()?;
    #[cfg(windows)]
    {
        let normalized = name.to_ascii_lowercase();
        let stem = [".exe", ".cmd", ".bat", ".com"]
            .into_iter()
            .find_map(|suffix| normalized.strip_suffix(suffix))
            .unwrap_or(&normalized);
        Some(Cow::Owned(stem.to_owned()))
    }
    #[cfg(not(windows))]
    {
        Some(Cow::Borrowed(name))
    }
}
