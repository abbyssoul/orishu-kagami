use super::*;
use crate::{
    run_commands::{CommandError, RunCommandCoordinator},
    workload_load::{LoadCoordinator, LoadPolicy},
    workload_receipts::{CommandReceiptStore, ReceiptStore, ReceiptStoreError},
};
use orishu::model::{
    run::{RunIdentity, WorkloadEpoch},
    run_command::*,
    run_load::*,
};
use std::{io, os::unix::fs::PermissionsExt};

fn directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn request(run: &RunIdentity, id: &str, boundary: u64, command: RunCommand) -> RunCommandRequest {
    RunCommandRequest::new(id.parse().unwrap(), run.clone(), boundary, command).unwrap()
}
async fn wait(commands: &RunCommandCoordinator) {
    tokio::time::timeout(Duration::from_secs(5), commands.wait_idle())
        .await
        .unwrap();
}
async fn crossed(entered: tokio::sync::oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(5), entered)
        .await
        .unwrap()
        .unwrap();
}
fn applied(receipt: &RunCommandReceipt) -> &RunStatus {
    let RunCommandState::Finished(RunCommandOutcome::Applied { status }) = receipt.state() else {
        panic!("expected applied, got {:?}", receipt.state());
    };
    status
}
async fn load(
    loads: &LoadCoordinator,
    formation: orishu::model::cluster::FormationId,
    id: &str,
) -> RunIdentity {
    let (bytes, root) = portable();
    let receipt = loads
        .submit(
            LoadRequest::new(id.parse().unwrap(), formation, root),
            io::Cursor::new(bytes.clone()),
            bytes.len() as u64,
        )
        .unwrap()
        .outcome()
        .await
        .unwrap();
    let LoadState::Finished(LoadOutcome::Accepted { descriptor }) = receipt.state() else {
        panic!("load refused: {:?}", receipt.state());
    };
    descriptor.identity().clone()
}

#[tokio::test]
async fn daemon_commands_survive_response_loss_preserve_identity_and_finish_without_resume() {
    use crate::{credentials::WorkerCredentials, runtime::WorkerRuntime};
    use orishu::model::cluster::LockRequest;
    let _slot = scientific_test_slot().await;
    portable();
    let dir = directory();
    let (runtime, owner) = WorkerRuntime::standalone(
        WorkerCredentials::load_or_create(dir.path()).unwrap(),
        "worker".parse().unwrap(),
        "cluster".parse().unwrap(),
        vec![],
    )
    .unwrap()
    .start();
    assert!(runtime.run_command_coordinator().is_none());
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
    let loads = runtime
        .install_load_coordinator(
            ReceiptStore::open(dir.path()).unwrap(),
            sandbox,
            LoadPolicy::default(),
        )
        .unwrap();
    let identity = load(&loads, formation.clone(), "load-one").await;
    let commands = runtime
        .install_run_command_coordinator(
            CommandReceiptStore::open(dir.path()).unwrap(),
            Duration::from_secs(10),
        )
        .unwrap();
    let initial = commands.status(&identity).unwrap();
    assert_eq!(initial.boundary(), 0);
    assert_eq!(initial.time_seconds(), 0.0);
    assert_eq!(initial.phase(), RunPhase::Ready);
    let first = request(&identity, "lost-response", 0, RunCommand::Step);
    let (entered, release) = commands.hold(false);
    drop(commands.submit(first.clone()).unwrap());
    crossed(entered).await;
    assert_eq!(
        commands.lookup(&first).unwrap().unwrap().state(),
        &RunCommandState::Pending
    );
    assert!(matches!(
        commands.submit(request(&identity, "busy", 0, RunCommand::Step)),
        Err(CommandError::Busy)
    ));
    assert_eq!(
        commands
            .submit(first.clone())
            .unwrap()
            .outcome()
            .await
            .unwrap()
            .state(),
        &RunCommandState::Pending
    );
    drop(commands); // No request handle/response retains daemon ownership.
    release.send(()).unwrap();
    let commands = runtime.run_command_coordinator().unwrap();
    wait(&commands).await;
    let receipt = commands.lookup(&first).unwrap().unwrap();
    assert_eq!(applied(&receipt).boundary(), 1);
    assert_eq!(applied(&receipt).time_seconds(), 0.5);
    assert_eq!(
        commands
            .submit(first.clone())
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(commands.status(&identity).unwrap().boundary(), 1);
    assert!(matches!(
        commands.submit(request(&identity, "lost-response", 1, RunCommand::Step)),
        Err(CommandError::Receipt(ReceiptStoreError::Conflict))
    ));

    // Another internal caller advances after intent is durable but before this
    // command reaches the executor. Only executor-side guards prevent a second step.
    let race = request(&identity, "stale-race", 1, RunCommand::Step);
    let (entered, release) = commands.hold(false);
    let submission = commands.submit(race.clone()).unwrap();
    crossed(entered).await;
    loads
        .run(&identity)
        .unwrap()
        .step_at(1, control())
        .await
        .unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        submission.outcome().await.unwrap().state(),
        RunCommandState::Finished(RunCommandOutcome::Refused {
            reason: RunCommandRefusal::StaleBoundary { actual: 2 }
        })
    ));
    assert_eq!(commands.status(&identity).unwrap().boundary(), 2);
    let finish = request(&identity, "finish", 2, RunCommand::Finish);
    let finished = commands
        .submit(finish.clone())
        .unwrap()
        .outcome()
        .await
        .unwrap();
    assert_eq!(applied(&finished).phase(), RunPhase::Finished);
    assert_eq!(applied(&finished).boundary(), 2);
    assert_eq!(
        commands.submit(finish).unwrap().outcome().await.unwrap(),
        finished
    );
    assert!(matches!(
        commands
            .submit(request(&identity, "after-finish", 2, RunCommand::Step))
            .unwrap()
            .outcome()
            .await
            .unwrap()
            .state(),
        RunCommandState::Finished(RunCommandOutcome::Refused {
            reason: RunCommandRefusal::Scientific
        })
    ));
    loads.run(&identity).unwrap().unload();
    released(&service).await;
    let second = load(&loads, formation, "load-two").await;
    assert_ne!(second, identity);
    assert!(matches!(
        commands.status(&identity),
        Err(CommandError::RunUnavailable)
    ));
    assert_eq!(
        commands
            .submit(first.clone())
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        receipt
    );
    assert!(matches!(
        commands
            .submit(request(&identity, "old-run", 2, RunCommand::Step))
            .unwrap()
            .outcome()
            .await
            .unwrap()
            .state(),
        RunCommandState::Finished(RunCommandOutcome::Refused {
            reason: RunCommandRefusal::RunUnavailable
        })
    ));
    assert_eq!(commands.status(&second).unwrap().boundary(), 0);
    runtime.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
    assert!(commands.status(&second).is_err());
    assert_eq!(
        commands
            .submit(first.clone())
            .unwrap()
            .outcome()
            .await
            .unwrap(),
        receipt
    );
    assert!(matches!(
        commands.submit(request(&second, "shutdown", 0, RunCommand::Step)),
        Err(CommandError::Closed)
    ));
    drop(commands);
    drop(runtime);
    drop(loads);
    drop(service);
    let store = CommandReceiptStore::open(dir.path()).unwrap();
    assert_eq!(store.lookup(&first).unwrap(), Some(&receipt));
}

