use super::*;
use crate::{
    workload_load::*,
    workload_receipts::{ReceiptStore, ReceiptStoreError},
};
use orishu::model::{
    run::{RunIdentity, WorkloadEpoch},
    run_load::*,
};
use std::{
    io,
    os::unix::fs::PermissionsExt,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};

struct NeverRead;
impl AsyncRead for NeverRead {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        panic!("replayed/refused body must not be polled")
    }
}
fn directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn load_request(
    service: &AdmissionService,
    id: &str,
    root: orishu_workload::WorkloadDigest,
) -> LoadRequest {
    LoadRequest::new(
        id.parse().unwrap(),
        service.owner.view().unwrap().summary.formation_id,
        root,
    )
}
fn dummy_root() -> orishu_workload::WorkloadDigest {
    format!("sha256:{}", "00".repeat(32)).parse().unwrap()
}
fn coordinator(service: &AdmissionService, dir: &tempfile::TempDir) -> LoadCoordinator {
    LoadCoordinator::new(
        service.clone(),
        ReceiptStore::open(dir.path()).unwrap(),
        Default::default(),
        Duration::from_secs(60),
    )
    .unwrap()
}
async fn pending(coordinator: &LoadCoordinator, request: &LoadRequest) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if coordinator.lookup(request).unwrap().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn idle(coordinator: &LoadCoordinator) {
    tokio::time::timeout(Duration::from_secs(90), coordinator.wait_admission())
        .await
        .unwrap();
}

#[tokio::test]
async fn daemon_retains_run_after_response_and_client_handles_are_dropped() {
    use crate::{credentials::WorkerCredentials, runtime::WorkerRuntime};
    use orishu::model::cluster::LockRequest;
    let _slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let dir = directory();
    let credentials = WorkerCredentials::load_or_create(dir.path()).unwrap();
    let (runtime, owner) = WorkerRuntime::standalone(
        credentials,
        "worker".parse().unwrap(),
        "cluster".parse().unwrap(),
        vec![],
    )
    .unwrap()
    .start();
    assert!(runtime.load_coordinator().is_none());
    let formation = runtime.summary().unwrap().formation_id;
    runtime
        .lock_operation(LockRequest {
            schema_version: 1,
            operation_id: "lock".parse().unwrap(),
            formation_id: formation.clone(),
            locked: true,
        })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let sandbox = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let service = runtime
        .scientific_admission(
            sandbox.clone(),
            Default::default(),
            Default::default(),
            BTreeSet::new(),
        )
        .unwrap();
    let coord = runtime
        .install_load_coordinator(
            ReceiptStore::open(dir.path()).unwrap(),
            sandbox,
            LoadPolicy::default(),
        )
        .unwrap();
    let request = LoadRequest::new("lost-response".parse().unwrap(), formation.clone(), root);
    let submission = coord
        .submit(
            request.clone(),
            io::Cursor::new(bytes.clone()),
            bytes.len() as u64,
        )
        .unwrap();
    drop(submission);
    drop(coord); // The actual daemon, not a client task, retains this owner.
    let coord = runtime.load_coordinator().unwrap();
    idle(&coord).await;
    let receipt = coord.lookup(&request).unwrap().unwrap();
    let LoadState::Finished(LoadOutcome::Accepted { descriptor }) = receipt.state() else {
        panic!("{:?}", receipt.state());
    };
    let run = coord.run(descriptor.identity()).unwrap();
    assert_eq!(run.view().unwrap().boundary(), 0);
    assert_eq!(run.step(control()).await.unwrap().boundary(), 1);
    drop(run);
    assert_eq!(
        coord
            .run(descriptor.identity())
            .unwrap()
            .view()
            .unwrap()
            .boundary(),
        1
    );
    assert_eq!(
        coord
            .submit(request.clone(), NeverRead, u64::MAX)
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        receipt
    );
    let conflict = LoadRequest::new(
        request.operation_id().clone(),
        formation.clone(),
        dummy_root(),
    );
    assert!(matches!(
        coord.submit(conflict, NeverRead, 1),
        Err(LoadError::Receipt(ReceiptStoreError::Conflict))
    ));
    assert!(matches!(
        coord.submit(
            LoadRequest::new("new".parse().unwrap(), formation.clone(), root),
            NeverRead,
            1
        ),
        Err(LoadError::Busy)
    ));
    let wrong_epoch = RunIdentity::new(formation, root, WorkloadEpoch::new(2));
    assert!(matches!(
        coord.run(&wrong_epoch),
        Err(LoadError::RunUnavailable)
    ));
    coord.run(descriptor.identity()).unwrap().unload();
    released(&service).await;
    let second = load_request(&service, "new-run", root);
    let next = coord
        .submit(second, io::Cursor::new(bytes.clone()), bytes.len() as u64)
        .unwrap()
        .outcome()
        .await
        .unwrap();
    let LoadState::Finished(LoadOutcome::Accepted {
        descriptor: second_descriptor,
    }) = next.state()
    else {
        panic!("second admission");
    };
    assert_eq!(second_descriptor.identity().workload_epoch().get(), 2);
    assert!(matches!(
        coord.run(descriptor.identity()),
        Err(LoadError::RunUnavailable)
    ));
    let loan = coord.run(second_descriptor.identity()).unwrap();
    assert_eq!(
        coord.retained_descriptor().unwrap().as_ref(),
        Some(second_descriptor)
    );
    runtime.shutdown().await.unwrap();
    assert!(coord.retained_descriptor().unwrap().is_none());
    assert!(loan.view().is_err(), "daemon shutdown revokes all loans");
    assert_eq!(
        coord
            .submit(request.clone(), NeverRead, 1)
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        receipt,
        "history remains readable after shutdown"
    );
    owner.await.unwrap().unwrap();
    drop(loan);
    drop(coord);
    drop(service);
    drop(runtime);
    let reopened = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(reopened.lookup(&request).unwrap(), Some(&receipt));
}

