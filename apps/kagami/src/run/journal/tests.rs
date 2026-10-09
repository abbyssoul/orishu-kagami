use super::*;
use crate::files::faults::{self, Action, Event, Phase, serial};
use orishu::model::{node::NodeId, run::*};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

const BYTES: &[u8] = b"frozen workload bytes";

fn formation() -> orishu::model::cluster::FormationId {
    "formation-a".parse().unwrap()
}
fn target() -> Target {
    Target::new("unix:/run/orishu/worker.sock".parse().unwrap(), formation()).unwrap()
}
fn workload() -> orishu_workload::WorkloadDigest {
    format!("sha256:{}", "01".repeat(32)).parse().unwrap()
}
fn source() -> Source {
    Source {
        incarnation: uuid::Uuid::from_u128(7),
        revision: 3,
    }
}
fn load_request() -> LoadRequest {
    LoadRequest::new("kagami-load-1".parse().unwrap(), formation(), workload())
}
fn identity() -> RunIdentity {
    RunIdentity::new(formation(), workload(), WorkloadEpoch::new(1))
}
fn load_receipt(state: LoadState) -> LoadReceipt {
    LoadReceipt::new(load_request(), NodeId::new("node-7f3a").unwrap(), state).unwrap()
}
fn accepted() -> LoadState {
    LoadState::Finished(LoadOutcome::Accepted {
        descriptor: RunDescriptor::new(identity()),
    })
}
fn command_request() -> RunCommandRequest {
    RunCommandRequest::new("kagami-1".parse().unwrap(), identity(), 3, RunCommand::Step).unwrap()
}
fn ledger_bytes(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path.join(LEDGER)).ok()
}
fn bundle_names(path: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(path.join(BUNDLES))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}
fn bundle_name() -> String {
    format!("sha256-{}.okw", "01".repeat(32))
}

#[test]
fn reopening_restores_the_exact_ledger_and_frozen_bytes() {
    let _serial = serial();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state/kagami/runs");
    let mut journal = Journal::open(&path).unwrap();
    assert!(journal.ledger().is_empty());
    journal
        .begin_load(target(), load_request(), source(), BYTES)
        .unwrap();
    journal.begin_command(target(), command_request()).unwrap();
    let expected = journal.ledger().clone();
    drop(journal);

    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(journal.ledger(), &expected);
    assert_eq!(journal.bundle().unwrap(), BYTES);
    assert_eq!(bundle_names(&path), [bundle_name()]);
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);

    // Pending keeps the bytes; a final outcome releases them.
    journal
        .record_load(Some(load_receipt(LoadState::Pending)))
        .unwrap();
    assert_eq!(journal.bundle().unwrap(), BYTES);
    journal.record_load(Some(load_receipt(accepted()))).unwrap();
    assert!(matches!(
        journal.bundle(),
        Err(JournalError::BundleUnavailable)
    ));
    assert!(bundle_names(&path).is_empty());
    journal.clear_load().unwrap();
    drop(journal);
    let journal = Journal::open(&path).unwrap();
    assert!(journal.ledger().load().is_none());
    assert_eq!(
        journal.ledger().command().unwrap().request(),
        &command_request()
    );
}

#[test]
fn a_second_instance_is_refused_until_the_first_closes() {
    let _serial = serial();
    let temp = tempfile::tempdir().unwrap();
    let first = Journal::open(temp.path()).unwrap();
    let error = Journal::open(temp.path()).unwrap_err();
    assert!(matches!(error, JournalError::InUse), "{error:?}");
    assert!(error.to_string().contains("Another Kagami instance"));
    drop(first);
    Journal::open(temp.path()).unwrap();
}

#[test]
fn damaged_or_newer_records_are_refused_and_left_unchanged() {
    let _serial = serial();
    let newer = br#"{"apiVersion":"kagami.run-intents/v2","future":true}"#.to_vec();
    let cases = [
        (b"{\"apiVersion\":".to_vec(), DecodeError::Malformed),
        (newer, DecodeError::UnsupportedVersion),
        (vec![b' '; MAX_LEDGER_BYTES + 1], DecodeError::Oversized),
    ];
    for (bytes, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(LEDGER), &bytes).unwrap();
        let error = Journal::open(temp.path()).unwrap_err();
        assert!(
            matches!(error, JournalError::Damaged(e) if e == expected),
            "{error:?}"
        );
        assert!(error.to_string().contains("does not overwrite"));
        assert_eq!(ledger_bytes(temp.path()).unwrap(), bytes);
    }
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(LEDGER)).unwrap();
    assert!(matches!(
        Journal::open(temp.path()),
        Err(JournalError::Damaged(DecodeError::Malformed))
    ));
}

