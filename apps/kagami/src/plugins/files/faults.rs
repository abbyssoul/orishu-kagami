//! Test-only, per-thread publication barriers. No production environment switch.
use super::{Code, Error};
use std::{cell::RefCell, io::Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    StageCreated,
    StageWritten,
    FileSynced,
    Published,
    StageUnlinked,
    DirectorySynced,
    ExistingFileSynced,
    ExistingDirectorySynced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Event {
    pub name: String,
    pub phase: Phase,
}

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Record,
    Fail(usize),
    Pause(usize),
}

struct Plan {
    action: Action,
    events: Vec<Event>,
}
thread_local! { static PLAN: RefCell<Option<Plan>> = const { RefCell::new(None) }; }

pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        PLAN.with(|p| *p.borrow_mut() = None);
    }
}
pub(crate) fn arm(action: Action) -> Guard {
    PLAN.with(|p| {
        assert!(p.borrow().is_none());
        *p.borrow_mut() = Some(Plan {
            action,
            events: Vec::new(),
        });
    });
    Guard
}
pub(crate) fn events() -> Vec<Event> {
    PLAN.with(|p| p.borrow().as_ref().unwrap().events.clone())
}
pub(crate) fn checkpoint(name: &str, phase: Phase) -> Result<(), Error> {
    let action = PLAN.with(|p| {
        let mut p = p.borrow_mut();
        let plan = p.as_mut()?;
        plan.events.push(Event {
            name: name.into(),
            phase,
        });
        match plan.action {
            Action::Fail(n) | Action::Pause(n) if n == plan.events.len() => Some(plan.action),
            _ => None,
        }
    });
    match action {
        Some(Action::Fail(_)) => Err(Error::new(Code::IoFailure, "injected publication failure")),
        Some(Action::Pause(_)) => {
            println!("PLUGIN-PUBLICATION-PAUSED");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        _ => Ok(()),
    }
}
