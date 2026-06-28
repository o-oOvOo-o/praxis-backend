use super::*;

/// Controls which color palette the diff renderer uses for backgrounds and
/// gutter styling.
///
/// Determined once per `render_change` call via [`diff_theme`], which probes
/// the terminal's queried background color.  When the background cannot be
/// determined (common in CI or piped output), `Dark` is used as the safe
/// default.
#[derive(Clone, Copy, Debug)]
pub(super) enum DiffTheme {
    Dark,
    Light,
}

/// Palette depth the diff renderer will target.
///
/// This is the *renderer's own* notion of color depth, derived from — but not
/// identical to — the raw [`StdoutColorLevel`] reported by `supports-color`.
/// The indirection exists because some terminals (notably Windows Terminal)
/// advertise only ANSI-16 support while actually rendering truecolor sequences
/// correctly; [`diff_color_level_for_terminal`] promotes those cases so the
/// diff output uses the richer palette.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DiffColorLevel {
    TrueColor,
    Ansi256,
    Ansi16,
}

/// Subset of [`DiffColorLevel`] that supports tinted backgrounds.
///
/// ANSI-16 terminals render backgrounds with bold, saturated palette entries
/// that overpower syntax tokens.  This type encodes the invariant "we have
/// enough color depth for pastel tints" so that background-producing helpers
/// (`add_line_bg`, `del_line_bg`, `light_add_num_bg`, `light_del_num_bg`)
/// never need an unreachable ANSI-16 arm.
///
/// Construct via [`RichDiffColorLevel::from_diff_color_level`], which returns
/// `None` for ANSI-16 — callers branch on the `Option` and skip backgrounds
/// entirely when `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RichDiffColorLevel {
    TrueColor,
    Ansi256,
}

impl RichDiffColorLevel {
    /// Extract a rich level, returning `None` for ANSI-16.
    fn from_diff_color_level(level: DiffColorLevel) -> Option<Self> {
        match level {
            DiffColorLevel::TrueColor => Some(Self::TrueColor),
            DiffColorLevel::Ansi256 => Some(Self::Ansi256),
            DiffColorLevel::Ansi16 => None,
        }
    }
}

/// Pre-resolved background colors for insert and delete diff lines.
///
/// Computed once per `render_change` call from the active syntax theme's
/// scope backgrounds (via [`resolve_diff_backgrounds`]) and then threaded
/// through every style helper so individual lines never re-query the theme.
///
/// Both fields are `None` when the color level is ANSI-16 — callers fall
/// back to foreground-only styling in that case.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ResolvedDiffBackgrounds {
    pub(super) add: Option<Color>,
    pub(super) del: Option<Color>,
}

/// Precomputed render state for diff line styling.
///
/// This bundles the terminal-derived theme and color depth plus theme-resolved
/// diff backgrounds so callers rendering many lines can compute once per render
/// pass and reuse it across all line calls.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffRenderStyleContext {
    pub(super) theme: DiffTheme,
    pub(super) color_level: DiffColorLevel,
    pub(super) diff_backgrounds: ResolvedDiffBackgrounds,
}

