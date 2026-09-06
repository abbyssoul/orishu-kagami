//! ADR 0006's parity guarantee, as a test rather than a promise.
//!
//! "The same commands submitted through the UI path and through MCP must
//! produce the same accepted revisions, run decisions, and observations."
//! These tests keep both callers visible in one place, so a change that gives
//! one adapter a privileged path fails here rather than being discovered by a
//! user whose agent and window disagree.

mod support;

use kagami_document::{ExperimentCommand, Limits};
use kagami_session::{
    Acceptance, DocumentAuthority, ExperimentChange, SessionCommand, SessionView,
};
use support::{Adapter, create, name, schemas};

fn authority() -> DocumentAuthority {
    DocumentAuthority::new(schemas(), Limits::DEFAULT)
}

/// The script both callers run: create, rename, create again, undo.
fn script() -> Vec<SessionCommand> {
    vec![
        SessionCommand::Edit(vec![create("Earth")]),
        SessionCommand::Edit(vec![create("Moon")]),
        SessionCommand::Undo,
        SessionCommand::Redo,
    ]
}

/// What a caller can compare across two sessions: everything except the
/// correlation identities, which are per-adapter by construction.
fn comparable(view: &SessionView) -> (u64, usize, bool, Option<String>, Option<String>) {
    (
        view.revision().get(),
        view.experiment.object_count(),
        view.dirty,
        view.history.undo.clone(),
        view.history.redo.clone(),
    )
}

fn changes(acceptances: &[Acceptance]) -> Vec<ExperimentChange> {
    acceptances
        .iter()
        .map(|acceptance| acceptance.change.clone())
        .collect()
}

#[test]
fn the_same_script_through_either_adapter_produces_the_same_session() {
    let mut ui_authority = authority();
    let mut ui = Adapter::guarded("ui");
    let ui_outcomes: Vec<_> = script()
        .into_iter()
        .map(|command| {
            ui.submit(&mut ui_authority, command)
                .expect("accepted through the UI path")
        })
        .collect();

    let mut mcp_authority = authority();
    let mut mcp = Adapter::unguarded("agent");
    let mcp_outcomes: Vec<_> = script()
        .into_iter()
        .map(|command| {
            mcp.submit(&mut mcp_authority, command)
                .expect("accepted through the MCP path")
        })
        .collect();

    // Same accepted revisions, in the same order.
    let ui_revisions: Vec<_> = ui_outcomes.iter().map(|a| a.revision).collect();
    let mcp_revisions: Vec<_> = mcp_outcomes.iter().map(|a| a.revision).collect();
    assert_eq!(ui_revisions, mcp_revisions);

    // Same decisions, including the labels an undo affordance shows.
    assert_eq!(changes(&ui_outcomes), changes(&mcp_outcomes));

    // Same resulting session.
    assert_eq!(
        comparable(&ui_authority.view()),
        comparable(&mcp_authority.view())
    );
}

#[test]
fn a_user_and_an_agent_share_one_history_in_one_session() {
    // Not "the same script twice": the same *session*, edited by both. An
    // agent's undo reverses whatever the last accepted edit was, including the
    // user's — which is the consequence ADR 0019 records and the UI must show.
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let mut agent = Adapter::unguarded("agent");

    ui.edit(&mut authority, vec![create("Earth")]);
    agent.edit(&mut authority, vec![create("Moon")]);
    assert_eq!(authority.snapshot().object_count(), 2);

    let undone = ui
        .submit(&mut authority, SessionCommand::Undo)
        .expect("the agent's edit is on the shared history");
    assert_eq!(
        undone.change,
        ExperimentChange::Undone {
            label: "Add object".to_owned()
        }
    );
    assert_eq!(authority.snapshot().object_count(), 1);

    // And the remaining object is the user's, not the agent's.
    let snapshot = authority.snapshot();
    let remaining: Vec<_> = snapshot
        .objects()
        .values()
        .map(|object| object.name.as_str())
        .collect();
    assert_eq!(remaining, vec!["Earth"]);
}

#[test]
fn a_guarded_submission_refuses_an_edit_composed_against_a_stale_view() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let mut agent = Adapter::unguarded("agent");

    // The UI reads a view, then the agent edits before the UI submits.
    let stale = authority.revision();
    agent.edit(&mut authority, vec![create("Moon")]);

    let envelope = ui
        .envelope(&authority, SessionCommand::Edit(vec![create("Earth")]))
        .guarded_by(stale);
    let rejection = authority
        .submit(envelope)
        .expect_err("the UI composed against a revision that has moved");

    assert_eq!(rejection.code(), "revision_conflict");
    // Nothing changed: the refusal is not a partial application.
    assert_eq!(authority.snapshot().object_count(), 1);

    // Recomposing against what is actually current succeeds.
    ui.edit(&mut authority, vec![create("Earth")]);
    assert_eq!(authority.snapshot().object_count(), 2);
}

#[test]
fn an_unguarded_submission_applies_to_whatever_is_current() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let mut agent = Adapter::unguarded("agent");

    ui.edit(&mut authority, vec![create("Earth")]);
    // The agent never named a revision, so a concurrent edit is not a
    // conflict for it. That is the opt-out, and it is the caller's choice.
    agent.edit(&mut authority, vec![create("Moon")]);
    assert_eq!(authority.snapshot().object_count(), 2);
}

#[test]
fn preflight_returns_the_decision_submitting_would_without_changing_anything() {
    let mut authority = authority();
    let mut agent = Adapter::unguarded("agent");

    let valid = agent.envelope(&authority, SessionCommand::Edit(vec![create("Earth")]));
    let report = authority.preflight(&valid).expect("a valid edit");
    assert_eq!(report.label, "Add object");
    assert_eq!(report.created_objects.len(), 1);

    // Advisory, not applied.
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.revision().get(), 0);
    assert!(!authority.is_dirty());

    // An identity that was real and is not: create it, then remove it.
    let mut setup = Adapter::unguarded("setup");
    let accepted = setup.edit(&mut authority, vec![create("Doomed")]);
    let ExperimentChange::Edited { report } = accepted.change else {
        panic!("an edit reports what it created");
    };
    let ghost = report.first_created().expect("one object");
    setup.edit(&mut authority, vec![ExperimentCommand::RemoveObject(ghost)]);

    // A refusal is the same refusal a submission would give, so an agent can
    // repair its input against the real rules rather than a looser copy.
    let broken = agent.envelope(
        &authority,
        SessionCommand::Edit(vec![ExperimentCommand::RenameObject {
            object: ghost,
            name: name("Ghost"),
        }]),
    );
    let preflight = authority
        .preflight(&broken)
        .expect_err("the object no longer exists");
    let submitted = authority
        .submit(broken)
        .expect_err("and submitting says exactly the same thing");
    assert_eq!(preflight.code(), submitted.code());
    assert_eq!(preflight.to_string(), submitted.to_string());
}
