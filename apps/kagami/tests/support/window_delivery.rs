//! Production window update-path delivery, with an explicitly supplied native
//! worker. No window, test-only submission API or worker plugin inventory.
use kagami::{
    message::{Message, WorkspaceIntent},
    model::Model,
    run::{Action, Connection, Controller, Recovery},
    update::update,
    workload_preparation::Action as Workload,
};
use orishu::{
    client::{
        ClientApi, ClusterAddress, ClusterApi, MembershipApi, credential_file,
        http_client::{HttClientOptions, HttpClusterClient},
    },
    model::{
        cluster::LockRequest,
        run_load::{LoadOutcome, LoadState},
    },
};
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn poll(model: &mut Model) {
    let end = Instant::now() + Duration::from_secs(80);
    while model.run.is_pending() {
        let _ = update(model, Message::Run(Action::Poll));
        assert!(Instant::now() < end, "window network job timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn action(model: &mut Model, action: Action) {
    let _ = update(model, Message::Run(action));
    poll(model);
}

/// Open the journal as a restarted process would. A process that the worker
/// test spawns in parallel can hold a copy of the released lock descriptor
/// until its `exec`, so a brief InUse is retried.
fn reopen(path: &std::path::Path) -> Recovery {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let recovery = Recovery::open(path);
        match &recovery {
            Recovery::Unavailable(reason)
                if reason.contains("Another Kagami instance") && Instant::now() < end =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => return recovery,
        }
    }
}

fn plugins(model: &mut Model, action: kagami::plugins::window::Action) {
    let _ = update(model, Message::Plugins(action));
    let end = Instant::now() + Duration::from_secs(15);
    while model.plugin_management.is_pending() || model.plugin_references.is_pending() {
        assert!(Instant::now() < end, "plugin refresh timed out");
        std::thread::sleep(Duration::from_millis(5));
        let _ = update(
            model,
            Message::Plugins(kagami::plugins::window::Action::Poll),
        );
    }
}

/// A dropped upload reply must keep both identity and original archive bytes,
/// even when a later UI edit discards the prepared candidate.
pub fn lost_reply(model: &mut Model, expected: &[u8]) {
    use orishu::model::{
        ApiResponse, ResponseData,
        run_load::{LoadReceipt, LoadRefusal, LoadRequest},
    };
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
    };
    let dir = tempfile::tempdir().unwrap();
    let token = dir.path().join("token");
    std::fs::write(&token, "a".repeat(64)).unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    let socket = dir.path().join("load.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let expected = expected.to_vec();
    let server = std::thread::spawn(move || {
        let mut original: Option<LoadRequest> = None;
        for index in 0..4 {
            let until = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("bounded load fixture: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut head = vec![];
            while !head.ends_with(b"\r\n\r\n") {
                assert!(head.len() < 8192);
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                head.push(b[0]);
            }
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            assert!(head.contains(&format!("authorization: bearer {}\r\n", "a".repeat(64))));
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            assert!(length <= expected.len() + 4100);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let request: LoadRequest = if index == 0 || index == 2 {
                assert!(head.starts_with("post /api/v1/run-loads "));
                let prefix = u32::from_be_bytes(body[..4].try_into().unwrap()) as usize;
                assert_eq!(&body[4 + prefix..], &expected);
                ciborium::from_reader(&body[4..4 + prefix]).unwrap()
            } else {
                ciborium::from_reader(&body[..]).unwrap()
            };
            if let Some(original) = &original {
                assert_eq!(&request, original);
            } else {
                original = Some(request.clone());
            }
            if index == 0 {
                continue;
            } // Lost reply after complete upload.
            let (status, reply) = if index == 1 {
                (
                    404,
                    ApiResponse::Error {
                        code: "OperationNotFound".into(),
                        message: "missing".into(),
                    },
                )
            } else {
                let state = if index == 2 {
                    LoadState::Pending
                } else {
                    LoadState::Finished(LoadOutcome::Refused {
                        reason: LoadRefusal::Policy,
                    })
                };
                (
                    if index == 2 { 202 } else { 200 },
                    ApiResponse::Ok {
                        data: Some(ResponseData::RunLoadReceipt(
                            LoadReceipt::new(request, "node-a".parse().unwrap(), state).unwrap(),
                        )),
                    },
                )
            };
            let mut bytes = vec![];
            ciborium::into_writer(&reply, &mut bytes).unwrap();
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
        }
    });
    let journal = dir.path().join("runs");
    let connection = Connection {
        address: ClusterAddress::UnixSocket(socket),
        token_file: token.clone(),
        ca_cert: None,
    };
    model.run = Controller::new(Some(connection.clone()), Recovery::open(&journal));
    // A second instance observes only; it explains why it cannot submit.
    let other = Controller::new(Some(connection.clone()), Recovery::open(&journal));
    assert!(
        other
            .check_submit()
            .unwrap_err()
            .contains("Another Kagami instance")
    );
    assert!(other.check(&Action::Discover).is_ok());
    drop(other);
    let _ = update(
        model,
        Message::Workload(Workload::Formation("formation-a".into())),
    );
    let _ = update(model, Message::Workload(Workload::Submit));
    poll(model);
    let original = model.run.submission().unwrap().request.clone();
    assert!(model.run.submission().unwrap().unresolved());
    let recorded = std::fs::read(journal.join("intents.json")).unwrap();
    let contains = |needle: &[u8]| recorded.windows(needle.len()).any(|w| w == needle);
    assert!(contains(
        String::from(original.operation_id().clone()).as_bytes()
    ));
    assert!(!contains("a".repeat(64).as_bytes()), "no credential");
    assert!(
        !contains(token.as_os_str().as_encoded_bytes()),
        "no credential path"
    );

    // Restart. Against another worker, the recovered intent is shown but not
    // reconciled; against the same worker, it is restored without its bytes in
    // memory, and nothing is sent automatically.
    model.run = Controller::new(None, Recovery::default());
    let elsewhere = Connection {
        address: ClusterAddress::UnixSocket(dir.path().join("other.sock")),
        ..connection.clone()
    };
    model.run = Controller::new(Some(elsewhere), reopen(&journal));
    assert_eq!(model.run.submission().unwrap().request, original);
    assert!(
        model
            .run
            .check(&Action::ReconcileLoad)
            .unwrap_err()
            .contains("--host")
    );
    model.run = Controller::new(None, Recovery::default());
    model.run = Controller::new(Some(connection), reopen(&journal));
    assert_eq!(model.run.submission().unwrap().request, original);
    assert!(model.run.notice.contains("Recovered"));
    assert!(!model.run.is_pending());
    let _ = update(
        model,
        Message::Workload(Workload::Name("changed-after-submit".into())),
    );
    assert!(model.workload_preparation.ready(&model.document).is_none());
    let _ = update(model, Message::Workload(Workload::Prepare));
    assert!(!model.workload_preparation.is_pending());
    action(model, Action::ClearLoad);
    assert_eq!(
        model.run.submission().unwrap().request,
        original,
        "uncertain intent cannot be cleared"
    );
    action(model, Action::ReconcileLoad);
    assert!(model.run.submission().unwrap().receipt.is_none());
    action(model, Action::ResubmitLoad);
    assert_eq!(
        model
            .run
            .submission()
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .state(),
        &LoadState::Pending
    );
    action(model, Action::ReconcileLoad);
    assert!(!model.run.submission().unwrap().unresolved());
    assert!(model.is_authoring() && !model.run.attached());
    action(model, Action::ClearLoad);
    assert!(model.run.submission().is_none());
    server.join().unwrap();
    let recorded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(journal.join("intents.json")).unwrap()).unwrap();
    assert!(recorded["load"].is_null());
    let _ = update(
        model,
        Message::Workload(Workload::Name("ui-created".into())),
    );
    let _ = update(model, Message::Workload(Workload::Prepare));
    super::poll_preparation(model);
    assert!(model.workload_preparation.ready(&model.document).is_some());
}
pub fn real_worker(model: &mut Model) {
    struct Worker(std::process::Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let binary = std::env::var_os("ORISHU_TEST_WORKER").expect("explicit native worker");
    assert!(std::path::Path::new(&binary).is_absolute());
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let state = dir.path().join("state");
    let socket = dir.path().join("worker.sock");
    let mut command = Command::new(binary);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ORISHU_") {
            command.env_remove(name);
        }
    }
    let mut worker = Worker(
        command
            .arg("--state-dir")
            .arg(&state)
            .arg("--listen.clients")
            .arg(&socket)
            .args(["--scientific.enabled", "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let address = ClusterAddress::UnixSocket(socket);
    let observer = HttpClusterClient::new(address.clone(), Default::default()).unwrap();
    let summary = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                assert!(worker.0.try_wait().unwrap().is_none());
                if let Ok(summary) = observer.cluster().summary().await {
                    break summary;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    });
    let token = state.join("operator.token");
    let operator = HttpClusterClient::new(
        address.clone(),
        HttClientOptions {
            credentials: Some(credential_file::load(&token).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    runtime
        .block_on(operator.membership().set_lock(&LockRequest {
            schema_version: 1,
            operation_id: "lock-window-test".parse().unwrap(),
            formation_id: summary.formation_id.clone(),
            locked: true,
        }))
        .unwrap();
    let connection = Connection {
        address,
        token_file: token,
        ca_cert: None,
    };
    model.run = Controller::new(
        Some(connection.clone()),
        Recovery::open(&dir.path().join("runs")),
    );
    let before = model.document.snapshot().clone();
    let view = model.document.authoring_view();
    let root = model
        .workload_preparation
        .ready(&model.document)
        .unwrap()
        .report
        .workload;
    let _ = update(
        model,
        Message::Workload(Workload::Formation(summary.formation_id.to_string())),
    );
    let _ = update(model, Message::Workload(Workload::Submit));
    let intent = model.run.submission().unwrap().request.clone();
    assert_eq!(intent.workload_id(), root);
    poll(model);
    let end = Instant::now() + Duration::from_secs(80);
    loop {
        let submission = model.run.submission().unwrap();
        assert_eq!(submission.request, intent);
        match submission.receipt.as_ref().map(|r| r.state()) {
            Some(LoadState::Finished(LoadOutcome::Accepted { .. })) => break,
            Some(LoadState::Pending) => {
                assert!(Instant::now() < end);
                action(model, Action::ReconcileLoad);
            }
            state => panic!("load failed: {state:?}; {}", model.run.notice),
        }
    }
    assert!(model.is_authoring(), "a historical receipt is not a frame");
    action(model, Action::InspectLoaded);
    assert_eq!(model.run.source_revision(), Some(before.revision().get()));
    assert_eq!(model.run.status().unwrap().boundary(), 0);
    action(model, Action::Observe);
    assert!(!model.is_authoring() && model.run.attached());
    assert_eq!(model.run.objects().unwrap().count, before.object_count());
    assert!(model.run.can_control());
    let initial_markers: Vec<_> = model
        .run
        .objects()
        .unwrap()
        .geometry
        .markers
        .markers()
        .iter()
        .map(|m| m.position())
        .collect();
    let initial = model
        .run
        .objects()
        .unwrap()
        .rows
        .iter()
        .find(|r| r.object.inertial_mass_kilograms.is_some())
        .unwrap()
        .object;
    let fixed = model
        .run
        .objects()
        .unwrap()
        .rows
        .iter()
        .find(|r| r.object.inertial_mass_kilograms.is_none())
        .unwrap()
        .object;
    // Inventory is an authoring preference, never an execution dependency of
    // an accepted portable workload. Disable an installed provider via the UI
    // while attached, then prove the worker still advances the same run.
    use kagami::plugins::{InventoryCommand, window::Action as PluginAction};
    plugins(model, PluginAction::Open);
    let provider = model
        .plugin_management
        .listing
        .as_ref()
        .unwrap()
        .releases
        .iter()
        .find(|entry| entry.enabled)
        .unwrap()
        .plugin_id
        .clone();
    plugins(
        model,
        PluginAction::Mutate(InventoryCommand::SetEnabled {
            plugin_id: provider.clone(),
            enabled: false,
        }),
    );
    assert!(
        model
            .plugin_management
            .listing
            .as_ref()
            .unwrap()
            .releases
            .iter()
            .filter(|e| e.plugin_id == provider)
            .all(|e| !e.enabled)
    );
    assert!(model.run.attached() && model.run.can_control());
    assert_eq!(model.document.snapshot(), &before);
    action(model, Action::Step);
    action(model, Action::Refresh);
    assert_eq!(model.run.status().unwrap().boundary(), 1);
    assert_eq!(model.run.objects().unwrap().force_boundary, Some(0));
    let frame = model.run.objects().unwrap();
    assert_eq!(frame.geometry.markers.markers().len(), frame.count);
    let markers: Vec<_> = frame
        .geometry
        .markers
        .markers()
        .iter()
        .map(|m| m.position())
        .collect();
    assert_ne!(
        markers, initial_markers,
        "worker integration moves viewport markers"
    );
    for (row, marker) in frame.rows.iter().zip(frame.geometry.markers.markers()) {
        assert_eq!(
            Some(marker.position()),
            kagami::viewport::geometry_units(
                frame.geometry.scale,
                row.object.kinematics.position_metres.map(|v| v.get())
            )
        );
    }
    let moved = model
        .run
        .objects()
        .unwrap()
        .rows
        .iter()
        .find(|r| r.object.id == initial.id)
        .unwrap();
    assert_ne!(
        moved.object.kinematics.position_metres,
        initial.kinematics.position_metres
    );
    assert!(
        moved.force.unwrap().newtons[0].get() > 0.0,
        "Newtonian field contributes an attractive force"
    );
    assert_eq!(
        model
            .run
            .objects()
            .unwrap()
            .rows
            .iter()
            .find(|r| r.object.id == fixed.id)
            .unwrap()
            .object
            .kinematics,
        fixed.kinematics,
        "no Dynamics means no integration"
    );
    inspect_fields(model, &connection, &runtime);
    plugins(
        model,
        PluginAction::Mutate(InventoryCommand::SetEnabled {
            plugin_id: provider,
            enabled: true,
        }),
    );
    plugins(model, PluginAction::Close);
    action(model, Action::Finish);
    let _ = update(
        model,
        Message::Workspace(WorkspaceIntent::EditInitialConditions),
    );
    assert!(model.is_authoring() && !model.run.attached());
    assert_eq!(model.document.snapshot(), &before);
    assert_eq!(model.document.authoring_view(), view);
    action(model, Action::ClearLoad);
}

fn inspect_fields(model: &mut Model, connection: &Connection, runtime: &tokio::runtime::Runtime) {
    use kagami::run::fields::Action as Field;
    use orishu_plugin::{
        ExecutionContractId,
        execution::{SampleInvalidity, SnapshotSource},
    };
    let before = model.document.snapshot().clone();
    let boundary = model.run.status().unwrap().boundary();
    let instance = before
        .setup()
        .scientific()
        .unwrap()
        .captures()
        .values()
        .find(|c| c.context.execution_contract == ExecutionContractId::Field)
        .unwrap()
        .context
        .instance
        .to_string();
    action(model, Action::Field(Field::Instance(instance)));
    // A changed local form invalidates a reply even if it completed before the
    // next poll. No retry/new boundary is silently substituted.
    model.run.act(Action::Field(Field::Inspect));
    assert!(model.run.is_pending());
    model.run.act(Action::Field(Field::Points("0,1,0".into())));
    poll(model);
    assert!(model.run.fields.descriptor().is_none());
    action(model, Action::Field(Field::Inspect));
    let descriptor = model.run.fields.descriptor().unwrap();
    assert!(
        matches!(descriptor.snapshot.source, SnapshotSource::Committed { boundary: actual, .. } if actual == boundary)
    );
    let count = descriptor.context.observables.len();
    assert_eq!(count, 3);
    let stale = model.run.fields.generation();
    action(model, Action::Field(Field::Points("NaN,0,0".into())));
    action(
        model,
        Action::Field(Field::Channel {
            generation: stale,
            index: 0,
        }),
    );
    assert!(!model.run.fields.channel_selected(0));
    for index in 0..count {
        action(
            model,
            Action::Field(Field::Channel {
                generation: model.run.fields.generation(),
                index,
            }),
        );
    }
    action(model, Action::Field(Field::Sample));
    assert!(model.run.fields.report().is_none());
    assert!(!model.run.is_pending());
    action(
        model,
        Action::Field(Field::Points("0,1,0;1,0,0;20,0,0".into())),
    );
    action(model, Action::Field(Field::Sample));
    let report = model
        .run
        .fields
        .report()
        .unwrap_or_else(|| panic!("{}", model.run.notice));
    assert_eq!(report.points, 3);
    assert_eq!(report.cells, 9);
    assert_eq!(report.rows.len(), 9);
    for row in &report.rows {
        match row.point.id {
            0 => {
                assert!(row.invalidity.is_none());
                assert_eq!(row.quality, Some(1));
                assert_eq!(row.values.len(), row.total_values);
            }
            1 => {
                assert_eq!(row.invalidity, Some(SampleInvalidity::Singular));
                assert!(row.values.is_empty());
                assert!(row.quality.is_none());
            }
            2 => assert_eq!(row.invalidity, Some(SampleInvalidity::OutsideDomain)),
            _ => unreachable!(),
        }
    }
    let acceleration = report
        .rows
        .iter()
        .find(|r| {
            r.point.id == 0
                && report.metadata.channels[r.channel]
                    .schema
                    .name
                    .as_str()
                    .ends_with("acceleration")
        })
        .unwrap();
    assert!(report.values[acceleration.values.start + 1].get() < 0.0);
    let channel = acceleration.channel;
    let other_channel = report
        .metadata
        .channels
        .iter()
        .position(|c| c.schema.shape != (orishu_plugin::Shape::Vector { length: 3 }))
        .unwrap();
    let report_digest = report.response_digest;
    action(model, Action::Field(Field::VectorLength("0.25".into())));
    action(
        model,
        Action::Field(Field::Vectors {
            generation: model.run.fields.generation(),
            channel,
        }),
    );
    let vectors = model
        .run
        .fields
        .vectors()
        .unwrap_or_else(|| panic!("{}", model.run.notice));
    assert_eq!(vectors.points, 3);
    assert_eq!(vectors.arrows.arrows().len(), 1);
    assert_eq!(vectors.invalid, 2);
    assert_eq!(vectors.zero, 0);
    assert_eq!(vectors.omitted_scale, 0);
    let arrow = vectors.arrows.arrows()[0];
    assert!(arrow.end()[1] < arrow.start()[1]);
    let length = arrow
        .end()
        .into_iter()
        .zip(arrow.start())
        .map(|(b, a)| (b - a).powi(2))
        .sum::<f32>()
        .sqrt();
    assert!((length - 0.25).abs() < 1e-6);
    assert_eq!(
        model.run.fields.report().unwrap().response_digest,
        report_digest
    );
    action(
        model,
        Action::Field(Field::Vectors {
            generation: model.run.fields.generation(),
            channel: other_channel,
        }),
    );
    assert!(model.run.notice.contains("vector-3"));
    // A stale local projection must not redraw after the user hides it.
    model.run.act(Action::Field(Field::Vectors {
        generation: model.run.fields.generation(),
        channel,
    }));
    assert!(model.run.is_pending());
    model.run.act(Action::Field(Field::HideVectors));
    poll(model);
    assert!(model.run.fields.vectors().is_none());
    assert!(
        model.run.fields.report().is_some(),
        "hide does not discard numeric evidence"
    );
    let stale = model.run.fields.generation();
    action(model, Action::Field(Field::VectorLength("NaN".into())));
    action(
        model,
        Action::Field(Field::Vectors {
            generation: stale,
            channel,
        }),
    );
    assert!(model.run.notice.contains("stale"));
    action(
        model,
        Action::Field(Field::Vectors {
            generation: model.run.fields.generation(),
            channel,
        }),
    );
    assert!(model.run.notice.contains("positive"));
    assert_eq!(model.run.status().unwrap().boundary(), boundary);
    assert_eq!(model.document.snapshot(), &before);
    assert!(
        model.run.can_control(),
        "sampling does not create a scientific command intent"
    );
    // Display admission is independent of scientific request completeness.
    let many = std::iter::repeat_n("0,1,0", 100)
        .collect::<Vec<_>>()
        .join(";");
    action(model, Action::Field(Field::Points(many)));
    assert!(model.run.fields.report().is_none());
    action(model, Action::Field(Field::Sample));
    let report = model
        .run
        .fields
        .report()
        .unwrap_or_else(|| panic!("{}", model.run.notice));
    assert_eq!(report.points, 100);
    assert_eq!(report.cells, 300);
    assert_eq!(report.rows.len(), 256);
    assert!(report.values.len() <= 256 * 16);
    action(model, Action::Field(Field::VectorLength("1".into())));
    action(
        model,
        Action::Field(Field::Vectors {
            generation: model.run.fields.generation(),
            channel,
        }),
    );
    assert_eq!(
        model.run.fields.vectors().unwrap().arrows.arrows().len(),
        100,
        "uses every queried point, not the truncated table"
    );
    // Scale changes reproject locally from retained SI values, with no request
    // or change to the report's exact source/packets.
    model
        .run
        .synchronize_scale(kagami_session::SceneScale::NANOMETRE);
    let end = Instant::now() + Duration::from_secs(10);
    while model.run.is_pending() {
        assert!(Instant::now() < end, "local vector projection timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
        model.run.poll();
        model
            .run
            .synchronize_scale(kagami_session::SceneScale::NANOMETRE);
    }
    let vectors = model.run.fields.vectors().unwrap();
    assert_eq!(vectors.scale, kagami_session::SceneScale::NANOMETRE);
    assert_eq!(vectors.omitted_scale, 100);
    assert!(vectors.arrows.arrows().is_empty());
    model
        .run
        .synchronize_scale(kagami_session::SceneScale::METRE);
    poll(model);
    // Another controller can advance after descriptor discovery. A stale sample
    // must remain an explicit refusal, not a query against the newer boundary.
    use orishu::model::run_command::{
        RunCommand, RunCommandOutcome, RunCommandRequest, RunCommandState,
    };
    let command = RunCommandRequest::new(
        "field-test-other-controller".parse().unwrap(),
        model.run.status().unwrap().descriptor().identity().clone(),
        boundary,
        RunCommand::Step,
    )
    .unwrap();
    // This is an independent controller, not the pooled administrative client
    // whose current-thread executor was parked during all window requests.
    let other = HttpClusterClient::new(
        connection.address.clone(),
        HttClientOptions {
            credentials: Some(credential_file::load(&connection.token_file).unwrap()),
            tls_cert: connection.ca_cert.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    let receipt = runtime
        .block_on(other.scientific().command(&command))
        .unwrap();
    assert!(matches!(
        receipt.state(),
        RunCommandState::Finished(RunCommandOutcome::Applied { .. })
    ));
    action(model, Action::Field(Field::Sample));
    assert!(model.run.fields.report().is_none());
    assert!(model.run.fields.vectors().is_none());
    assert!(
        model.run.notice.contains("StaleBoundary"),
        "{}",
        model.run.notice
    );
    assert_eq!(
        model.run.status().unwrap().boundary(),
        boundary,
        "sampling must not implicitly refresh"
    );
    action(model, Action::Refresh);
    assert_eq!(model.run.status().unwrap().boundary(), boundary + 1);
    assert!(model.run.fields.descriptor().is_none());
    assert!(model.run.fields.report().is_none());
    assert_eq!(model.document.snapshot(), &before);
}
