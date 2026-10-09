use super::*;
use orishu::model::{
    run::{RunDescriptor, RunIdentity, WorkloadEpoch},
    run_command::*,
};
use std::os::unix::fs::PermissionsExt;

fn directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn request(index: usize) -> RunCommandRequest {
    RunCommandRequest::new(
        format!("command-{index}").parse().unwrap(),
        RunIdentity::new(
            "formation-a".parse().unwrap(),
            format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
            WorkloadEpoch::new(1),
        ),
        index as u64,
        RunCommand::Step,
    )
    .unwrap()
}
fn begin(store: &mut CommandReceiptStore, request: RunCommandRequest) -> CommandCompletionTicket {
    let BeginCommand::Started(ticket) = store.begin(request, "node-a".parse().unwrap()).unwrap()
    else {
        panic!("fresh command intent");
    };
    ticket
}
fn applied(request: &RunCommandRequest) -> RunCommandOutcome {
    RunCommandOutcome::Applied {
        status: RunStatus::new(
            RunDescriptor::new(request.run().clone()),
            request.expected_boundary() + 1,
            0.5,
            RunPhase::Ready,
        )
        .unwrap(),
    }
}

#[test]
fn independent_command_history_replays_conflicts_and_recovers_without_execution() {
    let dir = directory();
    let mut loads = ReceiptStore::open(dir.path()).unwrap();
    let load = orishu::model::run_load::LoadRequest::new(
        "command-1".parse().unwrap(),
        request(1).run().formation_id().clone(),
        request(1).run().workload_id(),
    );
    loads
        .begin(load.clone(), "node-a".parse().unwrap())
        .unwrap();
    let load_bytes = std::fs::read(dir.path().join(SNAPSHOT)).unwrap();
    let mut store = CommandReceiptStore::open(dir.path()).unwrap();
    assert!(matches!(
        CommandReceiptStore::open(dir.path()),
        Err(ReceiptStoreError::InUse)
    ));
    let ticket = begin(&mut store, request(1));
    let applied = store.finish(&ticket, applied(&request(1))).unwrap();
    assert!(matches!(
        store.finish(&ticket, RunCommandOutcome::Indeterminate),
        Err(ReceiptStoreError::Ticket)
    ));
    let pending = begin(&mut store, request(2));
    assert!(matches!(
        store
            .begin(request(2), "new-node".parse().unwrap())
            .unwrap(),
        BeginCommand::Replay(_)
    ));
    for changed in [
        RunCommandRequest::new(
            request(1).operation_id().clone(),
            request(1).run().clone(),
            1,
            RunCommand::Finish,
        )
        .unwrap(),
        RunCommandRequest::new(
            request(1).operation_id().clone(),
            request(1).run().clone(),
            2,
            RunCommand::Step,
        )
        .unwrap(),
        RunCommandRequest::new(
            request(1).operation_id().clone(),
            RunIdentity::new(
                request(1).run().formation_id().clone(),
                request(1).run().workload_id(),
                WorkloadEpoch::new(2),
            ),
            1,
            RunCommand::Step,
        )
        .unwrap(),
    ] {
        assert!(matches!(
            store.lookup(&changed),
            Err(ReceiptStoreError::Conflict)
        ));
    }
    drop(store);
    let mut store = CommandReceiptStore::open(dir.path()).unwrap();
    assert_eq!(store.lookup(&request(1)).unwrap(), Some(&applied));
    assert_eq!(
        store.lookup(&request(2)).unwrap().unwrap().state(),
        &RunCommandState::Finished(RunCommandOutcome::Indeterminate)
    );
    assert!(matches!(
        store.finish(&pending, RunCommandOutcome::Indeterminate),
        Err(ReceiptStoreError::Ticket)
    ));
    assert!(
        matches!(store.begin(request(1), "new-node".parse().unwrap()).unwrap(), BeginCommand::Replay(r) if r == applied)
    );
    assert_eq!(
        std::fs::read(dir.path().join(SNAPSHOT)).unwrap(),
        load_bytes
    );
    assert!(loads.lookup(&load).unwrap().unwrap().is_pending());
}

#[test]
fn command_ticket_and_outcome_are_bound_to_store_and_exact_intent() {
    let a = directory();
    let b = directory();
    let mut first = CommandReceiptStore::open(a.path()).unwrap();
    let mut second = CommandReceiptStore::open(b.path()).unwrap();
    let ticket = begin(&mut first, request(1));
    begin(&mut second, request(1));
    assert!(matches!(
        second.finish(&ticket, applied(&request(1))),
        Err(ReceiptStoreError::Ticket)
    ));
    assert!(matches!(
        first.finish(&ticket, applied(&request(2))),
        Err(ReceiptStoreError::Outcome)
    ));
    assert!(first.lookup(&request(1)).unwrap().unwrap().is_pending());
    first.finish(&ticket, applied(&request(1))).unwrap();
}

