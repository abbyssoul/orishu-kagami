//! Ownership of the terminal modes the shell switches on.
//!
//! Acquiring a terminal is three separate, individually failable transitions,
//! and any of them can fail *after* taking effect — an escape sequence can
//! reach the terminal and the following flush still return an error. A guard
//! that recorded "this step succeeded" would then believe the terminal was
//! untouched and skip the matching restore, leaving the operator in the
//! alternate screen with no cursor.
//!
//! So [`TerminalSession`] records a *cleanup obligation* before each transition
//! is attempted, not after it returns. Restoration is best effort, so undoing
//! something that never took effect costs nothing; failing to undo something
//! that did costs the operator their terminal.
//!
//! The transitions go through [`TerminalControl`] rather than being called
//! directly. That is not a plug-in point — there is exactly one production
//! implementation, and the trait is private to the crate — it exists because
//! partial-failure rollback is otherwise unobservable, and it is precisely the
//! behaviour that must not regress.

use std::io::{self, Write};

use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute, terminal};

/// The terminal transitions a [`TerminalSession`] owns.
///
/// Each method is one transition. Implementations must not assume a method is
/// called only when its inverse would be valid: a restore may be attempted for
/// a transition that failed part-way through.
pub(crate) trait TerminalControl {
    /// Put the terminal into raw mode.
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    /// Return the terminal to canonical mode.
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    /// Switch to the alternate screen buffer.
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    /// Switch back to the primary screen buffer.
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    /// Stop drawing the cursor.
    fn hide_cursor(&mut self) -> io::Result<()>;
    /// Draw the cursor again.
    fn show_cursor(&mut self) -> io::Result<()>;
}

/// The production implementation, over the process's real standard output.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CrosstermControl;

impl TerminalControl for CrosstermControl {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), cursor::Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), cursor::Show)?;
        io::stdout().flush()
    }
}

/// What still has to be undone before the process leaves.
///
/// A field is set when its transition is *attempted*, so it means "this may
/// have taken effect", not "this succeeded".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Obligations {
    raw_mode: bool,
    alternate_screen: bool,
    cursor_hidden: bool,
}

/// Owns the terminal modes the shell switches on, and switches them back.
///
/// Restoration happens in [`Drop`], which means it covers a normal exit, an
/// error returned after terminal entry, and a panic — there is no cleanup
/// branch to forget.
///
/// Panic coverage rests on unwinding, which is the profile's default here. A
/// build that set `panic = "abort"` would skip this destructor and leave the
/// operator in the alternate screen, and would need a panic hook instead.
pub(crate) struct TerminalSession<C: TerminalControl> {
    control: C,
    obligations: Obligations,
}

impl<C: TerminalControl> TerminalSession<C> {
    /// Switch the terminal into raw mode and the alternate screen, and hide the
    /// cursor.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] from the first transition that
    /// fails. Everything attempted up to and including that transition is
    /// rolled back before the error reaches the caller, because the returned
    /// `Err` drops the partially built session.
    pub(crate) fn acquire(control: C) -> Result<Self, io::Error> {
        let mut session = Self {
            control,
            obligations: Obligations::default(),
        };

        // Obligation first, transition second — see the module documentation.
        // `?` here drops `session`, so `Drop` performs the rollback.
        session.obligations.raw_mode = true;
        session.control.enable_raw_mode()?;

        session.obligations.alternate_screen = true;
        session.control.enter_alternate_screen()?;

        session.obligations.cursor_hidden = true;
        session.control.hide_cursor()?;

        Ok(session)
    }
}

