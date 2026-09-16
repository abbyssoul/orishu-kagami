use super::*;
use orishu_membership::testing;
use orishu_runtime::SandboxLimits;
use std::sync::OnceLock;

mod delivery_tests;
#[cfg(unix)]
mod load_tests;

async fn scientific_test_slot() -> tokio::sync::SemaphorePermit<'static> {
    // Each case independently JITs real Components. Bound simultaneous fresh
    // engines within this binary so the ordinary parallel formation suite does
    // not turn CPU/memory contention into an admission-deadline assertion.
    // Production operation/guest/coordination limits are unchanged.
    static HEAVY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    HEAVY.acquire().await.unwrap()
}

#[path = "../../../../../crates/orishu-runtime/tests/reference_support/admission.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../../../../../crates/orishu-runtime/tests/reference_support/mod.rs"]
mod reference_support;

fn portable() -> (Arc<[u8]>, orishu_workload::WorkloadDigest) {
    static INPUT: OnceLock<(Arc<[u8]>, orishu_workload::WorkloadDigest)> = OnceLock::new();
    INPUT
        .get_or_init(|| {
            let fixture = fixture::Fixture::new();
            let compiled = fixture.compile();
            let blobs = fixture.export(&compiled);
            let borrowed = fixture::borrowed(&blobs);
            let bytes = bundle::pack(
                compiled.manifest_bytes(),
                &borrowed,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            (bytes.into(), compiled.verified().root())
        })
        .clone()
}
fn control() -> OperationControl {
    OperationControl::new(Duration::from_secs(60)).unwrap()
}
async fn worker() -> (
    AdmissionService,
    Handle,
    tokio::task::JoinHandle<Result<(), DriverError>>,
) {
    let sandbox = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    let (handle, task) = spawn_standalone(testing::model_with_members(0));
    let view = handle.view().unwrap();
    handle
        .set_membership_lock(view.generation, view.summary.formation_id, true)
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let service = AdmissionService::new(
        handle.clone(),
        sandbox,
        Default::default(),
        Default::default(),
        BTreeSet::new(),
    )
    .unwrap();
    (service, handle, task)
}
async fn prepared(service: &AdmissionService, control: OperationControl) -> PreparedAdmission {
    let view = service.owner.view().unwrap();
    service
        .prepare(view.generation, view.summary.formation_id, control)
        .await
        .unwrap()
}
fn hold(
    job: &mut PreparedAdmission,
) -> (
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::SyncSender<()>,
) {
    let (entered, observed) = std::sync::mpsc::sync_channel(1);
    let (release, held) = std::sync::mpsc::sync_channel(1);
    job.before_admission = Some((entered, held));
    (observed, release)
}
async fn entered(observed: std::sync::mpsc::Receiver<()>) {
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || observed.recv().unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn actual_portable_closure_is_confirmed_by_worker_and_handed_to_shared_runtime() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let (service, handle, task) = worker().await;
    let mut job = prepared(&service, control()).await;
    assert!(bytes.len() <= job.input_limit());
    let fence = job.fence();
    let (observed, release) = hold(&mut job);
    let pending = tokio::spawn(job.validate(bytes));
    entered(observed).await;
    // The only async runtime thread remains responsive while scientific work is
    // parked on its blocking executor. No wall-time speed assumption is needed.
    let view = handle.view().unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        handle
            .set_membership_lock(view.generation, view.summary.formation_id.clone(), true)
            .unwrap(),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert!(matches!(
        service
            .prepare(view.generation, view.summary.formation_id, control())
            .await,
        Err(AdmissionError::Reservation(ExecutionError::Busy))
    ));
    release.send(()).unwrap();
    let candidate = pending.await.unwrap().unwrap();
    assert_eq!(candidate.scope().workload, root);
    assert_eq!(candidate.scope().epoch, 1);
    assert_eq!(
        candidate.scope().run,
        candidate.descriptor().digest().unwrap()
    );
    assert_eq!(
        candidate.descriptor().identity().formation_id(),
        fence.formation()
    );
    let admitted = candidate.confirm().await.unwrap();
    assert!(admitted.fence().is_current());
    assert_eq!(admitted.admitted().workload.root(), root);
    assert_eq!(admitted.admitted().run.state().boundary(), 0);
    let (mut admitted, lease) = admitted.into_parts();
    // This is explicit lower-level handoff evidence, not a worker run endpoint or
    // externally published step. The next execution adapter must own that commit.
    admitted
        .run
        .advance(control().with_parent(fence.cancellation()).unwrap())
        .unwrap();
    assert_eq!(admitted.run.state().boundary(), 1);
    assert!(fence.is_current());
    drop(admitted);
    drop(lease);
    handle.model_snapshot().await;
    drop(prepared(&service, control()).await);
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn malformed_oversized_denied_inputs_release_the_slot() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, _root) = portable();
    let (service, handle, task) = worker().await;
    assert!(matches!(
        prepared(&service, control())
            .await
            .validate(Arc::from(&b"bad ZIP"[..]))
            .await,
        Err(AdmissionError::Closure(_))
    ));
    let mut limited = service.clone();
    limited.archive.max_bytes = bytes.len() - 1;
    assert!(matches!(
        prepared(&limited, control())
            .await
            .validate(bytes.clone())
            .await,
        Err(AdmissionError::Limit)
    ));
    let parsed = bundle::read(&bytes, Default::default(), Default::default()).unwrap();
    let mut denied = service.clone();
    let digest = *parsed.blobs().keys().next().unwrap();
    denied.denied = Arc::new(BTreeSet::from([digest]));
    assert!(
        matches!(prepared(&denied, control()).await.validate(bytes.clone()).await,
        Err(AdmissionError::Denied(d)) if d == digest)
    );
    let valid = prepared(&service, control())
        .await
        .validate(bytes)
        .await
        .unwrap();
    assert_eq!(
        valid.scope().epoch,
        1,
        "pre-allocation refusals must not issue epochs"
    );
    drop(valid);
    drop(prepared(&service, control()).await);
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn caller_abort_cancels_but_retains_capacity_until_blocking_work_really_exits() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, _root) = portable();
    let (service, handle, task) = worker().await;
    let cancellation = control();
    let mut job = prepared(&service, cancellation.clone()).await;
    let fence = job.fence();
    let (observed, release) = hold(&mut job);
    let pending = tokio::spawn(job.validate(bytes));
    entered(observed).await;
    pending.abort();
    assert!(matches!(pending.await, Err(e) if e.is_cancelled()));
    assert!(cancellation.check().is_err());
    assert!(
        fence.is_current(),
        "parent lease stays live until real disposal"
    );
    let view = handle.view().unwrap();
    assert!(matches!(
        service
            .prepare(view.generation, view.summary.formation_id, control())
            .await,
        Err(AdmissionError::Reservation(ExecutionError::Busy))
    ));
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while fence.is_current() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    handle.model_snapshot().await;
    drop(prepared(&service, control()).await);
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn shutdown_revokes_a_queued_job_and_a_completed_unconfirmed_candidate() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, _root) = portable();
    let (service, handle, task) = worker().await;
    let mut job = prepared(&service, control()).await;
    let (observed, release) = hold(&mut job);
    let pending = tokio::spawn(job.validate(bytes.clone()));
    entered(observed).await;
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(AdmissionError::Interrupted(Interruption::Cancelled))
    ));

    let (service, handle, task) = worker().await;
    let candidate = prepared(&service, control())
        .await
        .validate(bytes)
        .await
        .unwrap();
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
    assert!(matches!(
        candidate.confirm().await,
        Err(AdmissionError::Interrupted(Interruption::Cancelled))
    ));
}

