//! The four properties the envelope contract promises, plus the hostile
//! inputs an MCP-facing authority has to survive.

mod support;

use kagami_catalog::{PropertyName, SchemaRegistry};
use kagami_document::{
    AuthoredValue, CapabilityGap, CapabilitySummary, ExperimentCommand, ExperimentRevision, Limits,
    ObjectSpec,
};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, DocumentTarget, EventSeq, ExperimentChange,
    ExperimentCommandEnvelope, MAX_COMMAND_HISTORY, MAX_EVENT_HISTORY, SaveAcknowledgement,
    SessionCommand,
};
use support::{Adapter, create, mass_component, mass_properties, name, reshaped_schemas, schemas};

fn authority() -> DocumentAuthority {
    DocumentAuthority::new(schemas(), Limits::DEFAULT)
}

#[test]
fn a_new_session_is_empty_clean_and_offers_no_history() {
    let authority = authority();
    assert_eq!(authority.revision().get(), 0);
    assert_eq!(authority.snapshot().object_count(), 0);
    // A brand-new empty experiment has nothing to lose, so it is not dirty.
    assert!(!authority.is_dirty());
    assert!(!authority.history_status().can_undo());
    assert!(!authority.history_status().can_redo());
    assert_eq!(authority.view().open_gesture, None);
}

#[test]
fn replaying_a_command_identity_does_not_apply_it_twice() {
    let mut authority = authority();
    let envelope = ExperimentCommandEnvelope::new(
        CommandId::new("cmd-1").expect("valid"),
        ActorId::new("ui").expect("valid"),
        SessionCommand::Edit(vec![create("Earth")]),
    );

    let first = authority.submit(envelope.clone()).expect("accepted");
    assert!(!first.replayed);

    let second = authority.submit(envelope).expect("replayed");
    assert!(second.replayed);
    assert_eq!(second.revision, first.revision);
    assert_eq!(second.change, first.change);
    assert_eq!(authority.snapshot().object_count(), 1);
    assert_eq!(authority.revision(), first.revision);
}

#[test]
fn a_failed_command_identity_is_not_recorded_so_a_retry_still_runs() {
    let mut authority = authority();
    let id = CommandId::new("cmd-1").expect("valid");
    let actor = ActorId::new("ui").expect("valid");

    // Fails: an empty batch.
    let rejection = authority
        .submit(ExperimentCommandEnvelope::new(
            id.clone(),
            actor.clone(),
            SessionCommand::Edit(Vec::new()),
        ))
        .expect_err("an edit must carry a command");
    assert_eq!(rejection.code(), "empty_batch");

    // The same identity, now carrying a valid batch, must actually run — an
    // adapter retrying after a transient transport failure depends on it.
    let accepted = authority
        .submit(ExperimentCommandEnvelope::new(
            id,
            actor,
            SessionCommand::Edit(vec![create("Earth")]),
        ))
        .expect("accepted");
    assert!(!accepted.replayed);
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn a_refusal_leaves_the_revision_history_and_events_exactly_as_they_were() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    ui.edit(&mut authority, vec![create("Earth")]);

    let before = authority.view();
    let before_events: Vec<_> = authority.events_since(EventSeq::INITIAL).cloned().collect();

    let rejection = authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("bad").expect("valid"),
            ActorId::new("agent").expect("valid"),
            SessionCommand::Edit(vec![ExperimentCommand::AttachComponent {
                object: authority
                    .snapshot()
                    .objects()
                    .keys()
                    .next()
                    .copied()
                    .expect("one object"),
                component: mass_component(),
                // Missing the required `mass`.
                properties: std::collections::BTreeMap::new(),
            }]),
        ))
        .expect_err("the component's schema requires a mass");

    assert_eq!(rejection.code(), "required_property_missing");
    let after = authority.view();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(after.history, before.history);
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(
        authority
            .events_since(EventSeq::INITIAL)
            .cloned()
            .collect::<Vec<_>>(),
        before_events
    );
}

