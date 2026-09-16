use super::*;
use orishu::model::run::{RunDescriptor, RunIdentity, WorkloadEpoch};
use std::os::unix::fs::{PermissionsExt, symlink};

fn private_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn request(index: usize) -> LoadRequest {
    LoadRequest::new(
        format!("load-{index}").parse().unwrap(),
        "formation-a".parse().unwrap(),
        format!("sha256:{}", "00".repeat(32)).parse().unwrap(),
    )
}
fn begin(store: &mut ReceiptStore, request: LoadRequest) -> CompletionTicket {
    match store.begin(request, "node-a".parse().unwrap()).unwrap() {
        BeginLoad::Started(ticket) => ticket,
        _ => panic!("expected new durable reservation"),
    }
}
fn accepted(request: &LoadRequest) -> LoadOutcome {
    LoadOutcome::Accepted {
        descriptor: RunDescriptor::new(RunIdentity::new(
            request.formation_id().clone(),
            request.workload_id(),
            WorkloadEpoch::new(1),
        )),
    }
}
fn write_snapshot(path: &Path, receipts: Vec<LoadReceipt>) {
    let directory = credentials::open_private_directory(path).unwrap();
    let bytes = codec::encode(&Snapshot {
        api_version: Version::V1,
        receipts,
    })
    .unwrap();
    credentials::create_private(&directory, SNAPSHOT)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
}