#[tokio::test]
async fn confirmation_uses_reserved_capacity_and_rechecks_owner_cancellation() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, _root) = portable();
    let (service, handle, task) = worker().await;
    let candidate = prepared(&service, control())
        .await
        .validate(bytes.clone())
        .await
        .unwrap();
    let mut held = Vec::new();
    while let Ok(permit) = handle.control.clone().try_reserve_owned() {
        held.push(permit);
    }
    assert_eq!(held.len(), CONTROL_CAPACITY - 2);
    let admitted = tokio::time::timeout(Duration::from_secs(2), candidate.confirm())
        .await
        .unwrap()
        .unwrap();
    drop(held);
    drop(admitted);
    handle.model_snapshot().await;
    let candidate = prepared(&service, control())
        .await
        .validate(bytes)
        .await
        .unwrap();
    let fence = candidate.prepared.lease.fence();
    // Ordered immediately ahead of confirmation: the candidate's early check may
    // pass, but the owner must recheck after applying this topology change.
    handle
        .try_submit(
            fence.generation(),
            Message::Local(Command::SetMembershipLock(false)),
        )
        .unwrap();
    assert!(candidate.confirm().await.is_err());
    assert!(!fence.is_current());
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

async fn admitted(service: &AdmissionService, _handle: &Handle) -> ValidatedAdmission {
    let (bytes, _root) = portable();
    prepared(service, control())
        .await
        .validate(bytes)
        .await
        .unwrap()
        .confirm()
        .await
        .unwrap()
}

