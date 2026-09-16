//! V-C04 — the Unicode width matrix (v2.1 §14.6, R33, UX-08, ASSIST-018/067).
//!
//! §14.6 does not claim that display width can be got right everywhere. It says the opposite:
//! terminals and fonts disagree about East Asian Ambiguous characters, ZWJ sequences and
//! variation selectors, so "correct width" means *consistent with a declared policy*. What the
//! spec then owes the user is a way out when the policy is wrong for their terminal, and that
//! is the pair these tests exercise:
//!
//! - a **probe** (`CSI 6 n`) that measures what the terminal actually does, and
//! - a **fallback** drawing mode that keeps the frame aligned even when the width judgement is
//!   wrong.
//!
//! The fallback cannot be tested by rendering and reading back our own output — that only
//! proves the renderer agrees with itself. So these tests render under one width model and
//! *display* under a different one, using the harness screen's `CellWidth`. That is the real
//! failure: a CJK terminal, a narrow policy, and a table whose right-hand border walks a
//! little further out of line with every row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_render::{Cell, Column, DrawMode, Table, Theme, WidthPolicy};
use pty_harness::Screen;
use pty_harness::screen::CellWidth;

/// Render a table and show it on a screen that may measure characters differently.
fn display(
    table: &mut Table,
    policy: WidthPolicy,
    mode: DrawMode,
    screen_width: CellWidth,
) -> Screen {
    let rendered = table.render_with(100, Theme::plain(), policy, mode);
    let mut s = Screen::with_width(100, 40, screen_width);
    for line in &rendered.lines {
        s.feed(line.as_bytes());
        s.feed(b"\r\n");
    }
    s
}

/// The column of every vertical border on each drawn row.
///
/// A table is aligned exactly when this is the same list for every row.
fn border_columns(s: &Screen) -> Vec<Vec<usize>> {
    s.lines()
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains('│'))
        .map(|(row, _)| s.columns_at(row, '│'))
        .collect()
}

fn table_with(values: &[&str]) -> Table {
    let mut t = Table::new(vec![Column::new("FIELD"), Column::new("VALUE")]);
    for (i, v) in values.iter().enumerate() {
        t.push(vec![Cell::text(&format!("f{i}")), Cell::text(v)]);
    }
    t
}

// ================================================ UX-08
#[test]
fn ux_08_a_table_of_mixed_scripts_is_aligned_when_the_policy_matches() {
    // Chinese, emoji, a combining mark and plain ASCII in one table. With the policy and the
    // terminal agreeing, every border sits in the same column on every row.
    let mut t = table_with(&[
        "plain ascii",
        "中文字符串",
        "penguin 🐧 emoji",
        "cafe\u{301} combining",
        "mixed 中 abc 🐧",
    ]);
    let s = display(
        &mut t,
        WidthPolicy::wide(),
        DrawMode::Padded,
        CellWidth::UnicodeWide,
    );

    let rows = border_columns(&s);
    assert!(
        rows.len() >= 6,
        "header plus five data rows: {}",
        rows.len()
    );
    let first = &rows[0];
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(
            r,
            first,
            "row {i} borders at {r:?} instead of {first:?}\n{}",
            s.text()
        );
    }

    // And the content really is on screen, not mangled by the width handling.
    assert!(s.shows("中文字符串"), "{}", s.text());
    assert!(s.shows("penguin 🐧 emoji"));
}

#[test]
fn ux_08_the_rules_line_up_with_the_borders() {
    // The horizontal rules are built from fixed-width box characters, so if the *content*
    // rows drift the mismatch shows up here first.
    let mut t = table_with(&["中文", "abcd"]);
    let s = display(
        &mut t,
        WidthPolicy::wide(),
        DrawMode::Padded,
        CellWidth::UnicodeWide,
    );
    // Occupied columns, not character count: the two differ the moment a wide character
    // appears, and it is the columns that have to line up.
    let widths: Vec<usize> = (0..s.lines().len())
        .filter(|r| !s.line(*r).is_empty())
        .filter_map(|r| s.last_column(r))
        .collect();
    let first = widths[0];
    assert!(
        widths.iter().all(|w| *w == first),
        "the frame is ragged: {widths:?}\n{}",
        s.text()
    );
}

// ================================================ the §14.6 fallback
#[test]
fn a_padded_table_misaligns_when_the_terminal_disagrees_about_width() {
    // This is the failure, demonstrated rather than asserted about: narrow policy, CJK
    // terminal. Without it, the next test would be proving nothing.
    let mut t = table_with(&["→→→ arrows", "plain text", "±±± signs"]);
    let s = display(
        &mut t,
        WidthPolicy::narrow(),
        DrawMode::Padded,
        CellWidth::UnicodeWide,
    );
    let rows = border_columns(&s);
    assert!(
        rows.iter().any(|r| r != &rows[0]),
        "the fixture failed to produce a misalignment, so the fallback test below is vacuous\n{}",
        s.text()
    );
}

