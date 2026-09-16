//! The Table design system (v2.1 §6, ADR-004, V-E02).
//!
//! This is the visual contract the user signed off on, so it is spelled out rather than
//! left to a generic table crate:
//! - a rule between **every** logical field (§6.1), never between the lines of one cell
//! - a header separated by a *double* rule, so hierarchy survives with colour off (UX-04)
//! - JSON expanded inside the value cell, its lines belonging to one logical row (UX-05)
//! - widths from a bounded sample and then frozen, because a stream cannot wait for the last
//!   row to choose a layout (§6.2)
//! - nothing hidden without saying so: narrow terminals restack rather than silently clip
//!   (UX-07)

use crate::theme::{Styled, Theme, Token};
use crate::width::WidthPolicy;
use pr_core::SafeText;

/// What a cell holds.
#[derive(Clone, Debug)]
pub enum Cell {
    /// A single line of text.
    Text(SafeText),
    /// Pre-formatted lines that belong to one logical row — pretty JSON, a hex dump.
    /// No rule is ever drawn between them (§6.3).
    Lines(Vec<SafeText>),
    /// A nil reply, distinct from an empty string (§6.5).
    Nil,
    /// An empty string, distinct from nil.
    Empty,
    /// A column this row does not have.
    Missing,
}

impl Cell {
    /// Plain text from a `&str`.
    #[must_use]
    pub fn text(s: &str) -> Self {
        Self::Text(SafeText::from_str_escaped(s))
    }

    /// The lines this cell occupies, with annotations rendered.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Text(t) => vec![t.as_display().to_owned()],
            Self::Lines(v) => v.iter().map(|t| t.as_display().to_owned()).collect(),
            // Annotations are visually distinct from a literal value that happens to read
            // the same way (§6.5): a real "(nil)" string renders with quotes.
            Self::Nil => vec!["(nil)".to_owned()],
            Self::Empty => vec!["(empty)".to_owned()],
            Self::Missing => vec!["(missing)".to_owned()],
        }
    }

    /// Whether this cell is an annotation rather than data.
    #[must_use]
    pub fn is_annotation(&self) -> bool {
        matches!(self, Self::Nil | Self::Empty | Self::Missing)
    }

    /// Height in lines.
    #[must_use]
    pub fn height(&self) -> usize {
        match self {
            Self::Lines(v) => v.len().max(1),
            _ => 1,
        }
    }
}

/// Horizontal alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Left, the default for everything (§6.1).
    #[default]
    Left,
    /// Right. Only for genuine protocol numbers, never for a byte string that looks numeric.
    Right,
}

/// A column definition.
#[derive(Clone, Debug)]
pub struct Column {
    /// Header text as it appears.
    pub title: String,
    /// Alignment for the body.
    pub align: Align,
}

impl Column {
    /// A left-aligned column.
    #[must_use]
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            align: Align::Left,
        }
    }
    /// A right-aligned column, for protocol numbers only.
    #[must_use]
    pub fn numeric(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            align: Align::Right,
        }
    }
}

/// How the table was laid out when the terminal is too narrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Normal side-by-side columns.
    Columns,
    /// One block per row with field labels, used when columns cannot fit (§6.2).
    Stacked,
}

/// How cell boundaries are positioned (v2.1 §14.6).
///
/// Padding with spaces is correct only while the width policy agrees with the terminal. They
/// disagree in practice — East Asian Ambiguous characters, ZWJ sequences and variation
/// selectors are rendered differently by different terminals and fonts — and when they do, a
/// padded table's right-hand border walks further out of line with every row.
///
/// [`DrawMode::CursorReset`] is §14.6's stated fallback: every cell boundary is reached by an
/// absolute cursor move, so the frame stays aligned **even when the width judgement is
/// wrong**. The content inside a cell may still be too wide or too narrow; the box around it
/// will not be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawMode {
    /// Pad with spaces. The default, and correct when the policy matches the terminal.
    #[default]
    Padded,
    /// Reposition the cursor absolutely at every cell boundary.
    CursorReset,
}

