//! Colour tokens and degradation (v2.1 §7, ADR-004, R25, V-C06).
//!
//! Three rules the review pinned down:
//! - v2.0 only listed dark values while requiring five themes; Light and High-contrast are
//!   here because `production = #FDA4AF` is nearly invisible on a light background, and that
//!   is the one marker that must never be missed (R25)
//! - colour degrades 24-bit → 256 → 16 → mono, and **structure survives all the way down**:
//!   with no colour at all the header is still distinguishable by its double rule (§7.2)
//! - a semantic difference is never carried by colour alone; the text says it too

use std::fmt;

/// A colour as authored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parse `#rrggbb`.
    #[must_use]
    pub fn hex(s: &str) -> Option<Self> {
        let h = s.strip_prefix('#')?;
        if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self(
            u8::from_str_radix(&h[0..2], 16).ok()?,
            u8::from_str_radix(&h[2..4], 16).ok()?,
            u8::from_str_radix(&h[4..6], 16).ok()?,
        ))
    }

    /// Relative luminance per WCAG 2.x.
    #[must_use]
    pub fn luminance(self) -> f64 {
        fn channel(v: u8) -> f64 {
            let s = f64::from(v) / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }

    /// WCAG contrast ratio against another colour, 1.0..=21.0.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Nearest xterm-256 index.
    #[must_use]
    pub fn to_256(self) -> u8 {
        // Greys have their own ramp; quantising them through the 6x6x6 cube muddies them.
        if self.0 == self.1 && self.1 == self.2 {
            let g = self.0;
            if g < 8 {
                return 16;
            }
            if g > 248 {
                return 231;
            }
            // 24 grey steps; the arithmetic cannot exceed 255 so the conversion is exact.
            let step = u16::from(g - 8) * 24 / 247;
            return 232u8.saturating_add(u8::try_from(step).unwrap_or(23));
        }
        // Each channel folds to 0..=5, so the index is 16..=231 and always fits.
        let q = |v: u8| -> u16 { u16::from(v) * 5 / 255 };
        let idx = 16 + 36 * q(self.0) + 6 * q(self.1) + q(self.2);
        u8::try_from(idx).unwrap_or(231)
    }

    /// Nearest of the 16 ANSI colours.
    #[must_use]
    pub fn to_16(self) -> u8 {
        let bright = u16::from(self.0) + u16::from(self.1) + u16::from(self.2) > 384;
        let bit = |v: u8| u8::from(v > 110);
        let base = bit(self.0) | (bit(self.1) << 1) | (bit(self.2) << 2);
        if bright { base + 8 } else { base }
    }
}

/// How much colour the terminal can show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorDepth {
    /// No colour at all; structure must still read (`--plain`, `TERM=dumb`, a pipe).
    Mono,
    /// The 16 ANSI colours.
    Ansi16,
    /// xterm-256.
    Ansi256,
    /// 24-bit.
    #[default]
    TrueColor,
}

/// The semantic slots a renderer may ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    /// TUI background.
    Surface,
    /// Ordinary data.
    Text,
    /// Provenance, sampling and view annotations.
    Muted,
    /// Frame and row rules.
    Border,
    /// Key and result title.
    Title,
    /// Header cell background.
    HeaderBg,
    /// Header cell foreground.
    HeaderFg,
    /// JSON property name.
    JsonKey,
    /// JSON string.
    JsonString,
    /// JSON number.
    JsonNumber,
    /// JSON boolean.
    JsonBool,
    /// JSON null.
    JsonNull,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Production marker. Always accompanied by the word PRODUCTION (§7.1).
    Production,
}

/// Which palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeKind {
    /// Penguin Dark.
    #[default]
    Dark,
    /// Penguin Light.
    Light,
    /// High contrast: reverse video and the 16-colour slots only.
    HighContrast,
    /// No colour.
    Mono,
}