#[test]
fn every_accepted_command_appends_exactly_one_attributed_event() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let mut agent = Adapter::unguarded("agent");

    ui.edit(&mut authority, vec![create("Earth")]);
    agent.edit(&mut authority, vec![create("Moon")]);

    let events: Vec<_> = authority.events_since(EventSeq::INITIAL).collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].actor.as_str(), "ui");
    assert_eq!(events[1].actor.as_str(), "agent");
    assert!(events[1].seq > events[0].seq);
    assert_eq!(events[1].revision, authority.revision());
}

#[test]
fn a_view_catches_up_from_where_it_was() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");

    ui.edit(&mut authority, vec![create("Earth")]);
    let caught_up = authority.view().last_event;

    ui.edit(&mut authority, vec![create("Moon")]);
    ui.edit(&mut authority, vec![create("Mars")]);

    let missed: Vec<_> = authority.events_since(caught_up).collect();
    assert_eq!(missed.len(), 2);
    assert!(authority.can_catch_up_from(caught_up));
}

#[test]
fn a_view_that_fell_too_far_behind_is_told_to_re_read() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    let stale = authority.view().last_event;
    for index in 0..(MAX_EVENT_HISTORY + 8) {
        ui.edit(&mut authority, vec![create(&format!("object {index}"))]);
    }

    // Silently returning a truncated window would let a view believe it is
    // current when it has missed changes.
    assert!(!authority.can_catch_up_from(stale));
    assert!(authority.can_catch_up_from(authority.view().last_event));
}

#[test]
fn dirty_state_is_derived_from_the_revision_last_persisted() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");

    assert!(!authority.is_dirty());
    assert_eq!(authority.target(), None);
    ui.edit(&mut authority, vec![create("Earth")]);
    assert!(authority.is_dirty());

    let target = DocumentTarget::new("/tmp/orbit.kagami").expect("valid target");
    assert_eq!(
        authority.acknowledge_save(authority.revision(), target.clone()),
        SaveAcknowledgement::Clean
    );
    assert!(!authority.is_dirty());
    assert_eq!(authority.clean_revision(), authority.revision());
    assert_eq!(authority.target(), Some(&target));

    ui.edit(&mut authority, vec![create("Moon")]);
    assert!(authority.is_dirty());

    // Undoing back to the persisted revision does *not* clear dirty: the
    // revision moved forward, so the file on disk is not this experiment even
    // though the contents match.
    ui.submit(&mut authority, SessionCommand::Undo)
        .expect("one entry");
    assert!(authority.is_dirty());
}

#[test]
fn a_completion_for_an_older_snapshot_does_not_clean_a_newer_one() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let target = DocumentTarget::new("/tmp/orbit.kagami").expect("valid target");

    // The shell captures revision N and starts writing it.
    ui.edit(&mut authority, vec![create("Earth")]);
    let captured = authority.revision();

    // The user keeps typing while the write is in flight.
    ui.edit(&mut authority, vec![create("Moon")]);
    let current = authority.revision();

    // The write lands. It really did write N — but N+1 is here, and marking
    // that clean would lose the edit made in between.
    assert_eq!(
        authority.acknowledge_save(captured, target.clone()),
        SaveAcknowledgement::Superseded { current }
    );
    assert!(authority.is_dirty());
    assert_eq!(authority.clean_revision().get(), 0);
    // The bytes did land at that target, so the document is now that file.
    assert_eq!(authority.target(), Some(&target));

    // Acknowledging the revision actually in force does clean it.
    assert_eq!(
        authority.acknowledge_save(current, target),
        SaveAcknowledgement::Clean
    );
    assert!(!authority.is_dirty());
}

