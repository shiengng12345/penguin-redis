//! The V-C02 subject under test: a real terminal session driven by the coordinator.
//!
//! This binary exists so the PTY tests exercise the *same* code path a user would, including
//! raw mode and a real stdout, rather than a simulation. It does four things, which are
//! exactly the four §14.4 says must share one owner:
//!
//! - edits a REPL line
//! - draws its own candidate dropdown
//! - receives out-of-band notices from a worker thread
//! - hands the screen to a TUI and takes it back
//!
//! Control keys the test drives it with:
//!
//! | key | effect |
//! |---|---|
//! | `Ctrl-N` | open the dropdown |
//! | `Ctrl-P` | make a worker post a burst of pub/sub notices |
//! | `Ctrl-T` | enter the alternate screen |
//! | `Esc` | close the menu, or leave the alternate screen |
//! | `Ctrl-D` on an empty line | quit |

use std::io::Write as _;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use pr_terminal::{Action, Coordinator, Input, Key, Mailbox, NoticeOrigin, Stdout};

/// Translate a crossterm event into our own input.
///
/// Deliberately total: an event we do not model becomes `None` rather than a panic, because a
/// terminal can send anything and a client that dies on an unexpected escape sequence is
/// worse than one that ignores it.
fn translate(ev: &Event) -> Option<Input> {
    match ev {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => {
            let k = match code {
                KeyCode::Char(c) if modifiers.contains(KeyModifiers::CONTROL) => {
                    Key::Ctrl(c.to_ascii_lowercase())
                }
                KeyCode::Char(c) => Key::Char(*c),
                KeyCode::Backspace => Key::Backspace,
                KeyCode::Enter => Key::Enter,
                KeyCode::Tab => Key::Tab,
                KeyCode::Esc => Key::Esc,
                KeyCode::Up => Key::Up,
                KeyCode::Down => Key::Down,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::F(n) => Key::Function(*n),
                _ => return None,
            };
            Some(Input::Key(k))
        }
        Event::Paste(s) => Some(Input::Paste(s.as_bytes().to_vec())),
        Event::Resize(cols, rows) => Some(Input::Resize {
            cols: *cols,
            rows: *rows,
        }),
        _ => None,
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Printed before anything else touches the terminal, so a PTY test that times out can
    // tell "the process never started" apart from "the coordinator never painted". Windows
    // CI needed exactly this distinction.
    println!("demo starting");
    let _ = std::io::stdout().flush();

    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut co = Coordinator::new(Stdout::new(), cols, rows)?;
    co.set_prompt("penguin@r2/0> ");
    co.set_origin("@r2 / DB 0 / result #17");

    crossterm::terminal::enable_raw_mode()?;
    co.start();

    let mailbox = Mailbox::new();
    let mut alive = true;

    while alive {
        // The keyboard is served first so a burst of notices cannot starve it (ASSIST-062).
        let input = if crossterm::event::poll(Duration::from_millis(10))? {
            translate(&crossterm::event::read()?)
        } else {
            mailbox.take()
        };

        let Some(input) = input else { continue };

        // Test triggers. Real key bindings live in the application layer; here they only need
        // to be deterministic.
        if let Input::Key(Key::Ctrl(c)) = &input {
            match c {
                'n' => {
                    co.open_menu(vec![
                        "HGET      get a field".into(),
                        "HGETALL   get every field".into(),
                        "HGETDEL   get and delete".into(),
                    ]);
                    continue;
                }
                'p' => {
                    // A worker thread, which never touches the terminal — it posts.
                    let m = mailbox.clone();
                    std::thread::spawn(move || {
                        for i in 0..20 {
                            m.notify(NoticeOrigin::Push, format!("news #{i} from channel a"));
                        }
                    });
                    continue;
                }
                't' => {
                    co.enter_alternate();
                    continue;
                }
                _ => {}
            }
        }

        match co.handle(input) {
            Action::Submit(line) => {
                // Nothing is executed here; the point is that the line reached the
                // application intact.
                co.handle(Input::Message(pr_terminal::Notice {
                    text: format!("submitted: {line}"),
                    origin: NoticeOrigin::Client,
                }));
            }
            Action::Quit => alive = false,
            Action::None => {}
        }
    }

    co.shutdown();
    crossterm::terminal::disable_raw_mode()?;
    println!("demo exited cleanly");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        let _ = crossterm::terminal::disable_raw_mode();
        eprintln!("demo failed: {e}");
        std::process::exit(1);
    }
}