#[test]
fn exact_replay_conflict_terminal_history_and_crash_recovery() {
    let dir = private_dir();
    let mut store = ReceiptStore::open(dir.path()).unwrap();
    let ticket = begin(&mut store, request(1));
    assert_eq!(ticket.receipt().state(), &LoadState::Pending);
    assert!(
        matches!(store.begin(request(1), "new-node".parse().unwrap()).unwrap(), BeginLoad::Replay(receipt) if receipt == *ticket.receipt())
    );
    let other_root = LoadRequest::new(
        request(1).operation_id().clone(),
        request(1).formation_id().clone(),
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
    );
    assert!(matches!(
        store.begin(other_root.clone(), "node-a".parse().unwrap()),
        Err(ReceiptStoreError::Conflict)
    ));
    let accepted = store.finish(&ticket, accepted(&request(1))).unwrap();
    assert!(matches!(
        store.finish(&ticket, LoadOutcome::Indeterminate),
        Err(ReceiptStoreError::Ticket)
    ));
    let pending = begin(&mut store, request(2));
    let refused_ticket = begin(&mut store, request(3));
    let refused = store
        .finish(
            &refused_ticket,
            LoadOutcome::Refused {
                reason: orishu::model::run_load::LoadRefusal::Cancelled,
            },
        )
        .unwrap();
    drop(store);
    let mut store = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(store.lookup(&request(1)).unwrap(), Some(&accepted));
    assert_eq!(store.lookup(&request(3)).unwrap(), Some(&refused));
    assert_eq!(
        store.lookup(&request(2)).unwrap().unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
    assert!(matches!(
        store.begin(request(2), "node-b".parse().unwrap()).unwrap(),
        BeginLoad::Replay(_)
    ));
    assert!(matches!(
        store.lookup(&other_root),
        Err(ReceiptStoreError::Conflict)
    ));
    assert!(matches!(
        store.finish(&pending, accepted_state()),
        Err(ReceiptStoreError::Ticket)
    ));
    // Operation IDs are formation scoped; history of the old formation persists.
    let new_formation = LoadRequest::new(
        request(1).operation_id().clone(),
        "formation-b".parse().unwrap(),
        request(1).workload_id(),
    );
    begin(&mut store, new_formation);
    drop(store);
    let store = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(
        store.lookup(&request(2)).unwrap().unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
}
fn accepted_state() -> LoadOutcome {
    accepted(&request(1))
}

#[test]
fn tickets_cannot_complete_other_stores_or_cross_identity_executions() {
    let first = private_dir();
    let second = private_dir();
    let mut a = ReceiptStore::open(first.path()).unwrap();
    let mut b = ReceiptStore::open(second.path()).unwrap();
    let a_ticket = begin(&mut a, request(1));
    let b_ticket = begin(&mut b, request(1));
    assert!(matches!(
        b.finish(&a_ticket, accepted_state()),
        Err(ReceiptStoreError::Ticket)
    ));
    let wrong = LoadRequest::new(
        "load-1".parse().unwrap(),
        "wrong-formation".parse().unwrap(),
        request(1).workload_id(),
    );
    assert!(matches!(
        a.finish(&a_ticket, accepted(&wrong)),
        Err(ReceiptStoreError::Outcome)
    ));
    assert_eq!(
        a.lookup(&request(1)).unwrap().unwrap().state(),
        &LoadState::Pending
    );
    a.finish(&a_ticket, accepted_state()).unwrap();
    b.finish(&b_ticket, accepted_state()).unwrap();
}

#[test]
fn bounded_history_keeps_replays_and_conflicts_without_eviction() {
    let dir = private_dir();
    let mut store = ReceiptStore::open(dir.path()).unwrap();
    // Maximum field lengths/accepted shape must still fit byte and preflight work
    // budgets at capacity. This uses the actual writer once, not 256 fsync loops.
    let receipts = (0..MAX_RECEIPTS)
        .map(|i| {
            let request = LoadRequest::new(
                format!("{i:064}").parse().unwrap(),
                "f".repeat(128).parse().unwrap(),
                request(0).workload_id(),
            );
            LoadReceipt::new(
                request.clone(),
                "n".repeat(128).parse().unwrap(),
                LoadState::Finished(accepted(&request)),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let last = receipts.last().unwrap().clone();
    store.replace(receipts).unwrap();
    assert!(matches!(
        store.begin(request(0), "node-a".parse().unwrap()),
        Err(ReceiptStoreError::Full)
    ));
    assert!(
        matches!(store.begin(last.request().clone(), "node-a".parse().unwrap()).unwrap(), BeginLoad::Replay(receipt) if receipt == last)
    );
    let conflict = LoadRequest::new(
        last.request().operation_id().clone(),
        last.request().formation_id().clone(),
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
    );
    assert!(matches!(
        store.lookup(&conflict),
        Err(ReceiptStoreError::Conflict)
    ));
    drop(store);
    let store = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(store.lookup(last.request()).unwrap(), Some(&last));
}

#[test]
fn persistence_faults_never_issue_a_ticket_and_poison_until_reopen() {
    for stage in [
        WriteStage::Created,
        WriteStage::Written,
        WriteStage::Synced,
        WriteStage::Renamed,
        WriteStage::DirectorySynced,
    ] {
        let dir = private_dir();
        let mut store = ReceiptStore::open(dir.path()).unwrap();
        store.fail_at = Some(stage);
        assert!(matches!(
            store.begin(request(1), "node-a".parse().unwrap()),
            Err(ReceiptStoreError::Io(_))
        ));
        assert!(matches!(
            store.lookup(&request(1)),
            Err(ReceiptStoreError::Poisoned)
        ));
        assert!(matches!(
            store.begin(request(2), "node-a".parse().unwrap()),
            Err(ReceiptStoreError::Poisoned)
        ));
        drop(store);
        let store = ReceiptStore::open(dir.path()).unwrap();
        let recovered = store.lookup(&request(1)).unwrap();
        if matches!(stage, WriteStage::Renamed | WriteStage::DirectorySynced) {
            assert_eq!(
                recovered.unwrap().state(),
                &LoadState::Finished(LoadOutcome::Indeterminate)
            );
        } else {
            assert!(recovered.is_none());
        }
        assert!(!dir.path().join(STAGING).exists());
    }
}

#[test]
fn uncertain_final_writes_recover_history_never_fresh_permission() {
    for stage in [
        WriteStage::Created,
        WriteStage::Written,
        WriteStage::Synced,
        WriteStage::Renamed,
        WriteStage::DirectorySynced,
    ] {
        let dir = private_dir();
        let mut store = ReceiptStore::open(dir.path()).unwrap();
        let ticket = begin(&mut store, request(1));
        store.fail_at = Some(stage);
        assert!(matches!(
            store.finish(&ticket, accepted_state()),
            Err(ReceiptStoreError::Io(_))
        ));
        assert!(matches!(
            store.finish(&ticket, accepted_state()),
            Err(ReceiptStoreError::Poisoned)
        ));
        drop(store);
        let mut store = ReceiptStore::open(dir.path()).unwrap();
        let BeginLoad::Replay(receipt) = store
            .begin(request(1), "new-node".parse().unwrap())
            .unwrap()
        else {
            panic!("crash must never reexecute");
        };
        let expected = if matches!(stage, WriteStage::Renamed | WriteStage::DirectorySynced) {
            accepted_state()
        } else {
            LoadOutcome::Indeterminate
        };
        assert_eq!(receipt.state(), &LoadState::Finished(expected));
    }
}

#[test]
fn corrupt_unknown_duplicate_oversized_and_excess_count_snapshots_fail_closed() {
    let pending =
        LoadReceipt::new(request(1), "node-a".parse().unwrap(), LoadState::Pending).unwrap();
    for receipts in [
        vec![pending.clone(), pending.clone()],
        vec![pending; MAX_RECEIPTS + 1],
    ] {
        let dir = private_dir();
        write_snapshot(dir.path(), receipts);
        assert!(matches!(
            ReceiptStore::open(dir.path()),
            Err(ReceiptStoreError::Invalid)
        ));
    }
    for bytes in [
        vec![],
        vec![0xff],
        vec![0; MAX_RECEIPT_BYTES + 1],
        vec![0x9b, 255, 255, 255, 255, 255, 255, 255, 255],
    ] {
        let dir = private_dir();
        let directory = credentials::open_private_directory(dir.path()).unwrap();
        credentials::create_private(&directory, SNAPSHOT)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        assert!(matches!(
            ReceiptStore::open(dir.path()),
            Err(ReceiptStoreError::Invalid)
        ));
        assert_eq!(
            std::fs::read(dir.path().join(SNAPSHOT)).unwrap(),
            bytes,
            "never replace corrupt history"
        );
    }
    let dir = private_dir();
    let directory = credentials::open_private_directory(dir.path()).unwrap();
    let bytes = codec::encode(
        &serde_json::json!({"apiVersion":"orishu.worker-load-receipts/v2", "receipts":[]}),
    )
    .unwrap();
    credentials::create_private(&directory, SNAPSHOT)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    assert!(matches!(
        ReceiptStore::open(dir.path()),
        Err(ReceiptStoreError::Invalid)
    ));
}

#[test]
fn private_paths_and_single_writer_are_enforced() {
    let dir = private_dir();
    let store = ReceiptStore::open(dir.path()).unwrap();
    assert!(matches!(
        ReceiptStore::open(dir.path()),
        Err(ReceiptStoreError::InUse)
    ));
    assert_eq!(
        std::fs::metadata(dir.path().join(SNAPSHOT))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(store);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        ReceiptStore::open(dir.path()),
        Err(ReceiptStoreError::UnsafePath)
    ));
    for name in [SNAPSHOT, STAGING, LOCK] {
        for kind in ["symlink", "hardlink", "public", "directory"] {
            let dir = private_dir();
            let target = dir.path().join(name);
            let outside = dir.path().join("unrelated");
            std::fs::write(&outside, b"untouched").unwrap();
            std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o600)).unwrap();
            match kind {
                "symlink" => symlink(&outside, &target).unwrap(),
                "hardlink" => std::fs::hard_link(&outside, &target).unwrap(),
                "directory" => std::fs::create_dir(&target).unwrap(),
                _ => {
                    std::fs::write(&target, b"").unwrap();
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
            }
            assert!(ReceiptStore::open(dir.path()).is_err(), "{name} {kind}");
            assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
            assert!(
                std::fs::symlink_metadata(&target).is_ok(),
                "unsafe target must not be unlinked"
            );
        }
    }
    let outer = private_dir();
    let inner = private_dir();
    symlink(inner.path(), outer.path().join("alias")).unwrap();
    assert!(ReceiptStore::open(&outer.path().join("alias")).is_err());
}

#[test]
fn recovery_discards_only_unacknowledged_staging_and_coexists_with_credentials() {
    let dir = private_dir();
    let credentials = credentials::WorkerCredentials::load_or_create(dir.path()).unwrap();
    let directory = credentials::open_private_directory(dir.path()).unwrap();
    credentials::create_private(&directory, STAGING)
        .unwrap()
        .write_all(b"interrupted initial write")
        .unwrap();
    let mut store = ReceiptStore::open(dir.path()).unwrap();
    begin(&mut store, request(1));
    assert!(!dir.path().join(STAGING).exists());
    assert!(credentials::WorkerCredentials::load_or_create(dir.path()).is_err());
    drop(store);
    drop(credentials);
    let _credentials = credentials::WorkerCredentials::load_or_create(dir.path()).unwrap();
    let store = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(
        store.lookup(&request(1)).unwrap().unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
}

#[test]
fn extra_receipt_is_refused_without_deserializing_it_even_without_a_size_hint() {
    use serde::de::value::{Error, SeqAccessDeserializer};

    struct Explode;
    impl<'de> de::Deserializer<'de> for Explode {
        type Error = Error;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
            panic!("the excess receipt must not be deserialized")
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
            map struct enum identifier ignored_any
        }
    }
    struct Entries(usize);
    impl<'de> SeqAccess<'de> for Entries {
        type Error = Error;
        fn next_element_seed<T: de::DeserializeSeed<'de>>(
            &mut self,
            seed: T,
        ) -> Result<Option<T::Value>, Error> {
            if self.0 == MAX_RECEIPTS {
                return seed.deserialize(Explode).map(Some);
            }
            let receipt = LoadReceipt::new(
                request(self.0),
                "node-a".parse().unwrap(),
                LoadState::Pending,
            )
            .unwrap();
            self.0 += 1;
            seed.deserialize(serde_json::to_value(receipt).unwrap())
                .map(Some)
                .map_err(de::Error::custom)
        }
    }
    let error = bounded_receipts(SeqAccessDeserializer::new(Entries(0))).unwrap_err();
    assert!(error.to_string().contains("count exceeded"));
}