#[test]
fn command_capacity_keeps_exact_history_without_eviction() {
    let dir = directory();
    let mut store = CommandReceiptStore::open(dir.path()).unwrap();
    let records = (0..MAX_RECEIPTS)
        .map(|i| {
            let request = RunCommandRequest::new(
                format!("{i:064}").parse().unwrap(),
                RunIdentity::new(
                    "f".repeat(128).parse().unwrap(),
                    request(0).run().workload_id(),
                    WorkloadEpoch::new(u64::MAX),
                ),
                u64::MAX - 1,
                RunCommand::Step,
            )
            .unwrap();
            RunCommandReceipt::new(
                request.clone(),
                "n".repeat(128).parse().unwrap(),
                RunCommandState::Finished(applied(&request)),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let last = records.last().unwrap().clone();
    store.replace(records).unwrap();
    assert!(matches!(
        store.begin(request(0), "node-a".parse().unwrap()),
        Err(ReceiptStoreError::Full)
    ));
    assert!(
        matches!(store.begin(last.request().clone(), "node-a".parse().unwrap()).unwrap(), BeginCommand::Replay(r) if r == last)
    );
    drop(store);
    let store = CommandReceiptStore::open(dir.path()).unwrap();
    assert_eq!(store.lookup(last.request()).unwrap(), Some(&last));
}

#[test]
fn every_command_write_failure_preserves_conservative_retry_semantics() {
    for final_write in [false, true] {
        for stage in [
            WriteStage::Created,
            WriteStage::Written,
            WriteStage::Synced,
            WriteStage::Renamed,
            WriteStage::DirectorySynced,
        ] {
            let dir = directory();
            let mut store = CommandReceiptStore::open(dir.path()).unwrap();
            let ticket = final_write.then(|| begin(&mut store, request(1)));
            store.fail_at = Some(stage);
            let failed = if let Some(ticket) = &ticket {
                store.finish(ticket, applied(&request(1))).map(|_| ())
            } else {
                store
                    .begin(request(1), "node-a".parse().unwrap())
                    .map(|_| ())
            };
            assert!(matches!(failed, Err(ReceiptStoreError::Io(_))));
            assert!(matches!(
                store.lookup(&request(1)),
                Err(ReceiptStoreError::Poisoned)
            ));
            drop(store);
            let mut store = CommandReceiptStore::open(dir.path()).unwrap();
            let renamed = matches!(stage, WriteStage::Renamed | WriteStage::DirectorySynced);
            if !final_write && !renamed {
                assert!(store.lookup(&request(1)).unwrap().is_none());
            } else {
                let BeginCommand::Replay(receipt) = store
                    .begin(request(1), "new-node".parse().unwrap())
                    .unwrap()
                else {
                    panic!("never issue execution permission after durable intent");
                };
                let expected = if final_write && renamed {
                    applied(&request(1))
                } else {
                    RunCommandOutcome::Indeterminate
                };
                assert_eq!(receipt.state(), &RunCommandState::Finished(expected));
            }
            assert!(!dir.path().join(RunCommandReceipt::STAGING).exists());
        }
    }
}

#[test]
fn legacy_load_snapshot_encoding_is_unchanged() {
    #[derive(Serialize)]
    enum LegacyVersion {
        #[serde(rename = "orishu.worker-load-receipts/v1")]
        V1,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacySnapshot {
        api_version: LegacyVersion,
        receipts: Vec<LoadReceipt>,
    }
    let record = LoadReceipt::pending(
        orishu::model::run_load::LoadRequest::new(
            "load".parse().unwrap(),
            "formation-a".parse().unwrap(),
            request(0).run().workload_id(),
        ),
        "node-a".parse().unwrap(),
    );
    let legacy = codec::encode(&LegacySnapshot {
        api_version: LegacyVersion::V1,
        receipts: vec![record.clone()],
    })
    .unwrap();
    let current = codec::encode(&Snapshot {
        api_version: LoadReceipt::VERSION.into(),
        receipts: vec![record],
    })
    .unwrap();
    assert_eq!(legacy, current);
}

#[test]
fn command_paths_and_corrupt_profile_fail_closed_without_rewriting_files() {
    use std::os::unix::fs::symlink;
    for name in [
        RunCommandReceipt::SNAPSHOT,
        RunCommandReceipt::STAGING,
        RunCommandReceipt::LOCK,
    ] {
        let dir = directory();
        let outside = dir.path().join("unrelated");
        std::fs::write(&outside, b"untouched").unwrap();
        symlink(&outside, dir.path().join(name)).unwrap();
        assert!(CommandReceiptStore::open(dir.path()).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
    }
    let pending = RunCommandReceipt::pending(request(1), "node-a".parse().unwrap());
    for bytes in [
        codec::encode_receipts(
            &Snapshot {
                api_version: LoadReceipt::VERSION.into(),
                receipts: vec![pending.clone()],
            },
            true,
        )
        .unwrap(),
        codec::encode_receipts(
            &Snapshot {
                api_version: RunCommandReceipt::VERSION.into(),
                receipts: vec![pending.clone(); 2],
            },
            true,
        )
        .unwrap(),
        codec::encode_receipts(
            &Snapshot {
                api_version: RunCommandReceipt::VERSION.into(),
                receipts: vec![pending; MAX_RECEIPTS + 1],
            },
            true,
        )
        .unwrap(),
        vec![0; MAX_RECEIPT_BYTES + 1],
        vec![0xff],
        b"\xa2\x6aapiVersion\x78\x01x\x6aapiVersion\x78\x01x".to_vec(),
    ] {
        let dir = directory();
        let path = dir.path().join(RunCommandReceipt::SNAPSHOT);
        std::fs::write(&path, &bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            CommandReceiptStore::open(dir.path()),
            Err(ReceiptStoreError::Invalid)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