#[test]
fn a_failed_save_as_changes_neither_the_target_nor_the_dirty_state() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");
    let original = DocumentTarget::new("/tmp/orbit.kagami").expect("valid target");

    ui.edit(&mut authority, vec![create("Earth")]);
    authority.acknowledge_save(authority.revision(), original.clone());
    ui.edit(&mut authority, vec![create("Moon")]);

    // "Save As /read-only/orbit.kagami" fails. A failed write acknowledges
    // nothing, so there is no call to make here — and that is the point: the
    // target is adopted by the acknowledgement, never by starting a write.
    assert_eq!(authority.target(), Some(&original));
    assert!(authority.is_dirty());

    // Two writes completing out of order cannot walk the target backwards
    // either.
    let stale = DocumentTarget::new("/tmp/older.kagami").expect("valid target");
    let acknowledged = authority.clean_revision();
    assert_eq!(
        authority.acknowledge_save(ExperimentRevision::INITIAL, stale),
        SaveAcknowledgement::Stale { acknowledged }
    );
    assert_eq!(authority.target(), Some(&original));
    assert!(authority.is_dirty());
}

#[test]
fn an_identity_that_names_a_different_request_is_refused_rather_than_replayed() {
    let mut authority = authority();
    let id = CommandId::new("cmd-1").expect("valid");
    let envelope = ExperimentCommandEnvelope::new(
        id.clone(),
        ActorId::new("ui").expect("valid"),
        SessionCommand::Edit(vec![create("Earth")]),
    );
    let accepted = authority.submit(envelope.clone()).expect("accepted");

    // The identical request replays.
    let replayed = authority.submit(envelope.clone()).expect("replayed");
    assert!(replayed.replayed);
    assert_eq!(replayed.revision, accepted.revision);

    let before = authority.view();
    // A different actor reusing the identity must not be handed the first
    // actor's outcome, and must not be told what that outcome was.
    let conflicting = [
        // A different actor must not be handed the first actor's outcome.
        ExperimentCommandEnvelope::new(
            id.clone(),
            ActorId::new("agent").expect("valid"),
            SessionCommand::Edit(vec![create("Earth")]),
        ),
        // A different payload under the same identity, likewise.
        ExperimentCommandEnvelope::new(
            id.clone(),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Edit(vec![create("Moon")]),
        ),
        // And a different guard, which describes a different intent even where
        // the batch matches.
        ExperimentCommandEnvelope::new(
            id,
            ActorId::new("ui").expect("valid"),
            SessionCommand::Edit(vec![create("Earth")]),
        )
        .guarded_by(ExperimentRevision::INITIAL),
    ];
    for envelope in conflicting {
        let rejection = authority
            .submit(envelope)
            .expect_err("not the same request");
        assert_eq!(rejection.code(), "command_identity_conflict");
        assert!(
            !rejection.to_string().contains("Earth"),
            "a conflict must not leak the recorded outcome: {rejection}"
        );
    }

    let after = authority.view();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(after.history, before.history);
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn an_identity_the_replay_window_has_forgotten_is_a_new_request() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");
    let id = CommandId::new("cmd-1").expect("valid");
    let actor = ActorId::new("ui").expect("valid");

    authority
        .submit(ExperimentCommandEnvelope::new(
            id.clone(),
            actor.clone(),
            SessionCommand::Edit(vec![create("Earth")]),
        ))
        .expect("accepted");

    // Push it out of the retention window.
    for index in 0..MAX_COMMAND_HISTORY {
        ui.edit(&mut authority, vec![create(&format!("filler {index}"))]);
    }

    // Retention is bounded, so this is not a conflict: the authority no longer
    // knows the identity, and cannot tell a retry from a first attempt.
    let accepted = authority
        .submit(ExperimentCommandEnvelope::new(
            id,
            actor,
            SessionCommand::Edit(vec![create("Moon")]),
        ))
        .expect("a forgotten identity is a new request");
    assert!(!accepted.replayed);
}

