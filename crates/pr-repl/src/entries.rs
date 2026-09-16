//! Every function's text entry point (v2.1 §14.2, ASSIST-068, V-C05).
//!
//! §14.2 gives each function a default key and a command. The default keys are the
//! convenient path; the commands are the *guaranteed* one, and the difference matters because
//! a function key is not ours to rely on:
//!
//! - macOS binds F1–F6 to brightness and Mission Control unless the user changes a system
//!   setting
//! - a terminal multiplexer takes `Ctrl+B`, and often more
//! - an input method takes `Ctrl+Space` on all three platforms
//! - some terminals simply do not send function keys at all
//!
//! §14.2's line on this is unambiguous: "不能要求用户先修改系统快捷键才能找帮助" — a user must
//! not have to change a system shortcut before they can find help. So the invariant this
//! module exists to enforce is that **every function is reachable by typing**, and the tests
//! check it against the table rather than against a list somebody remembered to update.

/// One row of §14.2.
///
/// Named `KeyEntry` rather than `Entry` because `history::Entry` already means a recorded
/// command; two `Entry` types in one crate would make every import a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    /// What it does.
    pub function: &'static str,
    /// The default key, as §14.2 writes it. Advisory: remappable, and interceptable.
    pub default_key: &'static str,
    /// The text command that always works. `None` only where the function *is* the text —
    /// see [`Entry::text`].
    pub command: &'static str,
}

impl KeyEntry {
    /// The text a user types to reach this function.
    #[must_use]
    pub fn text(self) -> &'static str {
        self.command
    }

    /// Whether the default key is one an input method or window manager commonly takes.
    ///
    /// Not a defect in itself — it is why the command column is mandatory.
    #[must_use]
    pub fn key_is_contended(self) -> bool {
        self.default_key.starts_with('F') || self.default_key.contains("Ctrl+")
    }
}

/// §14.2's table, as data.
///
/// `Ctrl+Space` is deliberately absent as a *default*: §14.2 says it may collide with input
/// methods and terminal bindings, so it is never the only way to anything.
pub const ENTRIES: &[KeyEntry] = &[
    KeyEntry {
        function: "full command and argument help",
        default_key: "F1",
        command: ":help",
    },
    KeyEntry {
        function: "find a command or tool by purpose",
        default_key: "F2",
        command: ":find",
    },
    KeyEntry {
        function: "build the current command step by step",
        default_key: "F3",
        command: ":guide",
    },
    KeyEntry {
        function: "focus the TUI command box",
        default_key: "F4",
        // Not a `:` command: the visible Command label is the discoverable entry, and typing
        // anywhere in the TUI's data area is what moves focus there.
        command: ":focus command",
    },
    KeyEntry {
        function: "explicit candidate discovery",
        default_key: "F6",
        command: ":complete",
    },
    KeyEntry {
        function: "history search",
        default_key: "Ctrl+R",
        command: ":history",
    },
    KeyEntry {
        function: "cancel input or the current wait",
        default_key: "Ctrl+C",
        // Cancelling has no text form — by the time you need it the line is not yours to
        // type into. `Ctrl+C` is the one key every terminal delivers, which is why this is
        // the one row where that is acceptable.
        command: "",
    },
    KeyEntry {
        function: "leave the REPL",
        default_key: "Ctrl+D on an empty line",
        command: "exit",
    },
];

/// Look up a function's text entry.
#[must_use]
pub fn entry_for(function: &str) -> Option<KeyEntry> {
    ENTRIES.iter().copied().find(|e| e.function == function)
}

/// Every function that has a text entry, with the text.
#[must_use]
pub fn text_entries() -> Vec<(&'static str, &'static str)> {
    ENTRIES
        .iter()
        .filter(|e| !e.command.is_empty())
        .map(|e| (e.function, e.command))
        .collect()
}

/// A user's remapping of a default key.
///
/// Remapping is expected — §14.2 calls the keys "design defaults". What must survive it is the
/// text entry, which is why this type cannot express "remove the command".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyMap {
    overrides: Vec<(&'static str, String)>,
}

impl KeyMap {
    /// An empty map: every function keeps its default key.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `function` to a different key.
    pub fn remap(&mut self, function: &'static str, key: impl Into<String>) {
        self.overrides.retain(|(f, _)| *f != function);
        self.overrides.push((function, key.into()));
    }

    /// The key currently bound to `function`, if any.
    #[must_use]
    pub fn key_for(&self, function: &str) -> Option<String> {
        if let Some((_, k)) = self.overrides.iter().find(|(f, _)| *f == function) {
            return Some(k.clone());
        }
        entry_for(function).map(|e| e.default_key.to_owned())
    }