#[test]
fn cursor_reset_keeps_the_frame_aligned_when_the_width_judgement_is_wrong() {
    // §14.6: "若探测结果与策略矛盾，表格改用每格后强制重置列位置的绘制方式，保证框线对齐，
    // 即使内容宽度判定错误."
    let mut t = table_with(&["→→→ arrows", "plain text", "±±± signs"]);
    let s = display(
        &mut t,
        WidthPolicy::narrow(),
        DrawMode::CursorReset,
        CellWidth::UnicodeWide,
    );

    let rows = border_columns(&s);
    assert!(rows.len() >= 4);
    let first = &rows[0];
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(
            r,
            first,
            "row {i} borders at {r:?} instead of {first:?} — the fallback did not hold\n{}",
            s.text()
        );
    }
}

#[test]
fn cursor_reset_holds_for_wide_characters_too() {
    // The severe case: content that is two columns wider per character than assumed.
    let mut t = table_with(&["中文中文中文", "ascii", "🐧🐧🐧"]);
    let mut narrow_lying = WidthPolicy::narrow();
    narrow_lying.ambiguous = pr_render::AmbiguousWidth::Narrow;
    let s = display(
        &mut t,
        narrow_lying,
        DrawMode::CursorReset,
        CellWidth::UnicodeWide,
    );
    let rows = border_columns(&s);
    let first = &rows[0];
    for r in &rows {
        assert_eq!(r, first, "{}", s.text());
    }
}

#[test]
fn cursor_reset_changes_nothing_when_the_policy_is_right() {
    // The fallback must not be a different table. Same content, same policy, same terminal:
    // the two modes put the borders in the same places.
    let values = ["中文", "abc", "🐧 x"];
    let padded = display(
        &mut table_with(&values),
        WidthPolicy::wide(),
        DrawMode::Padded,
        CellWidth::UnicodeWide,
    );
    let reset = display(
        &mut table_with(&values),
        WidthPolicy::wide(),
        DrawMode::CursorReset,
        CellWidth::UnicodeWide,
    );
    for (name, s) in [("padded", &padded), ("reset", &reset)] {
        let rows = border_columns(s);
        let first = &rows[0];
        for r in &rows {
            assert_eq!(r, first, "{name} is misaligned:\n{}", s.text());
        }
    }
    assert_eq!(
        padded.text(),
        reset.text(),
        "the fallback drew a different table rather than the same one differently"
    );
}

#[test]
fn the_fallback_emits_absolute_moves_rather_than_more_padding() {
    // How it achieves the alignment matters: padding harder would just be the same bug with
    // different numbers.
    let mut t = table_with(&["中文"]);
    let padded = t
        .clone()
        .render_with(100, Theme::plain(), WidthPolicy::wide(), DrawMode::Padded);
    let reset = t.render_with(
        100,
        Theme::plain(),
        WidthPolicy::wide(),
        DrawMode::CursorReset,
    );
    let padded_text = padded.lines.join("\n");
    let reset_text = reset.lines.join("\n");
    assert!(
        !padded_text.contains("\x1b["),
        "the normal path stays free of cursor movement"
    );
    assert!(
        reset_text.contains("\r\x1b[") && reset_text.contains('C'),
        "the fallback positions each boundary absolutely"
    );
}

// ================================================ probe -> fallback
#[test]
fn the_probe_selects_the_fallback_for_a_terminal_that_disagrees() {
    // The two halves joined up: measure the terminal, then draw the way the measurement says.
    use pr_terminal::probe::{self, Sample};

    use unicode_width::UnicodeWidthChar as _;
    let cjk_terminal = |bytes: &[u8]| -> Option<Vec<u8>> {
        if !bytes.ends_with(b"\x1b[6n") {
            return Some(Vec::new());
        }
        let body = &bytes[3..bytes.len() - 4];
        let ch = std::str::from_utf8(body).ok()?.chars().next()?;
        Some(format!("\x1b[1;{}R", ch.width_cjk().unwrap_or(0) + 1).into_bytes())
    };

    let policy = WidthPolicy::narrow();
    let samples: Vec<Sample> = probe::run(policy, true, cjk_terminal).expect("the probe runs");
    let conclusion = probe::conclude(&samples);
    assert_eq!(conclusion.draw, DrawMode::CursorReset);

    // Drawing with what the probe concluded keeps the frame aligned on that terminal.
    let mut t = table_with(&["→ ± § ①", "plain"]);
    let s = display(&mut t, policy, conclusion.draw, CellWidth::UnicodeWide);
    let rows = border_columns(&s);
    let first = &rows[0];
    for r in &rows {
        assert_eq!(r, first, "{}", s.text());
    }
}

