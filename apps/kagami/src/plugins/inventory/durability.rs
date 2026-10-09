//! Real process interruption and injected-IO evidence at publication barriers.
//! Process death does not simulate loss of the kernel's page cache or power loss.
use super::*;
use crate::files::faults::{self, Action, Event, Phase, serial};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

fn package(version: u8) -> Package {
    let limits = Limits::default();
    let payload = payload_from_json(KnownPoint::Constants, &serde_json::to_vec(&serde_json::json!({
        "scientific":{"name":"org.example.durable.constants","version":1,"requirements":[],
        "constants":[{"id":"number","dimension":[0,0,0,0,0,0,0],"valueSI":version,"meaning":"test constant"}]}
    })).unwrap(), &limits).unwrap().canonical_bytes(&limits).unwrap();
    let digest = ArtifactDigest::sha256_of(&payload);
    let root = Release::new(
        ReleaseMetadata {
            plugin_id: "org.example.durable".parse().unwrap(),
            version_label: version.to_string(),
            display_name: None,
            description: None,
        },
        ReleaseSpec {
            contributions: vec![Contribution {
                local_id: "constants".parse().unwrap(),
                extension_point: KnownPoint::Constants.as_str().parse().unwrap(),
                payload: digest,
                requirements: vec![],
                annotations: None,
            }],
            artifacts: vec![Artifact {
                digest,
                size_bytes: payload.len() as u64,
                media_type: "application/cbor".into(),
            }],
        },
    );
    Package::from_bundle(
        &bundle::pack(
            &root,
            &BTreeMap::from([(digest, payload.as_slice())]),
            &limits,
            Default::default(),
        )
        .unwrap(),
    )
    .unwrap()
}

const OPERATIONS: &[&str] = &[
    "install",
    "add",
    "update",
    "reinstall",
    "disable",
    "enable",
    "default",
    "remove",
    "legacy-disable",
];

fn setup(path: &Path, operation: &str) -> PluginStore {
    let store = PluginStore::open(path).unwrap();
    if operation != "install" {
        store
            .install(
                0,
                &package(if operation == "reinstall" { 2 } else { 1 }),
                None,
                None,
            )
            .unwrap();
    }
    if matches!(operation, "default" | "remove") {
        store.install(1, &package(2), None, None).unwrap();
    }
    if operation == "enable" {
        store
            .submit(
                1,
                InventoryCommand::SetEnabled {
                    plugin_id: "org.example.durable".parse().unwrap(),
                    enabled: false,
                },
            )
            .unwrap();
    }
    if operation == "legacy-disable" {
        let mut index = store.index().unwrap();
        index.api_version = Version::V1;
        store
            .dir
            .replace("index.json", &serde_json::to_vec(&index).unwrap())
            .unwrap();
    }
    store
}

fn execute(store: &PluginStore, operation: &str, revision: u64) -> Result<u64, Error> {
    let plugin_id = "org.example.durable".parse().unwrap();
    let new = package(2);
    match operation {
        "install" | "add" | "reinstall" => store.install(revision, &new, None, None),
        "update" => store.install(revision, &new, Some(&plugin_id), None),
        "disable" | "enable" | "legacy-disable" => store.submit(
            revision,
            InventoryCommand::SetEnabled {
                plugin_id,
                enabled: operation == "enable",
            },
        ),
        "default" => store.submit(
            revision,
            InventoryCommand::SetDefault {
                plugin_id,
                release: new.release().id(),
            },
        ),
        "remove" => store.submit(
            revision,
            InventoryCommand::Remove {
                plugin_id,
                release: new.release().id(),
                ack_open_references: false,
            },
        ),
        _ => panic!("unknown test operation"),
    }
}

fn snapshot(store: &PluginStore) -> serde_json::Value {
    serde_json::to_value(store.list().unwrap()).unwrap()
}