/// A resolved palette.
///
/// `depth` governs **colour**; `attributes` governs whether SGR attributes such as bold may
/// be emitted at all. They are separate because §7.2 wants hierarchy to survive on a
/// colourless terminal (bold still marks the header), while PIPE-01 wants a redirected
/// stdout to carry no escape bytes whatsoever. Conflating them either loses the hierarchy or
/// pollutes the pipe.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    kind: ThemeKind,
    depth: ColorDepth,
    attributes: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            kind: ThemeKind::Dark,
            depth: ColorDepth::TrueColor,
            attributes: true,
        }
    }
}

impl Theme {
    /// Build a theme for a palette and a terminal capability.
    #[must_use]
    pub fn new(kind: ThemeKind, depth: ColorDepth) -> Self {
        // Asking for colour on a mono terminal is a contradiction; the terminal wins.
        let kind = if depth == ColorDepth::Mono {
            ThemeKind::Mono
        } else {
            kind
        };
        Self {
            kind,
            depth,
            attributes: true,
        }
    }

    /// `--plain`, `TERM=dumb` or a redirected stdout: no colour **and** no attributes, so
    /// not a single escape byte reaches the sink (§7.2, PIPE-01).
    #[must_use]
    pub fn plain() -> Self {
        Self {
            kind: ThemeKind::Mono,
            depth: ColorDepth::Mono,
            attributes: false,
        }
    }

    /// Whether SGR attributes (bold, reverse) may be emitted.
    #[must_use]
    pub fn attributes(self) -> bool {
        self.attributes
    }

    /// Which palette this is.
    #[must_use]
    pub fn kind(self) -> ThemeKind {
        self.kind
    }
    /// Terminal capability.
    #[must_use]
    pub fn depth(self) -> ColorDepth {
        self.depth
    }

    /// The background this palette assumes, for contrast checking.
    #[must_use]
    pub fn background(self) -> Rgb {
        match self.kind {
            ThemeKind::Dark => Rgb(0x11, 0x18, 0x27),
            ThemeKind::Light | ThemeKind::HighContrast => Rgb(0xFF, 0xFF, 0xFF),
            ThemeKind::Mono => Rgb(0x00, 0x00, 0x00),
        }
    }

    /// The authored colour for a token, before quantisation.
    // A palette is a lookup table: every token gets its own row so the whole palette can be
    // read at a glance. Collapsing rows because two values happen to coincide today would
    // make changing one silently change the other.
    #[allow(clippy::match_same_arms)]
    #[must_use]
    pub fn rgb(self, t: Token) -> Option<Rgb> {
        let hex = match (self.kind, t) {
            (ThemeKind::Mono, _) => return None,
            // ---------------------------------------------------------------- Dark (§7.1)
            (ThemeKind::Dark, Token::Surface) => "#111827",
            (ThemeKind::Dark, Token::Text) => "#E5E7EB",
            (ThemeKind::Dark, Token::Muted) => "#9CA3AF",
            (ThemeKind::Dark, Token::Border) => "#64748B",
            (ThemeKind::Dark, Token::Title) => "#67E8F9",
            (ThemeKind::Dark, Token::HeaderBg) => "#164E63",
            (ThemeKind::Dark, Token::HeaderFg) => "#ECFEFF",
            (ThemeKind::Dark, Token::JsonKey) => "#7DD3FC",
            (ThemeKind::Dark, Token::JsonString) => "#A7F3D0",
            (ThemeKind::Dark, Token::JsonNumber) => "#FDE68A",
            (ThemeKind::Dark, Token::JsonBool) => "#C4B5FD",
            (ThemeKind::Dark, Token::JsonNull) => "#CBD5E1",
            (ThemeKind::Dark, Token::Warning) => "#FBBF24",
            (ThemeKind::Dark, Token::Error) => "#FCA5A5",
            (ThemeKind::Dark, Token::Production) => "#FDA4AF",
            // ---------------------------------------------------------------- Light (R25)
            (ThemeKind::Light, Token::Surface) => "#FFFFFF",
            (ThemeKind::Light, Token::Text) => "#1F2937",
            (ThemeKind::Light, Token::Muted) => "#6B7280",
            (ThemeKind::Light, Token::Border) => "#9CA3AF",
            (ThemeKind::Light, Token::Title) => "#0E7490",
            (ThemeKind::Light, Token::HeaderBg) => "#CFFAFE",
            (ThemeKind::Light, Token::HeaderFg) => "#164E63",
            (ThemeKind::Light, Token::JsonKey) => "#0369A1",
            (ThemeKind::Light, Token::JsonString) => "#047857",
            (ThemeKind::Light, Token::JsonNumber) => "#B45309",
            (ThemeKind::Light, Token::JsonBool) => "#6D28D9",
            (ThemeKind::Light, Token::JsonNull) => "#475569",
            (ThemeKind::Light, Token::Warning) => "#B45309",
            (ThemeKind::Light, Token::Error) => "#B91C1C",
            // Deep enough to be unmistakable on white — the whole point of R25.
            (ThemeKind::Light, Token::Production) => "#BE123C",
            // ---------------------------------------------------------------- High contrast
            (ThemeKind::HighContrast, Token::Surface | Token::HeaderFg) => "#FFFFFF",
            (ThemeKind::HighContrast, Token::HeaderBg) => "#000000",
            (ThemeKind::HighContrast, Token::Text | Token::Border | Token::JsonNull) => "#000000",
            (ThemeKind::HighContrast, Token::Muted) => "#3F3F46",
            (ThemeKind::HighContrast, Token::Title | Token::JsonKey) => "#00417A",
            (ThemeKind::HighContrast, Token::JsonString) => "#00553D",
            (ThemeKind::HighContrast, Token::JsonNumber | Token::Warning) => "#6B3A00",
            (ThemeKind::HighContrast, Token::JsonBool) => "#4B0082",
            (ThemeKind::HighContrast, Token::Error | Token::Production) => "#8B0000",
        };
        Rgb::hex(hex)
    }