fn sampled_x(lease: &orishu_runtime::FieldSnapshot) -> f64 {
    use orishu_plugin::execution::*;
    let metadata = lease.request(1, vec![lease.context().observables[0].channel.clone()]);
    let mut bytes = vec![];
    encode_sample_request(
        &metadata,
        &[SamplePoint {
            id: 1,
            position_metres: [fixture::n(3.0), fixture::n(0.0), fixture::n(0.0)],
        }],
        &mut bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let request = SampleRequest::read(
        &bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let result = lease
        .sample(
            orishu_runtime::Buffer {
                schema: SAMPLE_REQUEST_SCHEMA.into(),
                value_count: 1,
                bytes: bytes.clone().into(),
            },
            control(),
        )
        .unwrap();
    let response = SampleResponse::read(
        &result.bytes,
        &request,
        lease.context(),
        SampleLimits::default(),
    )
    .unwrap();
    match response.cell(0, 0).unwrap() {
        SampleCell::Valid { values, .. } => values.iter().next().unwrap().get(),
        _ => panic!("finite reference sample"),
    }
}

async fn released(service: &AdmissionService) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let view = service.owner.view().unwrap();
            match service
                .prepare(view.generation, view.summary.formation_id, control())
                .await
            {
                Ok(job) => {
                    drop(job);
                    return;
                }
                Err(AdmissionError::Reservation(ExecutionError::Busy)) => {
                    tokio::task::yield_now().await
                }
                other => panic!("unexpected release result: {:?}", other.err()),
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn retained_run_publishes_steps_stop_and_bounded_independent_observations() {
    let _fixture_slot = scientific_test_slot().await;
    use super::super::run::RunError;
    use orishu_plugin::execution::SnapshotSource;
    portable();
    let (mut service, handle, task) = worker().await;
    service.limits.run.snapshots = 1;
    let run = admitted(&service, &handle)
        .await
        .start(control())
        .await
        .unwrap();
    assert_eq!(run.view().unwrap().boundary(), 0);
    assert_eq!(
        handle.view().unwrap().scientific.unwrap(),
        run.view().unwrap()
    );
    let old = run
        .acquire_field("newtonian".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(sampled_x(&old), 0.0);
    let cancelled = control();
    cancelled.cancel();
    assert!(run.step(cancelled).await.is_err());
    assert_eq!(run.view().unwrap().boundary(), 0);
    let first = run.step(control()).await.unwrap();
    let descriptor = run.descriptor().clone();
    assert_eq!(first.scope().run, descriptor.digest().unwrap());
    assert_eq!(
        descriptor.identity().workload_epoch().get(),
        first.scope().epoch
    );
    assert_eq!(first.boundary(), 1);
    assert_eq!(first.time_seconds(), fixture::n(0.5));
    // Observer quota refusal does not prevent the next scientific step.
    assert!(
        run.acquire_field("newtonian".parse().unwrap())
            .await
            .is_err()
    );
    assert_eq!(run.step(control()).await.unwrap().boundary(), 2);
    assert_eq!(sampled_x(&old), 0.0);
    drop(old);
    let current = run
        .acquire_field("newtonian".parse().unwrap())
        .await
        .unwrap();
    assert!(matches!(
        current.snapshot().source,
        SnapshotSource::Committed { boundary: 2, .. }
    ));
    assert!(sampled_x(&current) < 0.0);
    let stopped = run.stop(control()).await.unwrap();
    assert!(stopped.stopped());
    assert_eq!(run.stop(control()).await.unwrap(), stopped);
    assert!(matches!(
        run.step(control()).await,
        Err(RunError::Scientific(AdmissionError::Run(
            orishu_runtime::RunRejection::Stopped
        )))
    ));
    assert_eq!(run.view().unwrap(), stopped);
    run.unload();
    assert!(matches!(run.view(), Err(RunError::Closed)));
    // An immutable acquired observation remains valid after executor disposal.
    released(&service).await;
    assert!(sampled_x(&current) < 0.0);
    handle.model_snapshot().await;
    assert!(handle.view().unwrap().scientific.is_none());
    let repeated = admitted(&service, &handle)
        .await
        .start(control())
        .await
        .unwrap();
    assert_eq!(
        repeated.descriptor().identity().workload_id(),
        descriptor.identity().workload_id()
    );
    assert_eq!(repeated.descriptor().identity().workload_epoch().get(), 2);
    assert_ne!(
        repeated.descriptor().digest().unwrap(),
        descriptor.digest().unwrap()
    );
    assert_eq!(repeated.view().unwrap().boundary(), 0);
    assert!(
        matches!(current.snapshot().source, SnapshotSource::Committed { run: id, epoch: 1, boundary: 2, .. } if id == descriptor.digest().unwrap())
    );
    repeated.unload();
    released(&service).await;
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn retained_run_has_no_step_backlog_and_publishes_with_reserved_capacity_after_caller_loss() {
    let _fixture_slot = scientific_test_slot().await;
    use super::super::run::RunError;
    portable();
    let (service, handle, task) = worker().await;
    let mut admission = admitted(&service, &handle).await;
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    admission.before_step_publish = Some((entered_tx, release_rx));
    let run = admission.start(control()).await.unwrap();
    let caller = run.clone();
    let pending = tokio::spawn(async move { caller.step(control()).await });
    entered(entered_rx).await;
    assert!(matches!(run.step(control()).await, Err(RunError::Busy)));
    assert!(matches!(run.stop(control()).await, Err(RunError::Busy)));
    assert!(matches!(
        run.acquire_field("newtonian".parse().unwrap()).await,
        Err(RunError::Busy)
    ));
    assert_eq!(
        run.view().unwrap().boundary(),
        0,
        "candidate remains unpublished"
    );
    let view = handle.view().unwrap();
    handle
        .set_membership_lock(view.generation, view.summary.formation_id, true)
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let mut pressure = vec![];
    while let Ok(permit) = handle.control.clone().try_reserve_owned() {
        pressure.push(permit);
    }
    assert_eq!(pressure.len(), CONTROL_CAPACITY - 2);
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while run.view().unwrap().boundary() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(pressure);
    assert_eq!(run.view().unwrap().time_seconds(), fixture::n(0.5));
    run.unload();
    released(&service).await;
    handle.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn retained_run_refuses_complete_late_candidate_after_shutdown_or_owner_loss() {
    let _fixture_slot = scientific_test_slot().await;
    use super::super::run::RunError;
    portable();
    for abort in [false, true] {
        let (service, handle, task) = worker().await;
        let mut admission = admitted(&service, &handle).await;
        let fence = admission.fence();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        admission.before_step_publish = Some((entered_tx, release_rx));
        let run = admission.start(control()).await.unwrap();
        let caller = run.clone();
        let pending = tokio::spawn(async move { caller.step(control()).await });
        entered(entered_rx).await;
        assert_eq!(run.view().unwrap().boundary(), 0);
        if abort {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            handle.shutdown().await.unwrap();
            task.await.unwrap().unwrap();
        }
        assert!(!fence.is_current());
        release_tx.send(()).unwrap();
        assert!(matches!(pending.await.unwrap(), Err(RunError::Publication)));
        assert!(matches!(run.view(), Err(RunError::Closed)));
    }
}

#[tokio::test]
#[cfg(unix)]
async fn real_worker_bootstrap_and_restart_allocate_distinct_run_descriptors() {
    let _fixture_slot = scientific_test_slot().await;
    let (bytes, root) = portable();
    let directory = tempfile::tempdir().unwrap();
    let mut previous: Option<orishu::model::run::RunDescriptor> = None;
    for _ in 0..2 {
        let credentials =
            crate::credentials::WorkerCredentials::load_or_create(&directory.path().join("worker"))
                .unwrap();
        let bootstrap = crate::runtime::WorkerRuntime::standalone(
            credentials,
            "same-worker-label".parse().unwrap(),
            "same-cluster-label".parse().unwrap(),
            vec![],
        )
        .unwrap();
        let (worker, task) = bootstrap.start();
        let formation = worker.summary().unwrap().formation_id;
        let service = worker
            .scientific_admission(
                Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
                Default::default(),
                Default::default(),
                BTreeSet::new(),
            )
            .unwrap();
        assert!(matches!(
            service.prepare_for(formation.clone(), control()).await,
            Err(AdmissionError::Reservation(ExecutionError::Unavailable))
        ));
        worker
            .lock_operation(orishu::model::cluster::LockRequest {
                schema_version: 1,
                operation_id: "science-lock".parse().unwrap(),
                formation_id: formation.clone(),
                locked: true,
            })
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        if let Some(old) = &previous {
            assert_ne!(old.identity().formation_id(), &formation);
            assert!(matches!(
                service
                    .prepare_for(old.identity().formation_id().clone(), control())
                    .await,
                Err(AdmissionError::Reservation(ExecutionError::Stale))
            ));
        }
        let candidate = service
            .prepare_for(formation.clone(), control())
            .await
            .unwrap()
            .validate(bytes.clone())
            .await
            .unwrap();
        let descriptor = candidate.descriptor().clone();
        assert_eq!(descriptor.identity().formation_id(), &formation);
        assert_eq!(descriptor.identity().workload_id(), root);
        assert_eq!(descriptor.identity().workload_epoch().get(), 1);
        if let Some(old) = &previous {
            assert_ne!(old.digest().unwrap(), descriptor.digest().unwrap());
        }
        let run = candidate
            .confirm()
            .await
            .unwrap()
            .start(control())
            .await
            .unwrap();
        assert_eq!(
            run.step(control()).await.unwrap().scope().run,
            descriptor.digest().unwrap()
        );
        assert_eq!(run.descriptor(), &descriptor);
        run.unload();
        released(&service).await;
        worker.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
        previous = Some(descriptor);
        drop(worker);
    }
}