#[test]
fn refused_transitions_write_nothing() {
    let _serial = serial();
    let temp = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(temp.path()).unwrap();
    assert!(matches!(
        journal.record_load(None),
        Err(JournalError::Refused(IntentError::NoLoad))
    ));
    assert!(ledger_bytes(temp.path()).is_none());
    journal
        .begin_load(target(), load_request(), source(), BYTES)
        .unwrap();
    let before = ledger_bytes(temp.path()).unwrap();
    assert!(matches!(
        journal.begin_load(target(), load_request(), source(), b"other bytes"),
        Err(JournalError::Refused(IntentError::LoadRetained))
    ));
    assert!(matches!(
        journal.clear_load(),
        Err(JournalError::Refused(IntentError::Unresolved))
    ));
    assert_eq!(ledger_bytes(temp.path()).unwrap(), before);
    assert_eq!(journal.bundle().unwrap(), BYTES);
}

#[test]
fn open_removes_interrupted_writes_and_unneeded_bundles() {
    let _serial = serial();
    let temp = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(temp.path()).unwrap();
    journal
        .begin_load(target(), load_request(), source(), BYTES)
        .unwrap();
    drop(journal);
    let bundles = temp.path().join(BUNDLES);
    std::fs::write(temp.path().join(".stage-orphan"), b"{partial").unwrap();
    std::fs::write(bundles.join(".stage-orphan"), b"partial").unwrap();
    std::fs::write(bundles.join("sha256-ff.okw"), b"old upload").unwrap();
    let journal = Journal::open(temp.path()).unwrap();
    assert_eq!(bundle_names(temp.path()), [bundle_name()]);
    assert!(!temp.path().join(".stage-orphan").exists());
    assert_eq!(journal.bundle().unwrap(), BYTES);
}

/// The two journal writes this test drives at every barrier.
fn operate(journal: &mut Journal, operation: &str) -> Result<(), JournalError> {
    match operation {
        "begin" => journal.begin_load(target(), load_request(), source(), BYTES),
        "finish" => journal.record_load(Some(load_receipt(accepted()))),
        _ => panic!("unknown journal test operation"),
    }
}
fn setup(path: &Path, operation: &str) -> Journal {
    let mut journal = Journal::open(path).unwrap();
    if operation == "finish" {
        operate(&mut journal, "begin").unwrap();
    }
    journal
}
fn reference(operation: &str) -> (Ledger, Ledger, Vec<Event>) {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = setup(temp.path(), operation);
    let before = journal.ledger().clone();
    let _guard = faults::arm(Action::Record);
    operate(&mut journal, operation).unwrap();
    (before, journal.ledger().clone(), faults::events())
}
/// A reopened journal holds exactly the old or the new ledger, the new one
/// only when its rename was reached, and the bytes that its record needs.
fn verify(path: &Path, before: &Ledger, after: &Ledger, events: &[Event]) {
    let visible = events
        .iter()
        .any(|e| e.name == LEDGER && e.phase == Phase::Published);
    let journal = Journal::open(path).unwrap();
    assert_eq!(journal.ledger(), if visible { after } else { before });
    match journal.ledger().load().filter(|intent| !intent.is_final()) {
        Some(_) => {
            assert_eq!(journal.bundle().unwrap(), BYTES);
            assert_eq!(bundle_names(path), [bundle_name()]);
        }
        None => assert!(bundle_names(path).is_empty()),
    }
    assert!(std::fs::read_dir(path).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".stage-")
    }));
}