// ================================================ ASSIST-018
#[test]
fn assist_018_spans_are_bytes_and_widths_are_columns() {
    // "span 和可见宽度各用正确单位" — mixing them is how a cursor lands in the middle of a
    // character, or a truncation cuts one in half.
    let p = WidthPolicy::wide();
    let s = "中文abc🐧";
    assert_eq!(s.len(), 13, "byte length: 3+3+1+1+1+4");
    assert_eq!(s.chars().count(), 6, "character count");
    assert_eq!(p.str_width(s), 9, "display columns: 2+2+1+1+1+2");
    assert_ne!(p.str_width(s), s.len());
    assert_ne!(p.str_width(s), s.chars().count());

    // A combining mark adds bytes and characters but no columns.
    let combined = "cafe\u{301}";
    assert_eq!(combined.len(), 6);
    assert_eq!(combined.chars().count(), 5);
    assert_eq!(p.str_width(combined), 4, "the accent occupies no column");
}

#[test]
fn assist_018_truncation_never_splits_a_cluster() {
    // Cutting inside a character produces a lone combining mark or half a code point, and the
    // terminal then renders something the user never had.
    let p = WidthPolicy::wide();
    for s in ["中文中文", "🐧🐧🐧", "cafe\u{301} latte", "ab中cd"] {
        for max in 0..12 {
            let (out, w) = p.truncate(s, max);
            assert!(w <= max, "{s:?} truncated to {max} gave width {w}");
            assert!(s.starts_with(&out), "{out:?} is not a prefix of {s:?}");
            // A prefix that ends mid-character would not be valid UTF-8, which `String`
            // already guarantees. The cluster question is the other one: the cut must not
            // fall *between* a base character and the mark that belongs to it.
            let rest = &s[out.len()..];
            if let Some(next) = rest.chars().next() {
                assert_ne!(
                    next, '\u{301}',
                    "truncating {s:?} to {max} split a cluster: {out:?} | {rest:?}"
                );
            }
        }
    }
}

// ================================================ ASSIST-067
#[test]
fn assist_067_a_narrow_window_reflows_the_menu_without_covering_the_input() {
    use pr_terminal::testing::claim;
    use pr_terminal::{Capture, Coordinator, Input, Key};

    let _c = claim();
    let mut co = Coordinator::new(Capture::new(), 80, 24).unwrap();
    co.set_prompt("penguin> ");
    co.start();
    for ch in "HGET 中文键 f".chars() {
        co.handle(Input::Key(Key::Char(ch)));
    }
    let before = co.text().to_owned();
    let cursor_before = co.cursor();

    co.open_menu(vec![
        "field-one    a very long description that will not fit".into(),
        "field-two    another long description".into(),
    ]);
    co.handle(Input::Resize { cols: 24, rows: 10 });

    // The bytes the user typed are untouched by the reflow.
    assert_eq!(co.text(), before, "a resize changed the input");
    assert_eq!(co.cursor(), cursor_before);

    // And the prompt row still reads what was typed, with the menu below it rather than over.
    let mut s = Screen::with_width(24, 10, CellWidth::UnicodeWide);
    s.feed(&co.sink().bytes);
    let prompt_row = s
        .lines()
        .iter()
        .position(|l| l.contains("penguin>"))
        .expect("the prompt is on screen");
    let menu_row = s
        .lines()
        .iter()
        .position(|l| l.contains("field-one"))
        .expect("the menu is on screen");
    assert!(
        menu_row > prompt_row,
        "the menu covered the line being edited\n{}",
        s.text()
    );
    // Rows are truncated to the narrow window rather than wrapping over the prompt.
    for line in s.lines() {
        assert!(
            line.chars().count() <= 24,
            "a row overflowed the 24-column window: {line:?}"
        );
    }
    co.shutdown();
}

#[test]
fn assist_067_the_menu_truncates_rather_than_wrapping() {
    use pr_terminal::testing::claim;
    use pr_terminal::{Capture, Coordinator};

    let _c = claim();
    let mut co = Coordinator::new(Capture::new(), 20, 8).unwrap();
    co.set_prompt("> ");
    co.start();
    co.open_menu(vec!["a".repeat(100), "中".repeat(50)]);
    let mut s = Screen::with_width(20, 8, CellWidth::UnicodeWide);
    s.feed(&co.sink().bytes);
    for line in s.lines() {
        assert!(
            line.chars().count() <= 20,
            "a menu row wrapped instead of being truncated: {line:?}"
        );
    }
    co.shutdown();
}