#[tokio::test]
async fn command_io_failure_panics_and_shutdown_never_undo_or_repeat_committed_steps() {
    let _slot = scientific_test_slot().await;
    portable();
    let dir = directory();
    let (service, handle, owner) = worker().await;
    let loads = LoadCoordinator::new(
        service.clone(),
        ReceiptStore::open(dir.path()).unwrap(),
        Default::default(),
        Duration::from_secs(60),
    )
    .unwrap();
    let identity = load(&loads, handle.view().unwrap().summary.formation_id, "load").await;
    let reopen = || {
        RunCommandCoordinator::new(
            loads.clone(),
            CommandReceiptStore::open(dir.path()).unwrap(),
            Duration::from_secs(10),
        )
        .unwrap()
    };
    let commands = reopen();
    let request = request(&identity, "final-io", 0, RunCommand::Step);
    let (entered, release) = commands.hold(true);
    let submission = commands.submit(request.clone()).unwrap();
    crossed(entered).await;
    assert_eq!(commands.status(&identity).unwrap().boundary(), 1);
    let directory = crate::credentials::open_private_directory(dir.path()).unwrap();
    let collision =
        crate::credentials::create_private(&directory, ".run-command-receipts.pending").unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        submission.outcome().await,
        Err(CommandError::Receipt(_))
    ));
    assert!(matches!(
        commands.lookup(&request),
        Err(CommandError::Receipt(ReceiptStoreError::Poisoned))
    ));
    assert_eq!(commands.status(&identity).unwrap().boundary(), 1);
    assert!(commands.submit(request.clone()).is_err());
    wait(&commands).await;
    drop(commands);
    drop(collision);
    let commands = reopen();
    assert_eq!(
        commands
            .submit(request.clone())
            .unwrap()
            .outcome()
            .await
            .unwrap()
            .state(),
        &RunCommandState::Finished(RunCommandOutcome::Indeterminate)
    );
    assert_eq!(commands.status(&identity).unwrap().boundary(), 1);
    drop(commands);

    // Losing the detached job before or after execution poisons its live history.
    // Reopen is conservative; response loss must never manufacture a refusal.
    let mut boundary = 1;
    for after_execution in [false, true] {
        let commands = reopen();
        let intent = self::request(
            &identity,
            if after_execution {
                "panic-after"
            } else {
                "panic-before"
            },
            boundary,
            RunCommand::Step,
        );
        let (entered, release) = commands.hold(after_execution);
        let submission = commands.submit(intent.clone()).unwrap();
        crossed(entered).await;
        drop(release); // The test-only barrier panics the detached job.
        assert!(matches!(
            submission.outcome().await,
            Err(CommandError::OutcomeUnknown)
        ));
        wait(&commands).await;
        assert!(matches!(
            commands.lookup(&intent),
            Err(CommandError::Receipt(ReceiptStoreError::Poisoned))
        ));
        if after_execution {
            boundary += 1;
        }
        assert_eq!(commands.status(&identity).unwrap().boundary(), boundary);
        drop(commands);
        let commands = reopen();
        assert_eq!(
            commands
                .submit(intent)
                .unwrap()
                .outcome()
                .await
                .unwrap()
                .state(),
            &RunCommandState::Finished(RunCommandOutcome::Indeterminate)
        );
        assert_eq!(commands.status(&identity).unwrap().boundary(), boundary);
    }
    for after_execution in [false, true] {
        let commands = reopen();
        let intent = self::request(
            &identity,
            if after_execution {
                "close-after"
            } else {
                "close-before"
            },
            boundary,
            RunCommand::Step,
        );
        let (entered, release) = commands.hold(after_execution);
        let submission = commands.submit(intent).unwrap();
        crossed(entered).await;
        commands.shutdown();
        release.send(()).unwrap();
        let receipt = submission.outcome().await.unwrap();
        if after_execution {
            boundary += 1;
            assert_eq!(applied(&receipt).boundary(), boundary);
        } else {
            assert!(matches!(
                receipt.state(),
                RunCommandState::Finished(RunCommandOutcome::Refused {
                    reason: RunCommandRefusal::Cancelled
                })
            ));
        }
        assert_eq!(commands.status(&identity).unwrap().boundary(), boundary);
        assert!(matches!(
            commands.submit(self::request(
                &identity,
                "closed",
                boundary,
                RunCommand::Step
            )),
            Err(CommandError::Closed)
        ));
    }
    loads.shutdown();
    released(&service).await;
    handle.shutdown().await.unwrap();
    owner.await.unwrap().unwrap();
}

