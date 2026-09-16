//! Penguin Redis — `pr-terminal` (v2.1 §31.3, §14.4, ADR-022).
//!
//! The single terminal coordinator. Everything that reads the keyboard or paints the screen
//! goes through here: the REPL line editor, the self-drawn candidate dropdown, out-of-band
//! notices from Tokio workers, and the TUI's alternate screen.
//!
//! The rule §14.4 states — never let a worker, the editor and a TUI painter touch the
//! terminal at once — is enforced as a resource rather than described as a convention. See
//! [`ownership`].

pub mod bridge;
pub mod coordinator;
pub mod event;
pub mod ownership;
pub mod paint;
pub mod probe;
pub mod testing;

pub use bridge::{translate, translate_key};
pub use coordinator::{Action, Coordinator, Menu, Surface};
pub use event::{Input, Key, Mailbox, Merged, Notice, NoticeOrigin, Scripted, Source};
pub use ownership::{Ownership, OwnershipError};
pub use paint::{Capture, Painter, Sink, Stdout};
pub use probe::{Conclusion, ProbeError, Sample, conclude, parse_cursor_report};