/// Resolve diff backgrounds for production rendering.
///
/// Queries the active syntax theme for `markup.inserted` / `markup.deleted`
/// (and `diff.*` fallbacks), then delegates to [`resolve_diff_backgrounds_for`].
fn resolve_diff_backgrounds(
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> ResolvedDiffBackgrounds {
    resolve_diff_backgrounds_for(theme, color_level, diff_scope_background_rgbs())
}

/// Snapshot the current terminal environment into a reusable style context.
///
/// Queries `diff_theme`, `diff_color_level`, and the active syntax theme's
/// scope backgrounds once, bundling them into a [`DiffRenderStyleContext`]
/// that callers thread through every line-rendering call in a single pass.
///
/// Call this at the top of each render frame — not per line — so the diff
/// palette stays consistent within a frame even if the user swaps themes
/// mid-render (theme picker live preview).
pub(crate) fn current_diff_render_style_context() -> DiffRenderStyleContext {
    let theme = diff_theme();
    let color_level = diff_color_level();
    let diff_backgrounds = resolve_diff_backgrounds(theme, color_level);
    DiffRenderStyleContext {
        theme,
        color_level,
        diff_backgrounds,
    }
}

/// Core background-resolution logic, kept pure for testability.
///
/// Starts from the hardcoded fallback palette and then overrides with theme
/// scope backgrounds when both (a) the color level is rich enough and (b) the
/// theme defines a matching scope.  This means the fallback palette is always
/// the baseline and theme scopes are strictly additive.
fn resolve_diff_backgrounds_for(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    scope_backgrounds: DiffScopeBackgroundRgbs,
) -> ResolvedDiffBackgrounds {
    let mut resolved = fallback_diff_backgrounds(theme, color_level);
    let Some(level) = RichDiffColorLevel::from_diff_color_level(color_level) else {
        return resolved;
    };

    if let Some(rgb) = scope_backgrounds.inserted {
        resolved.add = Some(color_from_rgb_for_level(rgb, level));
    }
    if let Some(rgb) = scope_backgrounds.deleted {
        resolved.del = Some(color_from_rgb_for_level(rgb, level));
    }
    resolved
}

/// Hardcoded palette backgrounds, used when the syntax theme provides no
/// diff-specific scope backgrounds.  Returns empty backgrounds for ANSI-16.
fn fallback_diff_backgrounds(
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> ResolvedDiffBackgrounds {
    match RichDiffColorLevel::from_diff_color_level(color_level) {
        Some(level) => ResolvedDiffBackgrounds {
            add: Some(add_line_bg(theme, level)),
            del: Some(del_line_bg(theme, level)),
        },
        None => ResolvedDiffBackgrounds::default(),
    }
}

/// Convert an RGB triple to the appropriate ratatui `Color` for the given
/// rich color level — passthrough for truecolor, quantized for ANSI-256.
fn color_from_rgb_for_level(rgb: (u8, u8, u8), color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(rgb),
        RichDiffColorLevel::Ansi256 => quantize_rgb_to_ansi256(rgb),
    }
}

/// Find the closest ANSI-256 color (indices 16–255) to `target` using
/// perceptual distance.
///
/// Skips the first 16 entries (system colors) because their actual RGB
/// values depend on the user's terminal configuration and are unreliable
/// for distance calculations.
fn quantize_rgb_to_ansi256(target: (u8, u8, u8)) -> Color {
    let best_index = XTERM_COLORS
        .iter()
        .enumerate()
        .skip(16)
        .min_by(|(_, a), (_, b)| {
            perceptual_distance(**a, target).total_cmp(&perceptual_distance(**b, target))
        })
        .map(|(index, _)| index as u8);
    match best_index {
        Some(index) => indexed_color(index),
        None => indexed_color(DARK_256_ADD_LINE_BG_IDX),
    }
}

/// Testable helper: picks `DiffTheme` from an explicit background sample.
fn diff_theme_for_bg(bg: Option<(u8, u8, u8)>) -> DiffTheme {
    if let Some(rgb) = bg
        && is_light(rgb)
    {
        return DiffTheme::Light;
    }
    DiffTheme::Dark
}

/// Probe the terminal's background and return the appropriate diff palette.
fn diff_theme() -> DiffTheme {
    diff_theme_for_bg(default_bg())
}

/// Return the [`DiffColorLevel`] for the current terminal session.
///
/// This is the environment-reading adapter: it samples runtime signals
/// (`supports-color` level, terminal name, `WT_SESSION`, and `FORCE_COLOR`)
/// and forwards them to [`diff_color_level_for_terminal`].
///
/// Keeping env reads in this thin wrapper lets
/// [`diff_color_level_for_terminal`] stay pure and easy to unit test.
fn diff_color_level() -> DiffColorLevel {
    diff_color_level_for_terminal(
        stdout_color_level(),
        terminal_info().name,
        std::env::var_os("WT_SESSION").is_some(),
        has_force_color_override(),
    )
}

/// Returns whether `FORCE_COLOR` is explicitly set.
fn has_force_color_override() -> bool {
    std::env::var_os("FORCE_COLOR").is_some()
}

/// Map a raw [`StdoutColorLevel`] to a [`DiffColorLevel`] using
/// Windows Terminal-specific truecolor promotion rules.
///
/// This helper is intentionally pure (no env access) so tests can validate
/// the policy table by passing explicit inputs.
///
/// Windows Terminal fully supports 24-bit color but the `supports-color`
/// crate often reports only ANSI-16 there because no `COLORTERM` variable
/// is set.  We detect Windows Terminal two ways — via `terminal_name`
/// (parsed from `WT_SESSION` / `TERM_PROGRAM` by `terminal_info()`) and
/// via the raw `has_wt_session` flag.
///
/// These signals are intentionally not equivalent: `terminal_name` is a
/// derived classification with `TERM_PROGRAM` precedence, so `WT_SESSION`
/// can be present while `terminal_name` is not `WindowsTerminal`.
///
/// When `WT_SESSION` is present, we promote to truecolor unconditionally
/// unless `FORCE_COLOR` is set. This keeps Windows Terminal rendering rich
/// by default while preserving explicit `FORCE_COLOR` user intent.
///
/// Outside `WT_SESSION`, only ANSI-16 is promoted for identified
/// `WindowsTerminal` sessions; `Unknown` stays conservative.
fn diff_color_level_for_terminal(
    stdout_level: StdoutColorLevel,
    terminal_name: TerminalName,
    has_wt_session: bool,
    has_force_color_override: bool,
) -> DiffColorLevel {
    if has_wt_session && !has_force_color_override {
        return DiffColorLevel::TrueColor;
    }

    let base = match stdout_level {
        StdoutColorLevel::TrueColor => DiffColorLevel::TrueColor,
        StdoutColorLevel::Ansi256 => DiffColorLevel::Ansi256,
        StdoutColorLevel::Ansi16 | StdoutColorLevel::Unknown => DiffColorLevel::Ansi16,
    };

    // Outside `WT_SESSION`, keep the existing Windows Terminal promotion for
    // ANSI-16 sessions that likely support truecolor.
    if stdout_level == StdoutColorLevel::Ansi16
        && terminal_name == TerminalName::WindowsTerminal
        && !has_force_color_override
    {
        DiffColorLevel::TrueColor
    } else {
        base
    }
}

// -- Style helpers ------------------------------------------------------------
//
// Each diff line is composed of three visual regions, styled independently:
//
//   ┌──────────┬──────┬──────────────────────────────────────────┐
//   │  gutter  │ sign │              content                     │
//   │ (line #) │ +/-  │  (plain or syntax-highlighted text)      │
//   └──────────┴──────┴──────────────────────────────────────────┘
//
// A fourth, full-width layer — `line_bg` — is applied via `RtLine::style()`
// so that the background tint extends from the leftmost column to the right
// edge of the terminal, including any padding beyond the content.
//
// On dark terminals, the sign and content share one style (colored fg + tinted
// bg), and the gutter is simply dimmed.  On light terminals, sign and content
// are split: the sign gets only a colored foreground (no bg, so the line bg
// shows through), while content relies on the line bg alone; the gutter gets
// an opaque, more-saturated background so line numbers stay readable against
// the pastel line tint.

/// Full-width background applied to the `RtLine` itself (not individual spans).
/// Context lines intentionally leave the background unset so the terminal
/// default shows through.
pub(super) fn style_line_bg_for(
    kind: DiffLineType,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match kind {
        DiffLineType::Insert => diff_backgrounds
            .add
            .map_or_else(Style::default, |bg| Style::default().bg(bg)),
        DiffLineType::Delete => diff_backgrounds
            .del
            .map_or_else(Style::default, |bg| Style::default().bg(bg)),
        DiffLineType::Context => Style::default(),
    }
}

pub(super) fn style_context() -> Style {
    Style::default()
}

fn add_line_bg(theme: DiffTheme, color_level: RichDiffColorLevel) -> Color {
    match (theme, color_level) {
        (DiffTheme::Dark, RichDiffColorLevel::TrueColor) => rgb_color(DARK_TC_ADD_LINE_BG_RGB),
        (DiffTheme::Dark, RichDiffColorLevel::Ansi256) => indexed_color(DARK_256_ADD_LINE_BG_IDX),
        (DiffTheme::Light, RichDiffColorLevel::TrueColor) => rgb_color(LIGHT_TC_ADD_LINE_BG_RGB),
        (DiffTheme::Light, RichDiffColorLevel::Ansi256) => indexed_color(LIGHT_256_ADD_LINE_BG_IDX),
    }
}

fn del_line_bg(theme: DiffTheme, color_level: RichDiffColorLevel) -> Color {
    match (theme, color_level) {
        (DiffTheme::Dark, RichDiffColorLevel::TrueColor) => rgb_color(DARK_TC_DEL_LINE_BG_RGB),
        (DiffTheme::Dark, RichDiffColorLevel::Ansi256) => indexed_color(DARK_256_DEL_LINE_BG_IDX),
        (DiffTheme::Light, RichDiffColorLevel::TrueColor) => rgb_color(LIGHT_TC_DEL_LINE_BG_RGB),
        (DiffTheme::Light, RichDiffColorLevel::Ansi256) => indexed_color(LIGHT_256_DEL_LINE_BG_IDX),
    }
}

fn light_gutter_fg(color_level: DiffColorLevel) -> Color {
    match color_level {
        DiffColorLevel::TrueColor => rgb_color(LIGHT_TC_GUTTER_FG_RGB),
        DiffColorLevel::Ansi256 => indexed_color(LIGHT_256_GUTTER_FG_IDX),
        DiffColorLevel::Ansi16 => Color::Black,
    }
}

fn light_add_num_bg(color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(LIGHT_TC_ADD_NUM_BG_RGB),
        RichDiffColorLevel::Ansi256 => indexed_color(LIGHT_256_ADD_NUM_BG_IDX),
    }
}