fn reference(operation: &str) -> (serde_json::Value, serde_json::Value, Vec<Event>) {
    let temp = tempfile::tempdir().unwrap();
    let store = setup(temp.path(), operation);
    let before = snapshot(&store);
    let _guard = faults::arm(Action::Record);
    execute(&store, operation, store.list().unwrap().revision).unwrap();
    (before, snapshot(&store), faults::events())
}

fn verify_and_reconcile(
    path: &Path,
    operation: &str,
    before: &serde_json::Value,
    after: &serde_json::Value,
    events: &[Event],
) {
    // Interrupted staging is never interpreted as an index or cache entry.
    std::fs::write(path.join(".stage-orphan"), b"{partial").unwrap();
    std::fs::write(path.join("blobs/.stage-orphan"), b"partial").unwrap();
    let store = PluginStore::open(path).unwrap();
    let visible = events
        .iter()
        .any(|event| event.name == "index.json" && event.phase == Phase::Published);
    assert_eq!(&snapshot(&store), if visible { after } else { before });
    if operation == "legacy-disable" {
        assert_eq!(
            store.index().unwrap().api_version,
            if visible { Version::V2 } else { Version::V1 }
        );
    }
    for entry in store.list().unwrap().releases {
        assert_eq!(
            store.inspect(entry.release).unwrap().release().id(),
            entry.release
        );
    }
    let old_revision = before["revision"].as_u64().unwrap();
    if visible {
        assert_eq!(
            execute(&store, operation, old_revision).unwrap_err().code,
            Code::StaleRevision
        );
    } else {
        execute(&store, operation, old_revision).unwrap();
    }
    assert_eq!(snapshot(&store), *after);
    assert_eq!(store.index().unwrap().api_version, Version::V2);
    for entry in store.list().unwrap().releases {
        store.inspect(entry.release).unwrap();
    }
}

#[test]
fn every_inventory_publication_barrier_preserves_a_complete_revision_on_io_error() {
    let _serial = serial();
    let mut cuts = 0;
    for operation in OPERATIONS {
        let (before, after, events) = reference(operation);
        cuts += events.len();
        assert!(
            events
                .iter()
                .any(|e| e.name == "index.json" && e.phase == Phase::DirectorySynced)
        );
        for step in 1..=events.len() {
            let temp = tempfile::tempdir().unwrap();
            let store = setup(temp.path(), operation);
            let guard = faults::arm(Action::Fail(step));
            let error = execute(&store, operation, store.list().unwrap().revision).unwrap_err();
            assert_eq!(
                error.code,
                Code::IoFailure,
                "{operation}: {:?}",
                events[step - 1]
            );
            if events[step - 1].phase == Phase::Published
                || events[step - 1].phase == Phase::DirectorySynced
            {
                assert!(error.message.contains("inspect"));
            }
            assert_eq!(faults::events(), events[..step]);
            drop(guard);
            drop(store);
            verify_and_reconcile(temp.path(), operation, &before, &after, &events[..step]);
        }
    }
    assert_eq!(cuts, 79);
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn kill_at(path: &Path, operation: &str, step: usize) {
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "plugins::inventory::durability::crash_child",
                "--nocapture",
            ])
            .env("KAGAMI_TEST_CRASH_STORE", path)
            .env("KAGAMI_TEST_CRASH_OPERATION", operation)
            .env("KAGAMI_TEST_CRASH_STEP", step.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap().contains("PLUGIN-PUBLICATION-PAUSED") {
                let _ = send.send(());
                break;
            }
        }
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("child must reach requested publication barrier");
    child.0.kill().unwrap();
    let status = child.0.wait().unwrap();
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(9));
    reader.join().unwrap();
}

