use super::*;

fn connection() -> Connection {
    Connection {
        address: "unix:/run/orishu/worker.sock".parse().unwrap(),
        token_file: "/nonexistent/operator.token".into(),
        ca_cert: None,
    }
}
const ACTIONS: [Action; 13] = [
    Action::Discover,
    Action::Observe,
    Action::Refresh,
    Action::Step,
    Action::Finish,
    Action::Reconcile,
    Action::ResubmitOriginal,
    Action::ReconcileLoad,
    Action::ResubmitLoad,
    Action::InspectLoaded,
    Action::ClearLoad,
    Action::Open,
    Action::Close,
];

#[test]
fn without_run_access_every_remote_action_explains_why() {
    let run = Controller::new(None, Recovery::default());
    assert_eq!(run.check_submit(), Err(NO_ACCESS));
    for action in &ACTIONS[..11] {
        assert_eq!(run.check(action), Err(NO_ACCESS), "{action:?}");
    }
    assert!(run.check(&Action::Open).is_ok() && run.check(&Action::Close).is_ok());
}

#[test]
fn without_recording_only_observation_remains_available() {
    let mut run = Controller::new(
        Some(connection()),
        Recovery::Unavailable("The storage is full.".into()),
    );
    assert_eq!(run.recording_blocked(), Some("The storage is full."));
    assert_eq!(run.check_submit(), Err("The storage is full."));
    assert!(run.check(&Action::Discover).is_ok());
    for action in [
        Action::Step,
        Action::Finish,
        Action::Reconcile,
        Action::ReconcileLoad,
        Action::ClearLoad,
    ] {
        assert_eq!(
            run.check(&action),
            Err("The storage is full."),
            "{action:?}"
        );
    }
    run.act(Action::ClearLoad);
    assert_eq!(run.notice, "The storage is full.");
    assert!(!run.is_pending());
}

#[cfg(unix)]
#[test]
fn recorded_intent_is_restored_without_sending_and_bound_to_its_worker() {
    use orishu::model::run::{RunIdentity, WorkloadEpoch};
    let _serial = crate::files::faults::serial();
    let temp = tempfile::tempdir().unwrap();
    let formation: orishu::model::cluster::FormationId = "formation-a".parse().unwrap();
    let workload: orishu_workload::WorkloadDigest =
        format!("sha256:{}", "01".repeat(32)).parse().unwrap();
    let other = Target::new("192.0.2.1:6680".parse().unwrap(), formation.clone()).unwrap();
    let load = LoadRequest::new(
        "kagami-load-1".parse().unwrap(),
        formation.clone(),
        workload,
    );
    let command = RunCommandRequest::new(
        "kagami-1".parse().unwrap(),
        RunIdentity::new(formation, workload, WorkloadEpoch::new(1)),
        3,
        RunCommand::Finish,
    )
    .unwrap();
    let mut journal = journal::Journal::open(temp.path()).unwrap();
    let source = Source {
        incarnation: uuid::Uuid::from_u128(9),
        revision: 4,
    };
    journal
        .begin_load(other.clone(), load.clone(), source, b"bytes")
        .unwrap();
    journal.begin_command(other, command.clone()).unwrap();
    drop(journal);

    let run = Controller::new(Some(connection()), Recovery::open(temp.path()));
    assert!(!run.is_pending(), "nothing is sent automatically");
    assert!(run.notice.contains("Recovered"));
    assert_eq!(run.submission().unwrap().request, load);
    assert_eq!(run.submission().unwrap().source_revision, 4);
    assert_eq!(run.command(), Some(&command));
    for action in [
        Action::Reconcile,
        Action::ReconcileLoad,
        Action::ResubmitLoad,
    ] {
        let reason = run.check(&action).unwrap_err();
        assert!(
            reason.contains("192.0.2.1:6680") && reason.contains("--host"),
            "{reason}"
        );
    }
    assert_eq!(run.check(&Action::Discover), Err(COMMAND_FIRST));
    assert_eq!(run.check_submit(), Err(COMMAND_FIRST));
}