fn light_del_num_bg(color_level: RichDiffColorLevel) -> Color {
    match color_level {
        RichDiffColorLevel::TrueColor => rgb_color(LIGHT_TC_DEL_NUM_BG_RGB),
        RichDiffColorLevel::Ansi256 => indexed_color(LIGHT_256_DEL_NUM_BG_IDX),
    }
}

/// Line-number gutter style.  On light backgrounds the gutter has an opaque
/// tinted background so numbers contrast against the pastel line fill.  On
/// dark backgrounds a simple `DIM` modifier is sufficient.
pub(super) fn style_gutter_for(
    kind: DiffLineType,
    theme: DiffTheme,
    color_level: DiffColorLevel,
) -> Style {
    match (
        theme,
        kind,
        RichDiffColorLevel::from_diff_color_level(color_level),
    ) {
        (DiffTheme::Light, DiffLineType::Insert, None) => {
            Style::default().fg(light_gutter_fg(color_level))
        }
        (DiffTheme::Light, DiffLineType::Delete, None) => {
            Style::default().fg(light_gutter_fg(color_level))
        }
        (DiffTheme::Light, DiffLineType::Insert, Some(level)) => Style::default()
            .fg(light_gutter_fg(color_level))
            .bg(light_add_num_bg(level)),
        (DiffTheme::Light, DiffLineType::Delete, Some(level)) => Style::default()
            .fg(light_gutter_fg(color_level))
            .bg(light_del_num_bg(level)),
        _ => style_gutter_dim(),
    }
}