#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("KAGAMI_TEST_CRASH_STORE") else {
        return;
    };
    let operation = std::env::var("KAGAMI_TEST_CRASH_OPERATION").unwrap();
    let step = std::env::var("KAGAMI_TEST_CRASH_STEP")
        .unwrap()
        .parse()
        .unwrap();
    if operation == "pack" {
        let _guard = faults::arm(Action::Pause(step));
        package(2)
            .write_bundle(&Path::new(&path).join("output.okplugin"))
            .unwrap();
        panic!("requested pack barrier was not reached");
    }
    let store = PluginStore::open(Path::new(&path)).unwrap();
    let revision = store.list().unwrap().revision;
    let _guard = faults::arm(Action::Pause(step));
    execute(&store, &operation, revision).unwrap();
    panic!("requested crash barrier was not reached");
}

#[test]
fn bundle_publication_survives_each_error_and_process_cut_without_overwriting() {
    let _serial = serial();
    let reference = tempfile::tempdir().unwrap();
    let expected = package(2).pack().unwrap();
    let guard = faults::arm(Action::Record);
    package(2)
        .write_bundle(&reference.path().join("output.okplugin"))
        .unwrap();
    let events = faults::events();
    assert_eq!(
        events.len(),
        6,
        "include link, staging unlink and directory flush"
    );
    drop(guard);
    for crash in [false, true] {
        for step in 1..=events.len() {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("output.okplugin");
            if crash {
                kill_at(temp.path(), "pack", step);
            } else {
                let _guard = faults::arm(Action::Fail(step));
                let error = package(2).write_bundle(&output).unwrap_err();
                assert_eq!(error.code, Code::IoFailure);
                if step >= 4 {
                    assert!(error.message.contains("inspect"));
                }
            }
            if step < 4 {
                assert!(!output.exists());
                package(2).write_bundle(&output).unwrap();
            } else {
                assert_eq!(
                    package(2).write_bundle(&output).unwrap_err().code,
                    Code::InvalidSelection
                );
            }
            assert_eq!(std::fs::read(&output).unwrap(), expected);
            assert_eq!(
                Package::load_bundle(&output).unwrap().release().id(),
                package(2).release().id()
            );
            assert!(!temp.path().join("index.json").exists());
        }
    }
}

#[test]
fn every_inventory_publication_barrier_survives_abrupt_process_death() {
    let _serial = serial();
    let mut cuts = 0;
    for operation in OPERATIONS {
        let (before, after, events) = reference(operation);
        cuts += events.len();
        for step in 1..=events.len() {
            let temp = tempfile::tempdir().unwrap();
            drop(setup(temp.path(), operation));
            kill_at(temp.path(), operation, step);
            verify_and_reconcile(temp.path(), operation, &before, &after, &events[..step]);
        }
    }
    assert_eq!(cuts, 79);
}

#[test]
fn retry_reestablishes_cache_barriers_before_publishing_the_index() {
    let _serial = serial();
    let temp = tempfile::tempdir().unwrap();
    let store = setup(temp.path(), "install");
    let (_, _, events) = reference("install");
    // Interrupt after the release root rename but before its directory flush.
    let root = hex(package(2).release().id());
    let cut = events
        .iter()
        .position(|e| e.name == root && e.phase == Phase::Published)
        .unwrap()
        + 1;
    let guard = faults::arm(Action::Fail(cut));
    execute(&store, "install", 0).unwrap_err();
    assert!(store.list().unwrap().releases.is_empty());
    drop(guard);
    let _guard = faults::arm(Action::Record);
    execute(&store, "install", 0).unwrap();
    let events = faults::events();
    let index_start = events.iter().position(|e| e.name == "index.json").unwrap();
    assert_eq!(
        index_start, 4,
        "blob and root must each flush file and parent"
    );
    for pair in events[..index_start].chunks_exact(2) {
        assert_eq!(pair[0].phase, Phase::ExistingFileSynced);
        assert_eq!(pair[1].phase, Phase::ExistingDirectorySynced);
        assert_eq!(pair[0].name, pair[1].name);
    }
    assert_eq!(
        store
            .inspect(package(2).release().id())
            .unwrap()
            .release()
            .id(),
        package(2).release().id()
    );
}