#[test]
fn adopting_a_schema_snapshot_changes_availability_and_nothing_else() {
    let mut authority = authority();
    let mut ui = Adapter::guarded("ui");

    let accepted = ui.edit(
        &mut authority,
        vec![ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Earth")).with_component(mass_component(), mass_properties()),
        ))],
    );
    let ExperimentChange::Edited { report } = accepted.change else {
        panic!("an edit reports what it created");
    };
    let id = report.first_created().expect("one object");
    assert!(
        authority
            .acknowledge_save(
                authority.revision(),
                DocumentTarget::new("/tmp/orbit.kagami").expect("valid target"),
            )
            .is_clean()
    );

    let before = authority.view();
    assert!(authority.capabilities().is_complete());

    // The plugin is uninstalled.
    let seq = authority.adopt_schemas(
        SchemaRegistry::new(),
        ActorId::new("plugins").expect("valid"),
    );

    let after = authority.view();
    assert_eq!(after.revision(), before.revision(), "no revision advanced");
    assert_eq!(after.history, before.history, "no history entry");
    assert_eq!(after.dirty, before.dirty, "a clean document stays clean");
    assert!(!after.dirty);
    assert_eq!(
        after.experiment.object(id),
        before.experiment.object(id),
        "the authored bytes are untouched"
    );

    // What changed is the projection, and it is announced.
    assert_eq!(
        authority.capabilities().gap(id, &mass_component()),
        Some(&CapabilityGap::SchemaNotInstalled)
    );
    let event = authority
        .events_since(before.last_event)
        .find(|event| event.seq == seq)
        .expect("one published event");
    assert_eq!(event.command_id, None, "no submission asked for this");
    assert_eq!(event.actor.as_str(), "plugins");
    assert_eq!(
        event.change,
        ExperimentChange::CapabilityChanged {
            before: CapabilitySummary::default(),
            after: CapabilitySummary {
                objects: 1,
                absent: 1,
                incompatible: 0,
            },
        }
    );

    // Reinstalling it restores availability, again without touching the
    // experiment.
    authority.adopt_schemas(schemas(), ActorId::new("plugins").expect("valid"));
    assert!(authority.capabilities().is_complete());
    assert_eq!(authority.revision(), before.revision());
    assert!(!authority.is_dirty());
}

#[test]
fn undo_incompatible_with_the_current_registry_leaves_both_directions_intact() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    // Three edits and one undo, so both history directions hold something and
    // the next undo would restore contents that carry the component.
    ui.edit(
        &mut authority,
        vec![ExperimentCommand::CreateObject(Box::new(
            ObjectSpec::new(name("Earth")).with_component(mass_component(), mass_properties()),
        ))],
    );
    ui.edit(&mut authority, vec![create("Mars")]);
    ui.edit(&mut authority, vec![create("Moon")]);
    ui.submit(&mut authority, SessionCommand::Undo)
        .expect("one entry");

    let before = authority.view();
    assert!(before.history.can_undo() && before.history.can_redo());

    // The plugin is replaced by one whose declaration refuses the stored
    // values. Restoring them would assert numbers that declaration rejects.
    authority.adopt_schemas(reshaped_schemas(), ActorId::new("plugins").expect("valid"));

    let rejection = ui
        .submit(&mut authority, SessionCommand::Undo)
        .expect_err("the captured contents cannot be reinstated");
    assert_eq!(rejection.code(), "component_incompatible");

    let after = authority.view();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(
        after.history, before.history,
        "a refused restore moves neither history direction"
    );
    assert!(after.history.can_undo() && after.history.can_redo());
}

