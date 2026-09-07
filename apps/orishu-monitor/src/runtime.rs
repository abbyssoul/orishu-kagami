//! The imperative shell: terminal ownership and the event loop.
//!
//! Everything that touches a real terminal is here. The loop reads events,
//! hands each to [`crate::input::message_for`], folds the resulting message
//! through [`crate::update::update`], and draws [`crate::view::render`]. It
//! makes no decision of its own about navigation.

use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::event;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::input;
use crate::model::{Model, TerminalSize};
use crate::terminal::{CrosstermControl, TerminalSession};
use crate::update::update;
use crate::view;

/// How long a quiet loop blocks before waking to check again.
///
/// An explicit cadence rather than a spin: the process is idle between events,
/// and a terminal that reports a resize without an accompanying event is still
/// noticed within this interval.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How many queued events one wake-up drains before drawing again.
///
/// A paste or a burst of resizes is folded into the model up to this many
/// times and then costs a single replacement frame, so no input burst can grow
/// unbounded retained work.
const MAX_EVENTS_PER_WAKE: usize = 64;

// The two numbers above are what keep an idle shell cheap and an input burst
// bounded, so they are checked where a violation cannot be shipped: a drain
// bound of zero would ignore every event, and a poll interval near zero would
// turn the idle loop into a spin.
const _: () = assert!(MAX_EVENTS_PER_WAKE > 0);
const _: () = assert!(POLL_INTERVAL.as_millis() >= 50);

/// Why the shell could not run, or could not finish.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// Standard input or output is not a terminal.
    ///
    /// Reported before any terminal state is changed, so a piped or redirected
    /// invocation never has escape sequences written into it.
    #[error(
        "orishu-monitor is an interactive terminal application and needs a terminal on stdin and \
         stdout; use --help or --version for non-interactive output, or orishuctl for scriptable \
         administration"
    )]
    NotInteractive,

    /// The terminal could not be acquired, drawn to, or read from.
    #[error("terminal error: {0}")]
    Terminal(#[from] io::Error),
}

/// Run the shell until the operator asks to leave.
///
/// # Errors
///
/// [`ShellError::NotInteractive`] if stdin or stdout is not a terminal, which
/// is checked before any terminal state changes; otherwise
/// [`ShellError::Terminal`] for a failure acquiring, drawing to, or reading
/// from the terminal. The terminal is restored on every path that returns after
/// it was acquired.
pub fn run() -> Result<(), ShellError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(ShellError::NotInteractive);
    }

    let _session = TerminalSession::acquire(CrosstermControl)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    event_loop(&mut terminal)
}

/// Draw, wait, fold, repeat.
fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), ShellError> {
    // Start from the terminal's real size rather than a guess, so the first
    // frame is already correct for a terminal below the minimum.
    let size = terminal.size()?;
    let mut model = Model::new(TerminalSize::new(size.width, size.height));
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|frame| view::render(frame, &model))?;
            dirty = false;
        }

        if !event::poll(POLL_INTERVAL)? {
            continue;
        }

        for _ in 0..MAX_EVENTS_PER_WAKE {
            let event = event::read()?;
            if let Some(message) = input::message_for(&event) {
                let folded = update(model.clone(), message);
                if folded != model {
                    model = folded;
                    dirty = true;
                }
                if model.exit_intent().is_requested() {
                    return Ok(());
                }
            }

            // Drain what is already queued, but do not wait for more: the
            // pending frame is owed to the operator.
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal must name what is wrong and where to go instead, without
    /// suggesting the shell tried and failed to reach a worker.
    #[test]
    fn the_non_interactive_refusal_is_actionable() {
        let message = ShellError::NotInteractive.to_string();

        assert!(message.contains("terminal"));
        assert!(message.contains("--help"));
        assert!(message.contains("orishuctl"));
    }

    #[test]
    fn an_io_failure_is_reported_as_a_terminal_error() {
        let error = ShellError::from(io::Error::other("no tty"));

        assert!(matches!(error, ShellError::Terminal(_)));
        assert!(error.to_string().contains("no tty"));
    }
}
