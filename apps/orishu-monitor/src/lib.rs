//! `orishu-monitor`: the interactive terminal shell for Orishu operators.
//!
//! This crate is the **shell slice only**. It opens a full-screen Overview /
//! Members application with a help overlay, owns the terminal safely, and says
//! plainly that it has no worker integration. It does not connect to an Orishu
//! worker, hold credentials, poll, stream, or mutate anything, and it takes no
//! dependency on the Orishu client protocol. `orishuctl` is the implemented
//! administration client.
//!
//! A library as well as a binary so the parts that are *not* a terminal can be
//! tested without one: the model, the key table, the input mapping, the update
//! function, and the view all run against values.
//!
//! ## Shape
//!
//! A functional core behind an imperative shell, in the model-message-update
//! form described in `docs/Coding style.md`:
//!
//! ```text
//! terminal event ──▶ input::message_for ──▶ Message
//!                                             │
//!                                             ▼
//!                                  update(Model, Message) -> Model
//!                                             │
//!                                             ▼
//!                                    view::render(&Model)
//! ```
//!
//! Only `runtime` and the private `terminal` module touch a terminal: the
//! first owns the event loop, the second owns the modes the shell switches on
//! and is responsible for switching them back. [`model`], [`message`],
//! [`update`], [`keys`], and [`view`] are pure: they read their arguments and
//! return values.

pub mod input;
pub mod keys;
pub mod message;
pub mod model;
pub mod runtime;
mod terminal;
pub mod update;
pub mod view;