#[tokio::test]
async fn pending_replay_does_not_read_another_body_and_shutdown_releases_stalled_input() {
    let (service, handle, owner) = worker().await;
    let dir = directory();
    let coord = coordinator(&service, &dir);
    let request = load_request(&service, "pending", dummy_root());
    let (_sender, reader) = tokio::io::duplex(1);
    drop(coord.submit(request.clone(), reader, 1).unwrap());
    pending(&coord, &request).await;
    let receipt = coord
        .submit(request.clone(), NeverRead, 1)
        .unwrap()
        .outcome()
        .await
        .unwrap();
    assert_eq!(receipt.state(), &LoadState::Pending);
    assert!(matches!(
        coord.submit(load_request(&service, "other", dummy_root()), NeverRead, 1),
        Err(LoadError::Busy)
    ));
    coord.shutdown();
    idle(&coord).await;
    assert!(matches!(
        coord.lookup(&request).unwrap().unwrap().state(),
        LoadState::Finished(LoadOutcome::Refused { .. })
    ));
    released(&service).await;
    assert!(matches!(
        coord.submit(load_request(&service, "new", dummy_root()), NeverRead, 1),
        Err(LoadError::Closed)
    ));
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn stale_formation_bad_input_and_invalid_length_are_durable_refusals_not_runs() {
    let (service, handle, owner) = worker().await;
    let dir = directory();
    let coord = coordinator(&service, &dir);
    let stale = LoadRequest::new(
        "stale".parse().unwrap(),
        "old-formation".parse().unwrap(),
        dummy_root(),
    );
    let refused = coord
        .submit(stale.clone(), NeverRead, 1)
        .unwrap()
        .outcome()
        .await
        .unwrap();
    assert_eq!(
        refused.state(),
        &LoadState::Finished(LoadOutcome::Refused {
            reason: LoadRefusal::FormationUnavailable
        })
    );
    let invalid_length = load_request(&service, "length", dummy_root());
    assert!(matches!(
        coord
            .submit(invalid_length, NeverRead, u64::MAX)
            .unwrap()
            .outcome()
            .await
            .unwrap()
            .state(),
        LoadState::Finished(LoadOutcome::Refused { .. })
    ));
    released(&service).await;
    let malformed = load_request(&service, "malformed", dummy_root());
    let receipt = coord
        .submit(malformed, io::Cursor::new(vec![0]), 1)
        .unwrap()
        .outcome()
        .await
        .unwrap();
    assert_eq!(
        receipt.state(),
        &LoadState::Finished(LoadOutcome::Refused {
            reason: LoadRefusal::InvalidWorkload
        })
    );
    assert!(handle.view().unwrap().scientific.is_none());
    coord.shutdown();
    idle(&coord).await;
    assert_eq!(
        coord
            .submit(stale, NeverRead, 1)
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        refused
    );
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn unexpectedly_panicking_input_fails_closed_and_recovers_indeterminate() {
    let (service, handle, owner) = worker().await;
    let dir = directory();
    let coord = coordinator(&service, &dir);
    let request = load_request(&service, "panic", dummy_root());
    assert!(matches!(
        coord
            .submit(request.clone(), NeverRead, 1)
            .unwrap()
            .outcome()
            .await,
        Err(LoadError::OutcomeUnknown)
    ));
    idle(&coord).await;
    assert!(matches!(
        coord.lookup(&request),
        Err(LoadError::Receipt(ReceiptStoreError::Poisoned))
    ));
    released(&service).await;
    drop(coord);
    let reopened = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened.lookup(&request).unwrap().unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn final_receipt_io_failure_preserves_run_and_never_reports_false_refusal() {
    let _slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let (service, handle, owner) = worker().await;
    let dir = directory();
    let coord = coordinator(&service, &dir);
    let request = load_request(&service, "disk-failure", root);
    let (mut sender, reader) = tokio::io::duplex(8192);
    let submitted = coord
        .submit(request.clone(), reader, bytes.len() as u64)
        .unwrap();
    pending(&coord, &request).await;
    // Exact private test staging collision injects a failure in final publication
    // while allowing real closure verification/JIT/initial commit to succeed.
    let directory = crate::credentials::open_private_directory(dir.path()).unwrap();
    let _collision =
        crate::credentials::create_private(&directory, ".workload-load-receipts.pending").unwrap();
    sender.write_all(&bytes).await.unwrap();
    sender.shutdown().await.unwrap();
    assert!(matches!(
        submitted.outcome().await,
        Err(LoadError::Receipt(_))
    ));
    let descriptor = coord
        .retained_descriptor()
        .unwrap()
        .expect("discover the live run without guessing its epoch after lost receipt");
    assert_eq!(descriptor.identity().formation_id(), request.formation_id());
    assert_eq!(descriptor.identity().workload_id(), root);
    assert_eq!(descriptor.identity().workload_epoch().get(), 1);
    let run = coord.run(descriptor.identity()).unwrap();
    assert_eq!(run.view().unwrap().boundary(), 0);
    assert_eq!(run.step(control()).await.unwrap().boundary(), 1);
    assert!(matches!(
        coord.lookup(&request),
        Err(LoadError::Receipt(ReceiptStoreError::Poisoned))
    ));
    coord.shutdown();
    idle(&coord).await;
    drop(run);
    drop(coord);
    drop(_collision);
    let reopened = ReceiptStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened.lookup(&request).unwrap().unwrap().state(),
        &LoadState::Finished(LoadOutcome::Indeterminate)
    );
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn daemon_owner_drop_cancels_pending_load_even_with_a_client_handle_alive() {
    use crate::{credentials::WorkerCredentials, runtime::WorkerRuntime};
    use orishu::model::cluster::LockRequest;
    let dir = directory();
    let (runtime, owner) = WorkerRuntime::standalone(
        WorkerCredentials::load_or_create(dir.path()).unwrap(),
        "worker".parse().unwrap(),
        "cluster".parse().unwrap(),
        vec![],
    )
    .unwrap()
    .start();
    let formation = runtime.summary().unwrap().formation_id;
    runtime
        .lock_operation(LockRequest {
            schema_version: 1,
            operation_id: "lock".parse().unwrap(),
            formation_id: formation.clone(),
            locked: true,
        })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let sandbox = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let service = runtime
        .scientific_admission(
            sandbox.clone(),
            Default::default(),
            Default::default(),
            BTreeSet::new(),
        )
        .unwrap();
    let client = runtime
        .install_load_coordinator(
            ReceiptStore::open(dir.path()).unwrap(),
            sandbox.clone(),
            LoadPolicy::default(),
        )
        .unwrap();
    let other_dir = directory();
    assert!(matches!(
        runtime.install_load_coordinator(
            ReceiptStore::open(other_dir.path()).unwrap(),
            sandbox,
            LoadPolicy::default()
        ),
        Err(LoadError::Busy)
    ));
    let request = LoadRequest::new("drop-owner".parse().unwrap(), formation, dummy_root());
    let (_writer, reader) = tokio::io::duplex(1);
    drop(client.submit(request.clone(), reader, 1).unwrap());
    pending(&client, &request).await;
    drop(runtime);
    idle(&client).await;
    assert!(matches!(
        client.lookup(&request).unwrap().unwrap().state(),
        LoadState::Finished(LoadOutcome::Refused { .. })
    ));
    assert!(matches!(
        client.submit(load_request(&service, "new", dummy_root()), NeverRead, 1),
        Err(LoadError::Closed)
    ));
    released(&service).await;
    service.owner.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn journal_reservation_failure_and_invalid_host_policy_never_read_input() {
    let (service, handle, owner) = worker().await;
    let dir = directory();
    assert!(matches!(
        LoadCoordinator::new(
            service.clone(),
            ReceiptStore::open(dir.path()).unwrap(),
            Default::default(),
            Duration::ZERO
        ),
        Err(LoadError::Policy)
    ));
    let coord = coordinator(&service, &dir);
    let directory = crate::credentials::open_private_directory(dir.path()).unwrap();
    let _collision =
        crate::credentials::create_private(&directory, ".workload-load-receipts.pending").unwrap();
    let request = load_request(&service, "cannot-reserve", dummy_root());
    assert!(matches!(
        coord
            .submit(request.clone(), NeverRead, 1)
            .unwrap()
            .outcome()
            .await,
        Err(LoadError::Receipt(_))
    ));
    assert!(matches!(
        coord.lookup(&request),
        Err(LoadError::Receipt(ReceiptStoreError::Poisoned))
    ));
    assert!(handle.view().unwrap().scientific.is_none());
    drop(coord);
    drop(_collision);
    let reopened = ReceiptStore::open(dir.path()).unwrap();
    assert!(
        reopened.lookup(&request).unwrap().is_none(),
        "failed reservation never authorized work"
    );
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}