#[test]
fn one_gesture_is_one_undo_entry_however_many_commits_it_makes() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    let accepted = ui.edit(&mut authority, vec![create("Earth")]);
    let ExperimentChange::Edited { report } = accepted.change else {
        panic!("an edit reports what it created");
    };
    let id = report.first_created().expect("one object");
    authority.clear_history();

    let opened = ui
        .submit(&mut authority, SessionCommand::BeginInteractiveEdit)
        .expect("accepted");
    let ExperimentChange::GestureOpened { gesture } = opened.change else {
        panic!("opening a gesture reports its identity");
    };
    // Bracketing changes no contents, so it advances no revision.
    assert_eq!(opened.revision, authority.revision());
    assert_eq!(authority.view().open_gesture, Some(gesture));

    let revision_before_drag = authority.revision();
    for frame in 0..32 {
        let envelope = ExperimentCommandEnvelope::new(
            CommandId::new(format!("drag-{frame}")).expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Edit(vec![ExperimentCommand::RenameObject {
                object: id,
                name: name(&format!("Earth {frame}")),
            }]),
        )
        .within(gesture);
        authority.submit(envelope).expect("accepted");
    }

    // Every frame is its own revision — the experiment really did change 32
    // times — but the drag is one thing to undo.
    assert_eq!(authority.revision().get(), revision_before_drag.get() + 32);
    assert_eq!(authority.history_status().depth, 1);
    assert_eq!(
        authority.history_status().undo.as_deref(),
        Some("Rename object")
    );

    ui.submit(&mut authority, SessionCommand::EndInteractiveEdit)
        .expect("accepted");
    assert_eq!(authority.view().open_gesture, None);

    // Undoing returns to where the gesture started, not to the previous frame.
    ui.submit(&mut authority, SessionCommand::Undo)
        .expect("one entry");
    assert_eq!(
        authority
            .snapshot()
            .object(id)
            .expect("still there")
            .name
            .as_str(),
        "Earth"
    );
}

#[test]
fn a_stale_gesture_identity_is_refused_rather_than_silently_ignored() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    let opened = ui
        .submit(&mut authority, SessionCommand::BeginInteractiveEdit)
        .expect("accepted");
    let ExperimentChange::GestureOpened { gesture } = opened.change else {
        panic!("opening a gesture reports its identity");
    };
    ui.submit(&mut authority, SessionCommand::EndInteractiveEdit)
        .expect("accepted");

    let envelope = ExperimentCommandEnvelope::new(
        CommandId::new("late").expect("valid"),
        ActorId::new("ui").expect("valid"),
        SessionCommand::Edit(vec![create("Earth")]),
    )
    .within(gesture);
    let rejection = authority
        .submit(envelope)
        .expect_err("the gesture finished");

    // Silently treating it as ungestured would be defensible; refusing is
    // better, because the adapter has a bug worth reporting.
    assert_eq!(rejection.code(), "gesture_not_open");
    assert_eq!(authority.snapshot().object_count(), 0);
}

#[test]
fn an_unclosed_gesture_does_not_wedge_the_session() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    let opened = ui
        .submit(&mut authority, SessionCommand::BeginInteractiveEdit)
        .expect("accepted");
    let ExperimentChange::GestureOpened { gesture } = opened.change else {
        panic!("opening a gesture reports its identity");
    };

    // The adapter crashes mid-drag and never sends `EndInteractiveEdit`. The
    // next submission that does not name the gesture closes it, so the rest of
    // the session does not coalesce into one undo entry.
    let after = authority.view().last_event;
    ui.edit(&mut authority, vec![create("Earth")]);
    ui.edit(&mut authority, vec![create("Moon")]);

    assert_eq!(authority.view().open_gesture, None);
    assert_eq!(authority.history_status().depth, 2);

    let closes: Vec<_> = authority
        .events_since(after)
        .filter(|event| {
            matches!(
                event.change,
                ExperimentChange::GestureClosed { implicit: true, .. }
            )
        })
        .collect();
    assert_eq!(
        closes.len(),
        1,
        "the implicit close is reported, not hidden"
    );
    assert_eq!(
        closes[0].change,
        ExperimentChange::GestureClosed {
            gesture,
            implicit: true
        }
    );
}

#[test]
fn closing_a_gesture_that_is_not_open_is_refused() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");
    let rejection = ui
        .submit(&mut authority, SessionCommand::EndInteractiveEdit)
        .expect_err("nothing is open");
    assert_eq!(rejection.code(), "no_gesture_open");
}

#[test]
fn undo_and_redo_are_refused_when_there_is_nothing_to_do() {
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");

    assert_eq!(
        ui.submit(&mut authority, SessionCommand::Undo)
            .expect_err("empty history")
            .code(),
        "nothing_to_undo"
    );
    assert_eq!(
        ui.submit(&mut authority, SessionCommand::Redo)
            .expect_err("empty history")
            .code(),
        "nothing_to_redo"
    );
}