#[test]
fn every_write_barrier_failure_keeps_a_consistent_record() {
    let _serial = serial();
    for operation in ["begin", "finish"] {
        let (before, after, events) = reference(operation);
        assert!(
            events
                .iter()
                .any(|e| e.name == LEDGER && e.phase == Phase::DirectorySynced)
        );
        for step in 1..=events.len() {
            let temp = tempfile::tempdir().unwrap();
            let mut journal = setup(temp.path(), operation);
            let guard = faults::arm(Action::Fail(step));
            let error = operate(&mut journal, operation).unwrap_err();
            drop(guard);
            let failed = &events[step - 1];
            let uncertain = failed.name == LEDGER
                && matches!(failed.phase, Phase::Published | Phase::DirectorySynced);
            assert_eq!(journal.is_poisoned(), uncertain, "{operation}: {failed:?}");
            if uncertain {
                assert!(matches!(
                    operate(&mut journal, operation),
                    Err(JournalError::Poisoned)
                ));
                assert!(error.to_string().len() > 10);
            } else {
                // Nothing new is authoritative: the same instance can retry.
                assert_eq!(journal.ledger(), &before);
                operate(&mut journal, operation).unwrap();
                assert_eq!(journal.ledger(), &after);
            }
            drop(journal);
            let retried = if uncertain {
                &events[..step]
            } else {
                &events[..]
            };
            verify(temp.path(), &before, &after, retried);
        }
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("KAGAMI_TEST_JOURNAL_DIR") else {
        return;
    };
    let operation = std::env::var("KAGAMI_TEST_JOURNAL_OPERATION").unwrap();
    let step = std::env::var("KAGAMI_TEST_JOURNAL_STEP")
        .unwrap()
        .parse()
        .unwrap();
    let mut journal = Journal::open(Path::new(&path)).unwrap();
    let _guard = faults::arm(Action::Pause(step));
    operate(&mut journal, &operation).unwrap();
    panic!("requested crash barrier was not reached");
}

fn kill_at(path: &Path, operation: &str, step: usize) {
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "run::journal::tests::crash_child", "--nocapture"])
            .env("KAGAMI_TEST_JOURNAL_DIR", path)
            .env("KAGAMI_TEST_JOURNAL_OPERATION", operation)
            .env("KAGAMI_TEST_JOURNAL_STEP", step.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap().contains("PUBLICATION-PAUSED") {
                let _ = send.send(());
                break;
            }
        }
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("child must reach requested journal barrier");
    child.0.kill().unwrap();
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child.0.wait().unwrap().signal(), Some(9));
    reader.join().unwrap();
}

#[test]
fn every_write_barrier_survives_abrupt_process_death() {
    let _serial = serial();
    let mut cuts = 0;
    for operation in ["begin", "finish"] {
        let (before, after, events) = reference(operation);
        cuts += events.len();
        for step in 1..=events.len() {
            let temp = tempfile::tempdir().unwrap();
            drop(setup(temp.path(), operation));
            kill_at(temp.path(), operation, step);
            verify(temp.path(), &before, &after, &events[..step]);
        }
    }
    // Bundle and ledger each cross five barriers to begin; finishing writes
    // only the ledger.
    assert_eq!(cuts, 15);
}

#[test]
fn storage_failures_are_reported_as_actionable_reasons() {
    use std::io::{Error as Io, ErrorKind as K};
    let io = |kind| files::Error::Io(Io::from(kind));
    for (error, expected) in [
        (io(K::StorageFull), "full"),
        (io(K::QuotaExceeded), "full"),
        (io(K::ReadOnlyFilesystem), "read-only"),
        (io(K::PermissionDenied), "read-only"),
        (
            files::Error::Uncertain(Box::new(io(K::StorageFull))),
            "full",
        ),
        (files::Error::Busy, "Another Kagami instance"),
        (io(K::Other), "storage failed"),
    ] {
        let message = JournalError::from(error).to_string();
        assert!(message.contains(expected), "{message}");
    }

    let _serial = serial();
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let locked = temp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    // A privileged user ignores directory permissions; nothing to observe then.
    let privileged = std::fs::write(locked.join("probe"), b"").is_ok();
    if !privileged {
        for path in [locked.join("kagami/runs"), locked.clone()] {
            let error = Journal::open(&path).unwrap_err();
            assert!(matches!(error, JournalError::ReadOnly), "{error:?}");
        }
    }
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn bundle_verification_refuses_damaged_bytes() {
    assert!(matches!(
        verify_bundle(BYTES, workload()),
        Err(JournalError::BundleUnavailable)
    ));
}
