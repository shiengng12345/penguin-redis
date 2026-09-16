//! Translating crossterm events into coordinator inputs (v2.1 §14.4, ADR-022, V-C02).
//!
//! This is the only place crossterm's event model is interpreted, for the same reason
//! `pr-terminal` is the only place bytes are painted: two interpretations that disagree are
//! two behaviours the user cannot predict.
//!
//! One rule here is not obvious and cost a CI round trip to find. **Windows reports key
//! releases.** On Unix a key press yields one event; under `ConPTY` crossterm delivers
//! `KeyEventKind::Press` *and* `KeyEventKind::Release`, so a translation that ignores `kind`
//! types every character twice — `HGET` arrives as `HHGGEETT`. Filtering by kind is therefore
//! correctness, not tidiness.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::event::{Input, Key};

/// Translate one crossterm event, or `None` if it is not something the coordinator acts on.
///
/// Deliberately total: a terminal can send anything, and a client that dies on an unexpected
/// sequence is worse than one that ignores it.
#[must_use]
pub fn translate(ev: &Event) -> Option<Input> {
    match ev {
        Event::Key(k) => translate_key(k).map(Input::Key),
        Event::Paste(s) => Some(Input::Paste(s.as_bytes().to_vec())),
        Event::Resize(cols, rows) => Some(Input::Resize {
            cols: *cols,
            rows: *rows,
        }),
        // Mouse and focus events are not part of the §14 contract; ignoring them is the
        // behaviour, not an omission.
        Event::FocusGained | Event::FocusLost | Event::Mouse(_) => None,
    }
}

/// Translate a key event, dropping anything that is not an actual press.
#[must_use]
pub fn translate_key(k: &KeyEvent) -> Option<Key> {
    // A release is the *same* key coming back up. Acting on it repeats the keystroke, which
    // on Windows doubles every character typed.
    if !matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    Some(match k.code {
        KeyCode::Char(c) if ctrl => Key::Ctrl(c.to_ascii_lowercase()),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Esc => Key::Esc,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::F(n) => Key::Function(n),
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn key(code: KeyCode, kind: KeyEventKind, mods: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: mods,
            kind,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn a_key_release_produces_nothing() {
        // The failure this prevents: on Windows every character arrives twice, so `HGET` is
        // typed as `HHGGEETT`. Found by the first real ConPTY run of the V-C02 tests.
        let press = key(KeyCode::Char('H'), KeyEventKind::Press, KeyModifiers::NONE);
        let release = key(
            KeyCode::Char('H'),
            KeyEventKind::Release,
            KeyModifiers::NONE,
        );
        assert_eq!(translate_key(&press), Some(Key::Char('H')));
        assert_eq!(translate_key(&release), None);
    }

    #[test]
    fn typing_a_word_with_releases_interleaved_yields_the_word_once() {
        let mut typed = String::new();
        for c in "HGET".chars() {
            for kind in [KeyEventKind::Press, KeyEventKind::Release] {
                if let Some(Key::Char(got)) =
                    translate_key(&key(KeyCode::Char(c), kind, KeyModifiers::NONE))
                {
                    typed.push(got);
                }
            }
        }
        assert_eq!(typed, "HGET", "a release must not repeat the keystroke");
    }

    #[test]
    fn auto_repeat_is_a_real_keystroke() {
        // Holding Backspace has to keep deleting; `Repeat` is a press, not a release.
        assert_eq!(
            translate_key(&key(
                KeyCode::Backspace,
                KeyEventKind::Repeat,
                KeyModifiers::NONE
            )),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn control_chords_are_lowercased_so_bindings_match_either_shift_state() {
        for c in ['c', 'C'] {
            assert_eq!(
                translate_key(&key(
                    KeyCode::Char(c),
                    KeyEventKind::Press,
                    KeyModifiers::CONTROL
                )),
                Some(Key::Ctrl('c'))
            );
        }
        // Without Control it is just a character, including an uppercase one.
        assert_eq!(
            translate_key(&key(
                KeyCode::Char('C'),
                KeyEventKind::Press,
                KeyModifiers::NONE
            )),
            Some(Key::Char('C'))
        );
    }

    #[test]
    fn the_navigation_and_function_keys_map_across() {
        let cases = [
            (KeyCode::Enter, Key::Enter),
            (KeyCode::Tab, Key::Tab),
            (KeyCode::Esc, Key::Esc),
            (KeyCode::Up, Key::Up),
            (KeyCode::Down, Key::Down),
            (KeyCode::Left, Key::Left),
            (KeyCode::Right, Key::Right),
            (KeyCode::Backspace, Key::Backspace),
            (KeyCode::F(1), Key::Function(1)),
        ];
        for (code, want) in cases {
            assert_eq!(
                translate_key(&key(code, KeyEventKind::Press, KeyModifiers::NONE)),
                Some(want),
                "{code:?}"
            );
        }
    }

    #[test]
    fn a_paste_arrives_whole() {
        let ev = Event::Paste("SET a 1\nSET b 2".to_owned());
        match translate(&ev) {
            Some(Input::Paste(b)) => assert_eq!(b, b"SET a 1\nSET b 2"),
            other => panic!("expected one paste, got {other:?}"),
        }
    }

    #[test]
    fn a_resize_carries_the_new_size() {
        assert_eq!(
            translate(&Event::Resize(40, 12)),
            Some(Input::Resize { cols: 40, rows: 12 })
        );
    }

    #[test]
    fn events_outside_the_contract_are_ignored_rather_than_guessed_at() {
        assert_eq!(translate(&Event::FocusGained), None);
        assert_eq!(translate(&Event::FocusLost), None);
        // An unmodelled key is dropped, not turned into something else.
        assert_eq!(
            translate_key(&key(
                KeyCode::Insert,
                KeyEventKind::Press,
                KeyModifiers::NONE
            )),
            None
        );
    }
}