/// A rendered table plus what the user must be told about it.
#[derive(Clone, Debug)]
pub struct Rendered {
    /// The lines to print.
    pub lines: Vec<String>,
    /// Which layout was chosen.
    pub layout: Layout,
    /// Column widths actually used.
    pub widths: Vec<usize>,
    /// True if any cell was truncated — the footer must say so (§6.1).
    pub truncated: bool,
}

/// The table.
#[derive(Clone, Debug)]
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
    /// Widths frozen after the sample; `None` until the first render.
    frozen: Option<Vec<usize>>,
}

/// Minimum useful width for a column's content.
const MIN_CONTENT: usize = 3;
/// How many rows to sample before committing to a layout (§6.2).
pub const SAMPLE_ROWS: usize = 100;

impl Table {
    /// A table with the given columns.
    #[must_use]
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            frozen: None,
        }
    }

    /// The canonical Field / Value table for a hash (§5.2).
    #[must_use]
    pub fn field_value() -> Self {
        Self::new(vec![Column::new("FIELD"), Column::new("VALUE")])
    }

    /// Append a row. Short rows are padded with `Missing`, so a ragged reply is visible
    /// rather than silently squared off (§8.5).
    pub fn push(&mut self, mut row: Vec<Cell>) {
        while row.len() < self.columns.len() {
            row.push(Cell::Missing);
        }
        row.truncate(self.columns.len());
        self.rows.push(row);
    }

    /// Number of logical rows. This is what a footer may report as `returned`; it is never
    /// the number of printed lines (§6.4).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Natural width of each column from a bounded sample.
    fn sample_widths(&self, p: WidthPolicy) -> Vec<usize> {
        let mut w: Vec<usize> = self.columns.iter().map(|c| p.str_width(&c.title)).collect();
        for row in self.rows.iter().take(SAMPLE_ROWS) {
            for (i, cell) in row.iter().enumerate() {
                let widest = cell
                    .lines()
                    .iter()
                    .map(|l| p.str_width(l))
                    .max()
                    .unwrap_or(0);
                if let Some(slot) = w.get_mut(i) {
                    *slot = (*slot).max(widest);
                }
            }
        }
        w
    }

    /// Fit the natural widths into `total` columns.
    ///
    /// The last column absorbs the remainder rather than every column shrinking equally,
    /// because in a Field/Value table the value is what deserves the space (§6.2).
    fn fit(&self, natural: &[usize], total: usize) -> Option<Vec<usize>> {
        let n = self.columns.len();
        if n == 0 {
            return None;
        }
        // Frame overhead: one border per edge, plus a separator per gap, plus one space of
        // padding on each side of every cell.
        let overhead = (n + 1) + 2 * n;
        let content = total.checked_sub(overhead)?;
        if content < n * MIN_CONTENT {
            return None; // cannot fit; caller restacks
        }
        let want: usize = natural.iter().sum();
        if want <= content {
            return Some(natural.to_vec());
        }
        // Shrink from the left, keeping each column at least MIN_CONTENT.
        let mut out = natural.to_vec();
        let mut excess = want - content;
        let last_index = n - 1;
        for slot in out.iter_mut().take(last_index) {
            if excess == 0 {
                break;
            }
            let can = slot.saturating_sub(MIN_CONTENT);
            let take = can.min(excess);
            *slot -= take;
            excess -= take;
        }
        if excess > 0 {
            let can = out[last_index].saturating_sub(MIN_CONTENT);
            out[last_index] -= can.min(excess);
        }
        Some(out)
    }

    /// Render at `total_cols` wide.
    ///
    /// The first render freezes the widths, so later rows in a stream wrap instead of
    /// rewriting rows already printed (§6.2).
    pub fn render(&mut self, total_cols: usize, theme: Theme, p: WidthPolicy) -> Rendered {
        self.render_with(total_cols, theme, p, DrawMode::Padded)
    }

    /// Render, choosing how cell boundaries are positioned (§14.6).
    ///
    /// `DrawMode::CursorReset` is what a width probe switches to when it finds the terminal
    /// disagrees with the policy.
    #[allow(clippy::too_many_lines)]
    pub fn render_with(
        &mut self,
        total_cols: usize,
        theme: Theme,
        p: WidthPolicy,
        mode: DrawMode,
    ) -> Rendered {
        let natural = self.frozen.clone().unwrap_or_else(|| self.sample_widths(p));
        let Some(widths) = self.fit(&natural, total_cols) else {
            return self.render_stacked(theme, p);
        };
        if self.frozen.is_none() {
            self.frozen = Some(widths.clone());
        }
        let mut truncated = false;
        let mut lines = Vec::new();

        let rule = |l: char, mid: char, r: char, fill: char, w: &[usize]| -> String {
            let mut s = String::new();
            s.push(l);
            for (i, width) in w.iter().enumerate() {
                for _ in 0..width + 2 {
                    s.push(fill);
                }
                s.push(if i + 1 == w.len() { r } else { mid });
            }
            s
        };

        // Column of every vertical border, so `CursorReset` can go straight to it:
        // `│` + space + content + space, repeated.
        let mut bounds = vec![0usize];
        for w in &widths {
            let last = *bounds.last().unwrap_or(&0);
            bounds.push(last + w + 3);
        }
        let at = move |i: usize| -> String {
            match mode {
                DrawMode::Padded => String::new(),
                DrawMode::CursorReset => {
                    let col = bounds[i];
                    if col == 0 {
                        "\r".to_owned()
                    } else {
                        format!("\r\x1b[{col}C")
                    }
                }
            }
        };

        lines.push(rule('╭', '┬', '╮', '─', &widths));

        // Header. Bold plus a double rule, so hierarchy survives with colour off (UX-04).
        let mut head = at(0);
        head.push('│');
        for (i, c) in self.columns.iter().enumerate() {
            let w = widths[i];
            let (text, tw) = p.truncate(&c.title, w);
            truncated |= tw < p.str_width(&c.title);
            head.push(' ');
            let styled = Styled {
                text: &text,
                token: Token::HeaderFg,
                bold: true,
                theme,
            };
            head.push_str(&styled.to_string());
            for _ in 0..w.saturating_sub(tw) {
                head.push(' ');
            }
            head.push(' ');
            head.push_str(&at(i + 1));
            head.push('│');
        }
        lines.push(head);
        lines.push(rule('╞', '╪', '╡', '═', &widths));

        for (ri, row) in self.rows.iter().enumerate() {
            let height = row.iter().map(Cell::height).max().unwrap_or(1);
            for li in 0..height {
                let mut line = at(0);
                line.push('│');
                for (ci, cell) in row.iter().enumerate() {
                    let w = widths[ci];
                    let cl = cell.lines();
                    let raw = cl.get(li).map_or("", String::as_str);
                    let (text, tw) = p.truncate(raw, w);
                    truncated |= tw < p.str_width(raw);
                    let pad = w.saturating_sub(tw);
                    let token = if cell.is_annotation() {
                        Token::Muted
                    } else {
                        Token::Text
                    };
                    let styled = Styled {
                        text: &text,
                        token,
                        bold: false,
                        theme,
                    };
                    line.push(' ');
                    if self.columns[ci].align == Align::Right {
                        for _ in 0..pad {
                            line.push(' ');
                        }
                        line.push_str(&styled.to_string());
                    } else {
                        line.push_str(&styled.to_string());
                        for _ in 0..pad {
                            line.push(' ');
                        }
                    }
                    line.push(' ');
                    line.push_str(&at(ci + 1));
                    line.push('│');
                }
                lines.push(line);
            }
            // A rule between logical fields only — never between the lines of one cell.
            if ri + 1 < self.rows.len() {
                lines.push(rule('├', '┼', '┤', '─', &widths));
            }
        }
        lines.push(rule('╰', '┴', '╯', '─', &widths));

        Rendered {
            lines,
            layout: Layout::Columns,
            widths,
            truncated,
        }
    }

    /// Vertical blocks, used when columns cannot fit (§6.2). Nothing is hidden: every column
    /// still appears, labelled.
    fn render_stacked(&self, theme: Theme, p: WidthPolicy) -> Rendered {
        let mut lines = Vec::new();
        let label_w = self
            .columns
            .iter()
            .map(|c| p.str_width(&c.title))
            .max()
            .unwrap_or(0);
        for (ri, row) in self.rows.iter().enumerate() {
            if ri > 0 {
                lines.push(String::new());
            }
            for (ci, cell) in row.iter().enumerate() {
                let title = &self.columns[ci].title;
                let pad = label_w.saturating_sub(p.str_width(title));
                let styled = Styled {
                    text: title,
                    token: Token::HeaderFg,
                    bold: true,
                    theme,
                };
                for (li, l) in cell.lines().iter().enumerate() {
                    if li == 0 {
                        lines.push(format!("{}{} {}", styled, " ".repeat(pad), l));
                    } else {
                        lines.push(format!("{} {}", " ".repeat(label_w), l));
                    }
                }
            }
        }
        Rendered {
            lines,
            layout: Layout::Stacked,
            widths: vec![label_w],
            truncated: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::theme::{ColorDepth, ThemeKind};

    fn plain() -> Theme {
        Theme::plain()
    }
    fn pol() -> WidthPolicy {
        WidthPolicy::narrow()
    }

    fn hash_table() -> Table {
        let mut t = Table::field_value();
        t.push(vec![Cell::text("playerId"), Cell::text("10001")]);
        t.push(vec![Cell::text("username"), Cell::text("shieng")]);
        t.push(vec![Cell::text("status"), Cell::text("ACTIVE")]);
        t
    }

    #[test]
    fn a_rule_separates_every_logical_field() {
        // UX-03 / §6.1: the visual rule the user explicitly confirmed.
        let mut t = hash_table();
        let r = t.render(60, plain(), pol());
        let rules = r.lines.iter().filter(|l| l.starts_with('├')).count();
        assert_eq!(rules, 2, "3 rows need 2 separators between them");
        assert!(r.lines.first().unwrap().starts_with('╭'));
        assert!(r.lines.last().unwrap().starts_with('╰'));
    }

    #[test]
    fn the_header_uses_a_double_rule() {
        // UX-04: hierarchy must survive with colour off, so it cannot rely on colour.
        let mut t = hash_table();
        let r = t.render(60, plain(), pol());
        let double = r.lines.iter().filter(|l| l.starts_with('╞')).count();
        assert_eq!(double, 1);
        // lines: 0 = top border, 1 = header, 2 = double rule, 3+ = data
        assert!(
            r.lines[2].starts_with('╞'),
            "double rule sits directly under the header"
        );
        // And the single rule used between data rows is a different character.
        assert!(r.lines.iter().any(|l| l.starts_with('├')));
    }

    #[test]
    fn multiline_json_is_one_logical_row_with_no_internal_rules() {
        // UX-05: the JSON lines belong to one field; a rule between them would read as
        // extra fields.
        let mut t = Table::field_value();
        t.push(vec![
            Cell::text("config"),
            Cell::Lines(
                ["{", "  \"enabled\": true,", "  \"theme\": \"dark\"", "}"]
                    .iter()
                    .map(|s| SafeText::from_str_escaped(s))
                    .collect(),
            ),
        ]);
        t.push(vec![Cell::text("status"), Cell::text("ACTIVE")]);
        let r = t.render(60, plain(), pol());

        let rules = r.lines.iter().filter(|l| l.starts_with('├')).count();
        assert_eq!(rules, 1, "only between the two fields, not inside the JSON");
        assert_eq!(
            t.row_count(),
            2,
            "two logical rows regardless of printed height"
        );
        // The JSON body occupies four consecutive content lines.
        let body: Vec<&String> = r.lines.iter().filter(|l| l.starts_with('│')).collect();
        assert!(body.len() >= 6, "header + 4 json lines + status");
    }

    #[test]
    fn nil_empty_and_missing_are_distinguishable() {
        // §6.5 / CMD-04: nil and empty string are different answers.
        let mut t = Table::field_value();
        t.push(vec![Cell::text("a"), Cell::Nil]);
        t.push(vec![Cell::text("b"), Cell::Empty]);
        t.push(vec![Cell::text("c")]); // short row -> Missing
        let r = t.render(60, plain(), pol());
        let text = r.lines.join("\n");
        assert!(text.contains("(nil)"));
        assert!(text.contains("(empty)"));
        assert!(text.contains("(missing)"));
        assert_eq!(t.rows[2][1].lines(), vec!["(missing)"]);
    }

    #[test]
    fn a_literal_nil_string_is_not_confused_with_the_annotation() {
        // §6.5: a value that genuinely reads "(nil)" must not look like the annotation.
        let mut t = Table::field_value();
        t.push(vec![Cell::text("k"), Cell::text("(nil)")]);
        assert!(
            !t.rows[0][1].is_annotation(),
            "a real string is data, not an annotation"
        );
        assert!(Cell::Nil.is_annotation());
    }

    #[test]
    fn widths_freeze_after_the_first_render() {
        // §6.2: a stream cannot rewrite rows it has already printed.
        let mut t = hash_table();
        let first = t.render(60, plain(), pol());
        t.push(vec![
            Cell::text("a-very-long-field-name-arriving-later"),
            Cell::text("x"),
        ]);
        let second = t.render(60, plain(), pol());
        assert_eq!(
            first.widths, second.widths,
            "layout must not shift mid-stream"
        );
    }

    #[test]
    fn a_narrow_terminal_restacks_rather_than_hiding_columns() {
        // UX-07: nothing disappears without saying so.
        let mut t = hash_table();
        let r = t.render(12, plain(), pol());
        assert_eq!(r.layout, Layout::Stacked);
        let text = r.lines.join("\n");
        for expected in ["FIELD", "VALUE", "playerId", "10001", "status", "ACTIVE"] {
            assert!(text.contains(expected), "stacked layout dropped {expected}");
        }
    }

    #[test]
    fn wide_characters_keep_the_frame_aligned() {
        // UX-08: every printed line must be the same display width.
        let mut t = Table::field_value();
        t.push(vec![Cell::text("中文字段"), Cell::text("值")]);
        t.push(vec![Cell::text("emoji"), Cell::text("a🐧b")]);
        t.push(vec![Cell::text("ascii"), Cell::text("plain")]);
        let r = t.render(60, plain(), pol());
        let p = pol();
        let widths: Vec<usize> = r.lines.iter().map(|l| p.str_width(l)).collect();
        let first = widths[0];
        for (i, w) in widths.iter().enumerate() {
            assert_eq!(
                *w, first,
                "line {i} is {w} columns, expected {first}: {:?}",
                r.lines[i]
            );
        }
    }

    #[test]
    fn a_combining_mark_does_not_widen_a_row() {
        let mut t = Table::field_value();
        t.push(vec![Cell::text("e\u{301}"), Cell::text("x")]);
        t.push(vec![Cell::text("e"), Cell::text("x")]);
        let r = t.render(40, plain(), pol());
        let p = pol();
        let ws: Vec<usize> = r.lines.iter().map(|l| p.str_width(l)).collect();
        assert!(
            ws.windows(2).all(|w| w[0] == w[1]),
            "frame misaligned: {ws:?}"
        );
    }

    #[test]
    fn control_bytes_in_a_value_cannot_reach_the_terminal() {
        // SEC-05: a value must not be able to drive the terminal through the table.
        let mut t = Table::field_value();
        t.push(vec![Cell::text("k"), Cell::text("\u{1b}]0;evil\u{7}")]);
        let r = t.render(60, plain(), pol());
        let text = r.lines.join("\n");
        assert!(!text.contains('\u{1b}'), "escape leaked into the table");
        assert!(!text.contains('\u{7}'));
        assert!(text.contains("\\x1b"), "it is shown escaped instead");
    }

    #[test]
    fn plain_mode_produces_no_escape_bytes_at_all() {
        // PIPE-01.
        let mut t = hash_table();
        let r = t.render(60, Theme::plain(), pol());
        for l in &r.lines {
            assert!(!l.contains('\u{1b}'), "plain mode emitted an escape: {l:?}");
        }
    }

    #[test]
    fn coloured_mode_resets_every_run() {
        // §7.3: a header background must not bleed onto the next row.
        let mut t = hash_table();
        let theme = Theme::new(ThemeKind::Dark, ColorDepth::TrueColor);
        let r = t.render(60, theme, pol());
        for l in &r.lines {
            let opens = l.matches("\u{1b}[").count();
            let resets = l.matches("\u{1b}[0m").count();
            // Every opening sequence is matched by a reset (the reset itself counts as one
            // opening, hence the halving).
            assert_eq!(opens, resets * 2, "unbalanced styling on {l:?}");
        }
    }

    #[test]
    fn numeric_columns_right_align_but_text_does_not() {
        // §6.1: only genuine protocol numbers align right; 70 as a hash value is a string.
        let mut t = Table::new(vec![Column::new("MEMBER"), Column::numeric("SCORE")]);
        t.push(vec![Cell::text("player:1"), Cell::text("92881")]);
        t.push(vec![Cell::text("p"), Cell::text("1")]);
        let r = t.render(40, plain(), pol());
        let body: Vec<&String> = r
            .lines
            .iter()
            .filter(|l| l.starts_with('│'))
            .skip(1)
            .collect();
        // The short score is padded on its left.
        assert!(
            body[1].contains("    1 │") || body[1].contains(" 1 │"),
            "{:?}",
            body[1]
        );
    }

    #[test]
    fn truncation_is_reported_rather_than_silent() {
        // §6.1 footer rule: the user is told when something was cut.
        let mut t = Table::field_value();
        t.push(vec![
            Cell::text("k"),
            Cell::text("a-very-long-value-that-cannot-possibly-fit-in-this-narrow-table"),
        ]);
        let r = t.render(30, plain(), pol());
        assert_eq!(r.layout, Layout::Columns);
        assert!(r.truncated, "truncation must be reported");
    }

    #[test]
    fn an_empty_table_still_renders_a_frame() {
        let mut t = Table::field_value();
        let r = t.render(40, plain(), pol());
        assert_eq!(t.row_count(), 0);
        assert!(r.lines.len() >= 4, "top, header, double rule, bottom");
        assert!(r.lines.last().unwrap().starts_with('╰'));
    }

    #[test]
    fn row_count_is_logical_not_printed_lines() {
        // §6.4: "7 fields returned" must not become "11" because JSON expanded.
        let mut t = Table::field_value();
        t.push(vec![
            Cell::text("config"),
            Cell::Lines(
                (0..8)
                    .map(|i| SafeText::from_str_escaped(&format!("line {i}")))
                    .collect(),
            ),
        ]);
        assert_eq!(t.row_count(), 1);
        let r = t.render(40, plain(), pol());
        assert!(r.lines.len() > 8);
    }

    #[test]
    fn extra_cells_beyond_the_column_count_are_dropped_not_misaligned() {
        let mut t = Table::field_value();
        t.push(vec![Cell::text("a"), Cell::text("b"), Cell::text("c")]);
        assert_eq!(t.rows[0].len(), 2);
        let r = t.render(40, plain(), pol());
        let p = pol();
        let ws: Vec<usize> = r.lines.iter().map(|l| p.str_width(l)).collect();
        assert!(ws.windows(2).all(|w| w[0] == w[1]));
    }
}