    /// SGR parameters for a token at this depth. Empty means "no colour".
    #[must_use]
    pub fn sgr(self, t: Token) -> Vec<u8> {
        let Some(c) = self.rgb(t) else {
            return Vec::new();
        };
        let bg = matches!(t, Token::HeaderBg | Token::Surface);
        match self.depth {
            ColorDepth::Mono => Vec::new(),
            ColorDepth::Ansi16 => {
                let n = c.to_16();
                let base = if bg { 40 } else { 30 };
                vec![base + (n & 7) + if n >= 8 { 60 } else { 0 }]
            }
            ColorDepth::Ansi256 => vec![if bg { 48 } else { 38 }, 5, c.to_256()],
            ColorDepth::TrueColor => vec![if bg { 48 } else { 38 }, 2, c.0, c.1, c.2],
        }
    }
}

/// A styled run of text. `Display` emits SGR only when the theme has colour.
#[derive(Clone, Debug)]
pub struct Styled<'a> {
    /// Text to draw. Already `SafeText`-escaped by the caller.
    pub text: &'a str,
    /// Semantic slot.
    pub token: Token,
    /// Bold.
    pub bold: bool,
    /// Theme in force.
    pub theme: Theme,
}

impl fmt::Display for Styled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut params: Vec<u8> = Vec::new();
        if self.bold && self.theme.attributes() {
            params.push(1);
        }
        params.extend(self.theme.sgr(self.token));
        if params.is_empty() {
            return f.write_str(self.text);
        }
        let joined: Vec<String> = params.iter().map(ToString::to_string).collect();
        // Every run resets, so a style can never bleed into the next row (§7.3).
        write!(f, "\x1b[{}m{}\x1b[0m", joined.join(";"), self.text)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const ALL: [Token; 15] = [
        Token::Surface,
        Token::Text,
        Token::Muted,
        Token::Border,
        Token::Title,
        Token::HeaderBg,
        Token::HeaderFg,
        Token::JsonKey,
        Token::JsonString,
        Token::JsonNumber,
        Token::JsonBool,
        Token::JsonNull,
        Token::Warning,
        Token::Error,
        Token::Production,
    ];

    #[test]
    fn hex_parsing_rejects_junk() {
        assert_eq!(Rgb::hex("#112233"), Some(Rgb(0x11, 0x22, 0x33)));
        for bad in ["112233", "#11223", "#gg2233", "#1122334", ""] {
            assert!(Rgb::hex(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn every_token_is_defined_in_every_coloured_theme() {
        // v2.0 shipped only the dark table; R25 added the rest.
        for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
            let th = Theme::new(kind, ColorDepth::TrueColor);
            for t in ALL {
                assert!(th.rgb(t).is_some(), "{kind:?} is missing {t:?}");
            }
        }
    }

    #[test]
    fn mono_has_no_colour_at_all() {
        let th = Theme::new(ThemeKind::Mono, ColorDepth::TrueColor);
        for t in ALL {
            assert!(th.rgb(t).is_none());
            assert!(th.sgr(t).is_empty());
        }
    }

    #[test]
    fn plain_mode_emits_nothing_for_any_token() {
        // PIPE-01: a redirected stdout must be free of escape bytes.
        let th = Theme::plain();
        assert!(!th.attributes());
        for t in ALL {
            assert!(th.sgr(t).is_empty(), "{t:?} leaked into plain mode");
            let s = Styled {
                text: "x",
                token: t,
                bold: true,
                theme: th,
            }
            .to_string();
            assert_eq!(s, "x", "{t:?} emitted an escape in plain mode");
        }
    }

    #[test]
    fn a_mono_terminal_overrides_a_coloured_theme() {
        // Asking for Dark on a pipe must not emit escapes (PIPE-01).
        let th = Theme::new(ThemeKind::Dark, ColorDepth::Mono);
        assert_eq!(th.kind(), ThemeKind::Mono);
        for t in ALL {
            assert!(
                th.sgr(t).is_empty(),
                "{t:?} leaked colour into a mono terminal"
            );
        }
    }

    #[test]
    fn body_text_meets_wcag_aa_against_its_own_background() {
        for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
            let th = Theme::new(kind, ColorDepth::TrueColor);
            let bg = th.background();
            for t in [
                Token::Text,
                Token::Title,
                Token::Error,
                Token::Warning,
                Token::Production,
            ] {
                let c = th.rgb(t).unwrap();
                let ratio = c.contrast(bg);
                assert!(ratio >= 4.5, "{kind:?} {t:?} contrast {ratio:.2} < 4.5");
            }
            // Muted is deliberately quieter but must still clear the large-text bar.
            let m = th.rgb(Token::Muted).unwrap();
            assert!(m.contrast(bg) >= 3.0, "{kind:?} muted contrast too low");
        }
    }

    #[test]
    fn header_foreground_contrasts_with_header_background() {
        for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
            let th = Theme::new(kind, ColorDepth::TrueColor);
            let (fg, bg) = (
                th.rgb(Token::HeaderFg).unwrap(),
                th.rgb(Token::HeaderBg).unwrap(),
            );
            let ratio = fg.contrast(bg);
            assert!(ratio >= 4.5, "{kind:?} header contrast {ratio:.2} < 4.5");
        }
    }

    #[test]
    fn the_production_marker_is_visible_on_a_light_background() {
        // R25: the dark pink was nearly invisible on white, and this is the one marker that
        // must never be missed.
        let light = Theme::new(ThemeKind::Light, ColorDepth::TrueColor);
        let c = light.rgb(Token::Production).unwrap();
        assert!(c.contrast(Rgb(0xFF, 0xFF, 0xFF)) >= 4.5);
        // The dark palette's value would have failed here, which is why it is not reused.
        let dark_value = Theme::new(ThemeKind::Dark, ColorDepth::TrueColor)
            .rgb(Token::Production)
            .unwrap();
        assert!(
            dark_value.contrast(Rgb(0xFF, 0xFF, 0xFF)) < 4.5,
            "the bug R25 fixed"
        );
    }

    #[test]
    fn json_tokens_stay_distinguishable_after_quantisation() {
        // §7.2: adjacent semantics must remain distinct at every depth.
        let json = [
            Token::JsonKey,
            Token::JsonString,
            Token::JsonNumber,
            Token::JsonBool,
        ];
        for kind in [ThemeKind::Dark, ThemeKind::Light] {
            let th = Theme::new(kind, ColorDepth::Ansi256);
            let mut seen = Vec::new();
            for t in json {
                let idx = th.rgb(t).unwrap().to_256();
                assert!(
                    !seen.contains(&idx),
                    "{kind:?} {t:?} collides at 256 colours"
                );
                seen.push(idx);
            }
        }
    }

    #[test]
    fn sgr_shape_matches_the_depth() {
        let t = Token::Text;
        assert!(
            Theme::new(ThemeKind::Dark, ColorDepth::Mono)
                .sgr(t)
                .is_empty()
        );
        assert_eq!(
            Theme::new(ThemeKind::Dark, ColorDepth::Ansi16).sgr(t).len(),
            1
        );
        assert_eq!(
            Theme::new(ThemeKind::Dark, ColorDepth::Ansi256)
                .sgr(t)
                .len(),
            3
        );
        assert_eq!(
            Theme::new(ThemeKind::Dark, ColorDepth::TrueColor)
                .sgr(t)
                .len(),
            5
        );
    }

    #[test]
    fn background_tokens_use_background_sgr() {
        let th = Theme::new(ThemeKind::Dark, ColorDepth::TrueColor);
        assert_eq!(
            th.sgr(Token::HeaderBg)[0],
            48,
            "header background must set bg, not fg"
        );
        assert_eq!(th.sgr(Token::HeaderFg)[0], 38);
    }

    #[test]
    fn every_styled_run_resets() {
        // §7.3: a style must not bleed into the next row, and a pipe must stay clean.
        let th = Theme::new(ThemeKind::Dark, ColorDepth::TrueColor);
        let s = Styled {
            text: "FIELD",
            token: Token::HeaderFg,
            bold: true,
            theme: th,
        }
        .to_string();
        assert!(s.starts_with("\x1b[1;38;2;"));
        assert!(s.ends_with("\x1b[0m"), "missing reset: {s:?}");
        assert!(s.contains("FIELD"));

        // A colourless terminal keeps bold: that is how the header stays distinguishable
        // from the data once colour is gone (§7.2).
        let mono = Theme::new(ThemeKind::Dark, ColorDepth::Mono);
        let m = Styled {
            text: "FIELD",
            token: Token::HeaderFg,
            bold: true,
            theme: mono,
        }
        .to_string();
        assert_eq!(m, "\x1b[1mFIELD\x1b[0m", "mono keeps hierarchy via bold");
        assert!(!m.contains("38;2"), "but emits no colour");

        // Plain mode is the stricter contract: not one escape byte (PIPE-01).
        let plain = Styled {
            text: "FIELD",
            token: Token::HeaderFg,
            bold: true,
            theme: Theme::plain(),
        }
        .to_string();
        assert_eq!(plain, "FIELD", "plain must emit no escape bytes at all");
    }

    #[test]
    fn grey_quantisation_uses_the_grey_ramp() {
        // Pushing a grey through the colour cube turns it into a muddy colour.
        assert!(Rgb(0x80, 0x80, 0x80).to_256() >= 232);
        assert_eq!(Rgb(0x00, 0x00, 0x00).to_256(), 16);
        assert_eq!(Rgb(0xFF, 0xFF, 0xFF).to_256(), 231);
    }

    #[test]
    fn sixteen_colour_indices_are_in_range() {
        for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
            let th = Theme::new(kind, ColorDepth::Ansi16);
            for t in ALL {
                let p = th.sgr(t);
                assert_eq!(p.len(), 1);
                let v = p[0];
                assert!(
                    (30..=37).contains(&v)
                        || (90..=97).contains(&v)
                        || (40..=47).contains(&v)
                        || (100..=107).contains(&v),
                    "{kind:?} {t:?} produced SGR {v}"
                );
            }
        }
    }
}