/// Sign character (`+`) for insert lines.  On dark terminals it inherits the
/// full content style (green fg + tinted bg).  On light terminals it uses only
/// a green foreground and lets the line-level bg show through.
pub(super) fn style_sign_add(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match theme {
        DiffTheme::Light => Style::default().fg(Color::Green),
        DiffTheme::Dark => style_add(theme, color_level, diff_backgrounds),
    }
}

/// Sign character (`-`) for delete lines.  Mirror of [`style_sign_add`].
pub(super) fn style_sign_del(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match theme {
        DiffTheme::Light => Style::default().fg(Color::Red),
        DiffTheme::Dark => style_del(theme, color_level, diff_backgrounds),
    }
}

/// Content style for insert lines (plain, non-syntax-highlighted text).
///
/// Foreground-only on ANSI-16.  On rich levels, uses the pre-resolved
/// background from `diff_backgrounds` — which is the theme scope color when
/// available, or the hardcoded palette otherwise.  Dark themes add an
/// explicit green foreground for readability over the tinted background;
/// light themes rely on the default (dark) foreground against the pastel.
///
/// When no background is resolved (e.g. a theme that defines no diff
/// scopes and the fallback palette is somehow empty), the style degrades
/// to foreground-only so the line is still legible.
pub(super) fn style_add(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match (theme, color_level, diff_backgrounds.add) {
        (_, DiffColorLevel::Ansi16, _) => Style::default().fg(Color::Green),
        (DiffTheme::Light, DiffColorLevel::TrueColor, Some(bg))
        | (DiffTheme::Light, DiffColorLevel::Ansi256, Some(bg)) => Style::default().bg(bg),
        (DiffTheme::Dark, DiffColorLevel::TrueColor, Some(bg))
        | (DiffTheme::Dark, DiffColorLevel::Ansi256, Some(bg)) => {
            Style::default().fg(Color::Green).bg(bg)
        }
        (DiffTheme::Light, DiffColorLevel::TrueColor, None)
        | (DiffTheme::Light, DiffColorLevel::Ansi256, None) => Style::default(),
        (DiffTheme::Dark, DiffColorLevel::TrueColor, None)
        | (DiffTheme::Dark, DiffColorLevel::Ansi256, None) => Style::default().fg(Color::Green),
    }
}

/// Content style for delete lines (plain, non-syntax-highlighted text).
///
/// Mirror of [`style_add`] with red foreground and the delete-side
/// resolved background.
pub(super) fn style_del(
    theme: DiffTheme,
    color_level: DiffColorLevel,
    diff_backgrounds: ResolvedDiffBackgrounds,
) -> Style {
    match (theme, color_level, diff_backgrounds.del) {
        (_, DiffColorLevel::Ansi16, _) => Style::default().fg(Color::Red),
        (DiffTheme::Light, DiffColorLevel::TrueColor, Some(bg))
        | (DiffTheme::Light, DiffColorLevel::Ansi256, Some(bg)) => Style::default().bg(bg),
        (DiffTheme::Dark, DiffColorLevel::TrueColor, Some(bg))
        | (DiffTheme::Dark, DiffColorLevel::Ansi256, Some(bg)) => {
            Style::default().fg(Color::Red).bg(bg)
        }
        (DiffTheme::Light, DiffColorLevel::TrueColor, None)
        | (DiffTheme::Light, DiffColorLevel::Ansi256, None) => Style::default(),
        (DiffTheme::Dark, DiffColorLevel::TrueColor, None)
        | (DiffTheme::Dark, DiffColorLevel::Ansi256, None) => Style::default().fg(Color::Red),
    }
}

fn style_gutter_dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