#[test]
fn an_edit_the_installed_plugins_cannot_validate_never_reaches_the_history() {
    // A session with no simulation plugin installed. Attaching a component no
    // schema governs is refused, and the refusal must leave a session that
    // never had a history still without one — a history entry for an edit that
    // did not happen would make the first undo empty the workspace.
    let mut bare = DocumentAuthority::new(SchemaRegistry::new(), Limits::DEFAULT);
    let mut agent = Adapter::unguarded("agent");

    let mut properties = std::collections::BTreeMap::new();
    properties.insert(
        PropertyName::new("mass").expect("valid"),
        AuthoredValue::si("5.972e24"),
    );
    let rejection = agent
        .submit(
            &mut bare,
            SessionCommand::Edit(vec![ExperimentCommand::CreateObject(Box::new(
                ObjectSpec::new(name("Earth")).with_component(mass_component(), properties),
            ))]),
        )
        .expect_err("no plugin contributes that component");

    assert_eq!(rejection.code(), "component_type_not_installed");
    assert_eq!(bare.history_status().depth, 0);
    assert_eq!(bare.snapshot().object_count(), 0);
    assert!(!bare.is_dirty());
}

#[test]
fn clearing_the_history_leaves_the_experiment_untouched() {
    // Session setup — the starting experiment, later a loaded file — authors
    // through the same command path and then clears the history, so the
    // opening undo of a session does not empty the workspace.
    let mut authority = authority();
    let mut ui = Adapter::unguarded("ui");
    ui.edit(&mut authority, vec![create("Earth")]);

    let revision = authority.revision();
    authority.clear_history();

    assert!(!authority.history_status().can_undo());
    assert!(!authority.history_status().can_redo());
    assert_eq!(authority.revision(), revision);
    assert_eq!(authority.snapshot().object_count(), 1);
}

#[test]
fn hostile_identities_are_bounded_before_they_reach_the_authority() {
    // The bound is on the type, so an oversized identity cannot be put in an
    // envelope at all rather than being refused after allocation.
    assert!(CommandId::new("a".repeat(4096)).is_err());
    assert!(ActorId::new(String::new()).is_err());
}

#[test]
fn an_oversized_batch_is_refused_by_the_documents_own_limits() {
    let limits = Limits {
        max_commands_per_batch: 4,
        ..Limits::DEFAULT
    };
    let mut authority = DocumentAuthority::new(schemas(), limits);
    let mut agent = Adapter::unguarded("agent");

    let commands: Vec<_> = (0..8).map(|index| create(&format!("o{index}"))).collect();
    let rejection = agent
        .submit(&mut authority, SessionCommand::Edit(commands))
        .expect_err("over the batch limit");

    assert_eq!(rejection.code(), "batch_too_large");
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.revision().get(), 0);
}

#[test]
fn capability_discovery_reports_what_can_be_attached() {
    let authority = authority();
    let schemas = authority.schemas();
    let component = mass_component();
    let schema = schemas
        .get(&component)
        .expect("the installed plugin's component");
    assert_eq!(
        schema.required_properties().collect::<Vec<_>>(),
        vec![&PropertyName::new("mass").expect("valid")]
    );
    // One answer to "what can be attached", not a derived description that
    // can disagree with the registry an edit is validated against.
    assert_eq!(schemas.schemas().count(), 1);
}

#[test]
fn preflight_declines_commands_that_have_no_batch_to_check() {
    let authority = authority();
    let envelope = ExperimentCommandEnvelope::new(
        CommandId::new("check").expect("valid"),
        ActorId::new("agent").expect("valid"),
        SessionCommand::Undo,
    );
    let rejection = authority
        .preflight(&envelope)
        .expect_err("undo has nothing to validate");
    assert_eq!(rejection.code(), "not_preflightable");
}
