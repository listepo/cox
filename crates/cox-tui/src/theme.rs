//! Semantic colour tokens (T24.1): every colour the TUI draws is named here
//! instead of picked ad hoc at the render site. `dark()`/`light()` give
//! ANSI-16 defaults so the app works over plain SSH with no truecolor
//! support; `apply_truecolor` lets a theme file (T24.2) overlay 24-bit
//! values on top without touching the fallback; `mono()` answers `NO_COLOR`
//! by resetting every token so only `Modifier::BOLD`/`DIM` carry hierarchy.

use ratatui::style::Color;

/// One named colour per role a cell, a modal, the picker or the banner can
/// take. Every render site reads a field here instead of a `Color::`
/// literal, so a theme is one place to change and `NO_COLOR` (`mono`) is one
/// place to blank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub user: Color,
    pub agent: Color,
    pub tool: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    pub diff_add: Color,
    pub diff_del: Color,
    pub diff_hunk: Color,
    pub border: Color,
    pub selection: Color,
    pub mode_plan: Color,
    pub mode_auto: Color,
    pub mode_bypass: Color,
}

/// The banner's alert badge (T4.3's "`danger-full-access` is loud"): black
/// on `Theme::error`'s red in both themes. This one pair is a fixed
/// contrast choice, not a per-theme role, so it lives here as a constant
/// rather than a `Theme` field — the badge stays legible whatever `dark`/
/// `light` picks for everything else.
pub const ALERT_FG: Color = Color::Black;

impl Theme {
    /// The colours this crate used before T24.1, given a name instead of a
    /// literal at each call site — the token values below are exactly what
    /// used to sit inline, so a dark-theme render is unchanged.
    pub fn dark() -> Self {
        Self {
            text: Color::Reset,
            dim: Color::DarkGray,
            accent: Color::Magenta,
            user: Color::Blue,
            agent: Color::Green,
            tool: Color::Cyan,
            ok: Color::Green,
            warn: Color::Yellow,
            error: Color::Red,
            diff_add: Color::Green,
            diff_del: Color::Red,
            diff_hunk: Color::Cyan,
            border: Color::DarkGray,
            selection: Color::Cyan,
            mode_plan: Color::Blue,
            mode_auto: Color::Green,
            mode_bypass: Color::Red,
        }
    }

    /// The same roles over a light background: only the grey-scale tokens
    /// move (`dim`/`border` read as light grey rather than dark). The eight
    /// hues stay put — they are the terminal's own ANSI colours, and a
    /// terminal already adapts them to its own light palette.
    pub fn light() -> Self {
        Self {
            dim: Color::Gray,
            border: Color::Gray,
            ..Self::dark()
        }
    }

    /// `NO_COLOR`: every token resets to the terminal's own colour, so
    /// `Modifier::BOLD`/`DIM` (applied independently at each call site) are
    /// the only hierarchy left — the same intent as `color::Depth::None`,
    /// but decided once up front instead of stripped from a finished frame.
    pub fn mono() -> Self {
        Self {
            text: Color::Reset,
            dim: Color::Reset,
            accent: Color::Reset,
            user: Color::Reset,
            agent: Color::Reset,
            tool: Color::Reset,
            ok: Color::Reset,
            warn: Color::Reset,
            error: Color::Reset,
            diff_add: Color::Reset,
            diff_del: Color::Reset,
            diff_hunk: Color::Reset,
            border: Color::Reset,
            selection: Color::Reset,
            mode_plan: Color::Reset,
            mode_auto: Color::Reset,
            mode_bypass: Color::Reset,
        }
    }

