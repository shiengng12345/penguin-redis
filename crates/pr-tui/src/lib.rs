//! Penguin Redis — `pr-tui` (v2.1 §31.3, §14.4, ADR-022).
//!
//! The TUI's layout, drawn with Ratatui. It does **not** own the terminal: `pr-terminal`
//! does, and hands it the alternate screen when the user asks for it (§14.4). What lives here
//! is the frame composition, which is exactly the part Ratatui is good at.
//!
//! Phase 0 needs one real thing from this crate: an idle TUI that allocates its buffers, so
//! V-H01 can measure what the dependency actually costs rather than guessing.

use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Terminal, TerminalOptions, Viewport};

/// An idle TUI: real buffers, real layout, nothing running.
///
/// Built on `TestBackend` so it can be constructed without a terminal — which is what makes
/// the idle-RSS measurement reproducible on a CI runner with no tty.
pub struct Idle {
    terminal: Terminal<TestBackend>,
}

impl Idle {
    /// Build an idle TUI of the given size.
    ///
    /// # Errors
    /// Propagates Ratatui's terminal construction error.
    pub fn new(cols: u16, rows: u16) -> Result<Self, std::io::Error> {
        let backend = TestBackend::new(cols, rows);
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 0, cols, rows)),
            },
        )?;
        Ok(Self { terminal })
    }

    /// Draw one frame: the three panes the data view is made of.
    ///
    /// # Errors
    /// Propagates the draw error.
    pub fn draw(&mut self, origin: &str) -> Result<(), std::io::Error> {
        self.terminal.draw(|f| {
            let area = f.area();
            let panes = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(3),
                ])
                .split(area);
            f.render_widget(Paragraph::new(origin.to_owned()), panes[0]);
            f.render_widget(
                Block::default().borders(Borders::ALL).title("results"),
                panes[1],
            );
            f.render_widget(
                Block::default().borders(Borders::ALL).title("command"),
                panes[2],
            );
        })?;
        Ok(())
    }

    /// What the last frame put on screen, as lines. For tests.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let buf = self.terminal.backend().buffer();
        let area = *buf.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_tui_draws_its_three_panes() {
        let mut t = Idle::new(80, 24).unwrap();
        t.draw("@r2 / DB 0 / result #17").unwrap();
        let lines = t.lines();
        assert_eq!(lines.len(), 24);
        assert!(lines[0].contains("@r2 / DB 0 / result #17"));
        let all = lines.join("\n");
        assert!(all.contains("results"), "{all}");
        assert!(all.contains("command"), "{all}");
    }

    #[test]
    fn a_narrow_terminal_still_composes() {
        // UX-07: a narrow terminal must not panic or silently drop a pane.
        let mut t = Idle::new(20, 8).unwrap();
        t.draw("@dev").unwrap();
        assert_eq!(t.lines().len(), 8);
    }
}