impl<C: TerminalControl> Drop for TerminalSession<C> {
    /// Restore the terminal, in the reverse of acquisition order.
    ///
    /// Best effort by necessity: this may run while unwinding, or after the
    /// terminal has gone away, and there is nowhere useful left to report a
    /// failure to. Each obligation is discharged regardless of whether the
    /// previous one succeeded, so one failure cannot strand the others.
    fn drop(&mut self) {
        if self.obligations.cursor_hidden {
            let _ = self.control.show_cursor();
        }
        if self.obligations.alternate_screen {
            let _ = self.control.leave_alternate_screen();
        }
        if self.obligations.raw_mode {
            let _ = self.control.disable_raw_mode();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Call {
        EnableRaw,
        DisableRaw,
        EnterAlternate,
        LeaveAlternate,
        HideCursor,
        ShowCursor,
    }

    /// Records every transition, and fails the ones it is told to.
    ///
    /// The log is shared so the test can read it after the session has been
    /// dropped, which is when restoration happens.
    struct FakeControl {
        log: Rc<RefCell<Vec<Call>>>,
        failing: Vec<Call>,
    }

    impl FakeControl {
        fn new(log: &Rc<RefCell<Vec<Call>>>, failing: &[Call]) -> Self {
            Self {
                log: Rc::clone(log),
                failing: failing.to_vec(),
            }
        }

        fn record(&mut self, call: Call) -> io::Result<()> {
            self.log.borrow_mut().push(call);
            if self.failing.contains(&call) {
                return Err(io::Error::other(format!("{call:?} failed")));
            }
            Ok(())
        }
    }

    impl TerminalControl for FakeControl {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.record(Call::EnableRaw)
        }
        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.record(Call::DisableRaw)
        }
        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.record(Call::EnterAlternate)
        }
        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.record(Call::LeaveAlternate)
        }
        fn hide_cursor(&mut self) -> io::Result<()> {
            self.record(Call::HideCursor)
        }
        fn show_cursor(&mut self) -> io::Result<()> {
            self.record(Call::ShowCursor)
        }
    }

    /// Acquire with `failing` transitions rejected, then drop, and return the
    /// complete call log.
    fn acquire_then_drop(failing: &[Call]) -> (bool, Vec<Call>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let acquired = {
            let session = TerminalSession::acquire(FakeControl::new(&log, failing));
            session.is_ok()
        };
        let calls = log.borrow().clone();

        (acquired, calls)
    }

    #[test]
    fn a_complete_session_restores_everything_in_reverse_order() {
        let (acquired, calls) = acquire_then_drop(&[]);

        assert!(acquired);
        assert_eq!(
            calls,
            [
                Call::EnableRaw,
                Call::EnterAlternate,
                Call::HideCursor,
                Call::ShowCursor,
                Call::LeaveAlternate,
                Call::DisableRaw,
            ]
        );
    }

    /// The regression this ordering exists for: a transition that fails may
    /// still have reached the terminal, so its undo must be attempted anyway.
    #[test]
    fn a_transition_that_fails_is_still_undone() {
        let (acquired, calls) = acquire_then_drop(&[Call::EnterAlternate]);

        assert!(!acquired);
        assert_eq!(
            calls,
            [
                Call::EnableRaw,
                Call::EnterAlternate,
                // The cursor was never touched, so it is not shown; the screen
                // may have switched despite the error, so it is switched back.
                Call::LeaveAlternate,
                Call::DisableRaw,
            ]
        );
    }

    #[test]
    fn a_failure_hiding_the_cursor_undoes_all_three() {
        let (acquired, calls) = acquire_then_drop(&[Call::HideCursor]);

        assert!(!acquired);
        assert_eq!(
            calls,
            [
                Call::EnableRaw,
                Call::EnterAlternate,
                Call::HideCursor,
                Call::ShowCursor,
                Call::LeaveAlternate,
                Call::DisableRaw,
            ]
        );
    }

    /// Nothing that was never attempted is undone: a terminal that refused raw
    /// mode must not be sent alternate-screen or cursor sequences.
    #[test]
    fn transitions_that_were_never_attempted_are_not_undone() {
        let (acquired, calls) = acquire_then_drop(&[Call::EnableRaw]);

        assert!(!acquired);
        assert_eq!(calls, [Call::EnableRaw, Call::DisableRaw]);
    }

    /// Restoration is best effort and unordered in its failures: one undo that
    /// errors must not prevent the others from being attempted.
    #[test]
    fn a_failing_restore_does_not_strand_the_others() {
        let (acquired, calls) = acquire_then_drop(&[Call::ShowCursor, Call::DisableRaw]);

        assert!(acquired);
        assert_eq!(
            calls,
            [
                Call::EnableRaw,
                Call::EnterAlternate,
                Call::HideCursor,
                Call::ShowCursor,
                Call::LeaveAlternate,
                Call::DisableRaw,
            ]
        );
    }
}
