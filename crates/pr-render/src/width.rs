//! Display width (v2.1 §6.2, §14.6, R33, V-C04 groundwork).
//!
//! "Correct width" cannot mean "matches every terminal": East Asian Ambiguous characters,
//! ZWJ emoji and variation selectors are font- and terminal-dependent. §14.6 therefore
//! defines correctness as *consistent under a declared policy*, which is what this module
//! implements — and the table renderer additionally re-anchors each column so a wrong guess
//! cannot break the frame.

use unicode_width::UnicodeWidthChar;

/// How to treat East Asian Ambiguous characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AmbiguousWidth {
    /// One column. The safe default for Latin-centric locales.
    #[default]
    Narrow,
    /// Two columns. Correct for CJK locales, where these characters are drawn wide.
    Wide,
}

/// The width policy in force.
#[derive(Clone, Copy, Debug, Default)]
pub struct WidthPolicy {
    /// Ambiguous-width handling.
    pub ambiguous: AmbiguousWidth,
}

impl WidthPolicy {
    /// Narrow ambiguous characters.
    #[must_use]
    pub fn narrow() -> Self {
        Self {
            ambiguous: AmbiguousWidth::Narrow,
        }
    }
    /// Wide ambiguous characters.
    #[must_use]
    pub fn wide() -> Self {
        Self {
            ambiguous: AmbiguousWidth::Wide,
        }
    }

    /// Derive from the locale, as §14.6 specifies.
    #[must_use]
    pub fn from_locale(lang: Option<&str>) -> Self {
        let cjk = lang.is_some_and(|l| {
            let l = l.to_ascii_lowercase();
            l.starts_with("zh") || l.starts_with("ja") || l.starts_with("ko")
        });
        if cjk { Self::wide() } else { Self::narrow() }
    }

    /// Width of one character under this policy.
    #[must_use]
    pub fn char_width(self, c: char) -> usize {
        // A ZWJ or variation selector contributes nothing on its own; the cluster is
        // measured by its base character.
        if is_zero_width(c) {
            return 0;
        }
        match self.ambiguous {
            AmbiguousWidth::Wide => c.width_cjk().unwrap_or(0),
            AmbiguousWidth::Narrow => c.width().unwrap_or(0),
        }
    }

    /// Display width of a string, measured by grapheme cluster.
    ///
    /// A ZWJ sequence counts as one wide cluster rather than the sum of its parts, which is
    /// what a terminal actually draws for a family emoji.
    #[must_use]
    pub fn str_width(self, s: &str) -> usize {
        let mut total = 0usize;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if is_zero_width(c) {
                continue;
            }
            let mut w = self.char_width(c);
            // Absorb the rest of this cluster.
            while let Some(&next) = chars.peek() {
                if next == '\u{200d}' {
                    chars.next();
                    // The character joined on does not add width.
                    if chars.peek().is_some() {
                        chars.next();
                    }
                    w = w.max(2);
                    continue;
                }
                if is_zero_width(next) {
                    chars.next();
                    continue;
                }
                break;
            }
            total += w;
        }
        total
    }

    /// Truncate to at most `max` columns, returning the text and its width.
    ///
    /// Never splits a cluster, so truncation cannot produce a lone combining mark.
    #[must_use]
    pub fn truncate(self, s: &str, max: usize) -> (String, usize) {
        if self.str_width(s) <= max {
            let w = self.str_width(s);
            return (s.to_owned(), w);
        }
        let mut out = String::new();
        let mut w = 0usize;
        for c in s.chars() {
            let cw = self.char_width(c);
            if w + cw > max {
                break;
            }
            out.push(c);
            w += cw;
        }
        (out, w)
    }
}

