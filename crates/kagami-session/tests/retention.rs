//! Cold scientific admission shares one retention rule across edits and file Open.
use kagami_document::{ExperimentCommand as Edit, Limits, Setup, scientific::ScientificRetention};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
};
use orishu_plugin::archive::{self, Root};
use std::collections::BTreeMap;

fn draft() -> kagami_session::ExperimentDocument {
    let bytes = archive::pack(
        Root::Document,
        include_bytes!("fixtures/experiment_scientific_v4.json"),
        &BTreeMap::new(),
        kagami_session::container::ContainerLimits::default().archive,
    )
    .unwrap();
    kagami_session::decode_document(&bytes).unwrap()
}
fn envelope(n: usize, command: SessionCommand) -> ExperimentCommandEnvelope {
    ExperimentCommandEnvelope::new(
        CommandId::new(format!("test-{n}")).unwrap(),
        ActorId::new("test").unwrap(),
        command,
    )
}
fn open(document: kagami_session::ExperimentDocument) -> SessionCommand {
    SessionCommand::Open {
        experiment: Box::new(
            document
                .into_experiment(&kagami_catalog::SchemaRegistry::new(), &Limits::default())
                .unwrap(),
        ),
        target: None,
        discard_unsaved: true,
    }
}
fn weight(document: &kagami_session::ExperimentDocument) -> usize {
    let mut tally = ScientificRetention::new(usize::MAX);
    tally
        .include(document.experiment.setup.scientific().unwrap())
        .unwrap();
    tally.bytes()
}

#[test]
fn repeated_independent_opens_stay_bounded_and_keep_the_newest_receipt() {
    let document = draft();
    let mut limits = Limits::default();
    limits.scientific.retained_bytes = weight(&document);
    let mut authority = DocumentAuthority::new(kagami_catalog::SchemaRegistry::new(), limits);
    for n in 0..300 {
        let request = envelope(n, open(draft())).guarded_by(authority.revision());
        authority.submit(request.clone()).unwrap();
        assert_eq!(
            authority.scientific_retained_bytes().unwrap(),
            limits.scientific.retained_bytes
        );
        assert!(authority.submit(request).unwrap().replayed);
    }
}

#[test]
fn explicit_history_release_allows_a_capture_without_losing_the_new_receipt() {
    let original = draft();
    let mut limits = Limits::default();
    // An empty scientific selection still has domain/selection metadata worth
    // retaining. Exercise limits without fabricating a numerical kernel blob.
    limits.scientific.retained_bytes = weight(&original);
    let mut authority = DocumentAuthority::new(kagami_catalog::SchemaRegistry::new(), limits);
    authority.submit(envelope(0, open(original))).unwrap();
    let Setup::Scientific(setup) = draft().experiment.setup else {
        unreachable!()
    };
    let capture = envelope(
        1,
        SessionCommand::Edit(vec![Edit::AdoptScientificSetup(setup)]),
    );
    assert_eq!(
        authority.preflight(&capture).unwrap_err().code(),
        "scientific_retention_limit"
    );
    assert_eq!(
        authority.submit(capture.clone()).unwrap_err().code(),
        "scientific_retention_limit"
    );
    // Replacing with a new document is an explicit lifecycle decision. It
    // releases the old current state, and later byte-pressure eviction can drop
    // its replay receipt. No hidden history trimming is used by admission.
    authority
        .submit(envelope(
            2,
            SessionCommand::New {
                discard_unsaved: true,
            },
        ))
        .unwrap();
    authority.submit(capture.clone()).unwrap();
    assert!(authority.submit(capture).unwrap().replayed);
    assert_eq!(
        authority.scientific_retained_bytes().unwrap(),
        limits.scientific.retained_bytes
    );
    authority.submit(envelope(3, SessionCommand::Undo)).unwrap();
    assert!(authority.snapshot().setup().legacy().is_some());
    assert_eq!(
        authority.scientific_retained_bytes().unwrap(),
        limits.scientific.retained_bytes
    );
    authority.submit(envelope(4, SessionCommand::Redo)).unwrap();
    assert!(authority.snapshot().setup().scientific().is_some());
}