    /// The text entry for `function`, which a remapping cannot take away.
    #[must_use]
    pub fn text_for(&self, function: &str) -> Option<&'static str> {
        entry_for(function)
            .map(KeyEntry::text)
            .filter(|t| !t.is_empty())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::local::parse_local;

    #[test]
    fn every_function_except_cancelling_is_reachable_by_typing() {
        // ASSIST-068. The one exception is `Ctrl+C`: by the time you need it, the line is not
        // yours to type into, and it is the one key every terminal delivers.
        for e in ENTRIES {
            if e.function.starts_with("cancel") {
                assert!(e.command.is_empty(), "cancelling has no text form");
                continue;
            }
            assert!(
                !e.command.is_empty(),
                "{} has no text entry, so a terminal that eats {} makes it unreachable",
                e.function,
                e.default_key
            );
        }
    }

    #[test]
    fn every_text_entry_parses() {
        // A documented entry that does not parse is not an entry.
        for (function, text) in text_entries() {
            if text == "exit" {
                // A bare word, not a local command: `exit` is what every REPL uses.
                assert!(!text.starts_with(':'));
                continue;
            }
            let parsed = parse_local(text.as_bytes())
                .unwrap_or_else(|e| panic!("{function}: {text:?} does not parse: {e}"));
            assert!(!parsed.name.is_empty());
        }
    }

    #[test]
    fn no_text_entry_needs_a_key_a_terminal_might_eat() {
        // The entries are typed, so they must be ordinary characters — no escape sequences, no
        // control bytes, nothing an input method or a multiplexer can intercept.
        for (function, text) in text_entries() {
            for c in text.chars() {
                assert!(
                    c.is_ascii_graphic() || c == ' ',
                    "{function}: {text:?} contains {c:?}, which is not something a user types"
                );
            }
        }
    }

    #[test]
    fn ctrl_space_is_never_a_default() {
        // §14.2: it collides with input methods on all three platforms, so it is never the
        // only way to anything — and here, not a default at all.
        for e in ENTRIES {
            assert_ne!(
                e.default_key, "Ctrl+Space",
                "{} defaults to a key input methods take",
                e.function
            );
        }
    }

    #[test]
    fn every_contended_key_has_a_text_entry_beside_it() {
        // The function keys and the Ctrl chords are exactly the ones a system may intercept.
        let contended: Vec<&KeyEntry> = ENTRIES.iter().filter(|e| e.key_is_contended()).collect();
        assert!(contended.len() >= 6, "{}", contended.len());
        for e in contended {
            if e.function.starts_with("cancel") {
                continue;
            }
            assert!(
                !e.command.is_empty(),
                "{} is only reachable by {}",
                e.function,
                e.default_key
            );
        }
    }

    #[test]
    fn remapping_a_key_cannot_remove_the_text_entry() {
        let mut m = KeyMap::new();
        assert_eq!(m.key_for("history search").as_deref(), Some("Ctrl+R"));
        m.remap("history search", "Ctrl+G");
        assert_eq!(m.key_for("history search").as_deref(), Some("Ctrl+G"));
        assert_eq!(
            m.text_for("history search"),
            Some(":history"),
            "the typed entry is not the user's to lose"
        );
    }

    #[test]
    fn remapping_twice_keeps_only_the_last_binding() {
        let mut m = KeyMap::new();
        m.remap("full command and argument help", "Ctrl+H");
        m.remap("full command and argument help", "F9");
        assert_eq!(
            m.key_for("full command and argument help").as_deref(),
            Some("F9")
        );
    }

    #[test]
    fn the_table_covers_every_row_of_section_14_2() {
        // If §14.2 gains a row, this fails until the table does too.
        let functions: Vec<&str> = ENTRIES.iter().map(|e| e.function).collect();
        assert_eq!(functions.len(), 8, "§14.2 has eight rows");
        for key in ["F1", "F2", "F3", "F4", "F6", "Ctrl+R", "Ctrl+C"] {
            assert!(
                ENTRIES.iter().any(|e| e.default_key == key),
                "{key} is in §14.2 and missing here"
            );
        }
        // F5 is deliberately absent: §14.2 does not assign it, and inventing a binding would
        // be a decision the spec did not make.
        assert!(!ENTRIES.iter().any(|e| e.default_key == "F5"));
    }

    #[test]
    fn an_unknown_function_has_no_entry_rather_than_a_guessed_one() {
        assert_eq!(entry_for("teleport"), None);
        assert_eq!(KeyMap::new().text_for("teleport"), None);
    }
}