    /// Overlays 24-bit values a theme file (T24.2) supplies on top of the
    /// ANSI-16 defaults; a field left `None` in `overrides` keeps whatever
    /// `self` already had.
    pub fn apply_truecolor(&mut self, overrides: &TrueColorOverrides) {
        if let Some(c) = overrides.text {
            self.text = c;
        }
        if let Some(c) = overrides.dim {
            self.dim = c;
        }
        if let Some(c) = overrides.accent {
            self.accent = c;
        }
        if let Some(c) = overrides.user {
            self.user = c;
        }
        if let Some(c) = overrides.agent {
            self.agent = c;
        }
        if let Some(c) = overrides.tool {
            self.tool = c;
        }
        if let Some(c) = overrides.ok {
            self.ok = c;
        }
        if let Some(c) = overrides.warn {
            self.warn = c;
        }
        if let Some(c) = overrides.error {
            self.error = c;
        }
        if let Some(c) = overrides.diff_add {
            self.diff_add = c;
        }
        if let Some(c) = overrides.diff_del {
            self.diff_del = c;
        }
        if let Some(c) = overrides.diff_hunk {
            self.diff_hunk = c;
        }
        if let Some(c) = overrides.border {
            self.border = c;
        }
        if let Some(c) = overrides.selection {
            self.selection = c;
        }
        if let Some(c) = overrides.mode_plan {
            self.mode_plan = c;
        }
        if let Some(c) = overrides.mode_auto {
            self.mode_auto = c;
        }
        if let Some(c) = overrides.mode_bypass {
            self.mode_bypass = c;
        }
    }
}

/// 24-bit overrides a theme file (T24.2) supplies; every field is optional
/// so a theme can redefine only the tokens it cares about and fall back to
/// the ANSI-16 default (`Theme::dark`/`light`) for the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TrueColorOverrides {
    pub text: Option<Color>,
    pub dim: Option<Color>,
    pub accent: Option<Color>,
    pub user: Option<Color>,
    pub agent: Option<Color>,
    pub tool: Option<Color>,
    pub ok: Option<Color>,
    pub warn: Option<Color>,
    pub error: Option<Color>,
    pub diff_add: Option<Color>,
    pub diff_del: Option<Color>,
    pub diff_hunk: Option<Color>,
    pub border: Option<Color>,
    pub selection: Option<Color>,
    pub mode_plan: Option<Color>,
    pub mode_auto: Option<Color>,
    pub mode_bypass: Option<Color>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_resets_every_token() {
        let mono = Theme::mono();
        assert_eq!(mono.text, Color::Reset);
        assert_eq!(mono.error, Color::Reset);
        assert_eq!(mono.mode_bypass, Color::Reset);
    }

    #[test]
    fn dark_and_light_share_hues_but_not_greys() {
        assert_eq!(Theme::dark().error, Theme::light().error);
        assert_eq!(Theme::dark().selection, Theme::light().selection);
        assert_ne!(Theme::dark().dim, Theme::light().dim);
        assert_ne!(Theme::dark().border, Theme::light().border);
    }

    #[test]
    fn apply_truecolor_overlays_only_the_given_fields() {
        let mut theme = Theme::dark();
        let overrides = TrueColorOverrides {
            error: Some(Color::Rgb(220, 20, 60)),
            ..Default::default()
        };
        theme.apply_truecolor(&overrides);
        assert_eq!(theme.error, Color::Rgb(220, 20, 60));
        assert_eq!(
            theme.tool,
            Theme::dark().tool,
            "a field left `None` keeps its default"
        );
    }

    /// T24.1's grep test: every non-test line outside `theme.rs`/`color.rs`
    /// picks a colour through a `Theme` token, never a `Color::` literal.
    /// `svg.rs` (already-resolved `Buffer` → CSS) and `markdown.rs` (syntect's
    /// own 24-bit syntax colours, T24.3's territory) convert a colour someone
    /// else chose rather than choosing a UI one, so both are exempt like
    /// `color.rs` itself.
    #[test]
    fn no_color_literal_outside_theme() {
        let src = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let exempt = ["theme.rs", "color.rs", "svg.rs", "markdown.rs"];
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(src).expect("read cox-tui/src") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if exempt.contains(&name.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source file");
            // Tests may compare against a literal colour freely, same as the
            // crate's `unwrap`/`expect` convention; only non-test code must
            // route through `Theme`.
            for line in text.lines().take_while(|l| l.trim() != "#[cfg(test)]") {
                if line.contains("Color::") {
                    offenders.push(format!("{name}: {}", line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "Color:: literal outside theme.rs/color.rs: {offenders:#?}"
        );
    }
}