#[tokio::test]
async fn daemon_owner_drop_cancels_command_even_with_live_client_handles() {
    use crate::{credentials::WorkerCredentials, runtime::WorkerRuntime};
    let dir = directory();
    let (runtime, owner) = WorkerRuntime::standalone(
        WorkerCredentials::load_or_create(dir.path()).unwrap(),
        "worker".parse().unwrap(),
        "cluster".parse().unwrap(),
        vec![],
    )
    .unwrap()
    .start();
    assert!(matches!(
        runtime.install_run_command_coordinator(
            CommandReceiptStore::open(dir.path()).unwrap(),
            Duration::from_secs(10),
        ),
        Err(CommandError::Policy)
    ));
    assert!(runtime.run_command_coordinator().is_none());
    let summary = runtime.summary().unwrap();
    let sandbox = Arc::new(Sandbox::new(SandboxLimits::default()).unwrap());
    runtime
        .install_load_coordinator(
            ReceiptStore::open(dir.path()).unwrap(),
            sandbox,
            LoadPolicy::default(),
        )
        .unwrap();
    assert!(matches!(
        runtime.install_run_command_coordinator(
            CommandReceiptStore::open(dir.path()).unwrap(),
            Duration::ZERO,
        ),
        Err(CommandError::Policy)
    ));
    assert!(runtime.run_command_coordinator().is_none());
    let commands = runtime
        .install_run_command_coordinator(
            CommandReceiptStore::open(dir.path()).unwrap(),
            Duration::from_secs(10),
        )
        .unwrap();
    let duplicate_dir = directory();
    assert!(matches!(
        runtime.install_run_command_coordinator(
            CommandReceiptStore::open(duplicate_dir.path()).unwrap(),
            Duration::from_secs(10),
        ),
        Err(CommandError::Busy)
    ));
    let identity = RunIdentity::new(
        summary.formation_id,
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
        WorkloadEpoch::new(1),
    );
    let intent = request(&identity, "drop", 0, RunCommand::Step);
    let (entered, release) = commands.hold(false);
    let submission = commands.submit(intent.clone()).unwrap();
    crossed(entered).await;
    drop(runtime);
    assert!(matches!(
        commands.submit(request(&identity, "new", 0, RunCommand::Step)),
        Err(CommandError::Closed)
    ));
    release.send(()).unwrap();
    assert!(matches!(
        submission.outcome().await.unwrap().state(),
        RunCommandState::Finished(RunCommandOutcome::Refused {
            reason: RunCommandRefusal::Cancelled
        })
    ));
    wait(&commands).await;
    assert!(commands.lookup(&intent).unwrap().is_some());
    owner.abort();
    let _ = owner.await;
}