/// Characters that occupy no columns of their own.
fn is_zero_width(c: char) -> bool {
    matches!(u32::from(c),
        0x200B..=0x200F | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0100..=0xE01EF)
        || matches!(c.width(), Some(0) | None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_one_column_each() {
        let p = WidthPolicy::narrow();
        assert_eq!(p.str_width("HGETALL"), 7);
        assert_eq!(p.str_width(""), 0);
        assert_eq!(p.str_width("player:10001"), 12);
    }

    #[test]
    fn cjk_is_two_columns_each() {
        let p = WidthPolicy::narrow();
        assert_eq!(p.str_width("中文"), 4);
        assert_eq!(p.str_width("中文字段"), 8);
    }

    #[test]
    fn emoji_is_two_columns() {
        let p = WidthPolicy::narrow();
        assert_eq!(p.str_width("🐧"), 2);
        assert_eq!(p.str_width("a🐧b"), 4);
    }

    #[test]
    fn a_zwj_family_is_one_cluster_not_the_sum_of_its_parts() {
        // ASSIST-018: summing the parts would give 6 and break every frame on the row.
        let p = WidthPolicy::narrow();
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        assert_eq!(p.str_width(family), 2);
        assert_eq!(p.str_width(&format!("a{family}b")), 4);
    }

    #[test]
    fn combining_marks_add_nothing() {
        let p = WidthPolicy::narrow();
        assert_eq!(
            p.str_width("e\u{301}"),
            1,
            "e plus combining acute is one column"
        );
        assert_eq!(p.str_width("a\u{300}\u{301}\u{302}"), 1);
    }

    #[test]
    fn variation_selectors_and_skin_tones_add_nothing() {
        let p = WidthPolicy::narrow();
        assert_eq!(p.str_width("\u{2764}\u{fe0f}"), p.str_width("\u{2764}"));
        let waving = "\u{1f44b}\u{1f3fd}";
        assert_eq!(
            p.str_width(waving),
            2,
            "base emoji plus skin tone is still one cluster"
        );
    }

    #[test]
    fn ambiguous_width_follows_the_policy() {
        // R33: this is the configurable part, and both answers are legitimate.
        let narrow = WidthPolicy::narrow();
        let wide = WidthPolicy::wide();
        // U+00B1 PLUS-MINUS is East Asian Ambiguous.
        assert_eq!(narrow.str_width("\u{b1}"), 1);
        assert_eq!(wide.str_width("\u{b1}"), 2);
    }

    #[test]
    fn locale_selects_the_policy() {
        assert_eq!(
            WidthPolicy::from_locale(Some("zh_CN.UTF-8")).ambiguous,
            AmbiguousWidth::Wide
        );
        assert_eq!(
            WidthPolicy::from_locale(Some("ja_JP")).ambiguous,
            AmbiguousWidth::Wide
        );
        assert_eq!(
            WidthPolicy::from_locale(Some("ko_KR")).ambiguous,
            AmbiguousWidth::Wide
        );
        assert_eq!(
            WidthPolicy::from_locale(Some("en_US.UTF-8")).ambiguous,
            AmbiguousWidth::Narrow
        );
        assert_eq!(
            WidthPolicy::from_locale(None).ambiguous,
            AmbiguousWidth::Narrow
        );
    }

    #[test]
    fn control_characters_contribute_nothing() {
        // They are escaped by SafeText before they reach a renderer, but measuring them as
        // zero keeps the two layers consistent if one is bypassed.
        let p = WidthPolicy::narrow();
        assert_eq!(p.str_width("\u{1}\u{2}"), 0);
    }

    #[test]
    fn truncation_never_splits_a_cluster() {
        let p = WidthPolicy::narrow();
        let (s, w) = p.truncate("中文字段", 5);
        assert_eq!(w, 4, "cannot use the 5th column without splitting 段");
        assert_eq!(s, "中文");
        assert!(p.str_width(&s) <= 5);

        let (s2, w2) = p.truncate("abcdef", 3);
        assert_eq!((s2.as_str(), w2), ("abc", 3));

        let (s3, w3) = p.truncate("short", 99);
        assert_eq!((s3.as_str(), w3), ("short", 5));
    }

    #[test]
    fn truncation_result_always_fits() {
        let p = WidthPolicy::narrow();
        for s in [
            "中文字段",
            "a🐧b",
            "e\u{301}x",
            "plain",
            "\u{1f468}\u{200d}\u{1f469}",
        ] {
            for max in 0..10 {
                let (t, w) = p.truncate(s, max);
                assert!(w <= max, "{s:?} truncated to {max} gave width {w}");
                assert_eq!(p.str_width(&t), w, "reported width disagrees for {s:?}");
            }
        }
    }

    #[test]
    fn width_is_stable_under_repetition() {
        let p = WidthPolicy::narrow();
        let unit = "中a🐧";
        let w = p.str_width(unit);
        assert_eq!(
            p.str_width(&unit.repeat(5)),
            w * 5,
            "width must be additive"
        );
    }
}
