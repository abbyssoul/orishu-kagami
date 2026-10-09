#![cfg(unix)]
//! Real headless Kagami executable: bounded portable inputs and identified HTTP
//! outcomes. The opt-in final test adds a separately built actual worker.
use orishu::model::{
    ApiResponse, ResponseData,
    run::{RunDescriptor, RunIdentity, WorkloadEpoch},
    run_command::*,
    run_load::*,
    run_observation::*,
};
use std::{
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[path = "../../../crates/orishu-runtime/tests/reference_support/admission.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../../../crates/orishu-runtime/tests/reference_support/mod.rs"]
mod reference_support;

fn command(socket: &Path, token: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kagami"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ORISHU_")
            || key.to_string_lossy().starts_with("KAGAMI_")
        {
            command.env_remove(key);
        }
    }
    command.env_remove("RUST_LOG");
    command
        .arg("--host")
        .arg(socket)
        .args(["workload", "--operator-token-file"])
        .arg(token)
        .arg("--json");
    command
}
fn run(mut command: Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Reports can contain bounded scientific batches. Drain both pipes while
    // waiting so a valid larger report cannot deadlock against pipe capacity.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = vec![];
        stderr.take(64 * 1024 + 1).read_to_end(&mut bytes).unwrap();
        bytes
    });
    let end = Instant::now() + Duration::from_secs(85);
    while child.try_wait().unwrap().is_none() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    let timed_out = child.try_wait().unwrap().is_none();
    if timed_out {
        let _ = child.kill();
    }
    let output = Output {
        status: child.wait().unwrap(),
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    assert!(!timed_out, "headless command exceeded deadline");
    assert!(output.stdout.len() <= 64 * 1024 * 1024 && output.stderr.len() <= 64 * 1024);
    output
}
fn result(output: Output, code: i32, outcome: &str) -> serde_json::Value {
    versioned_result(output, code, outcome, "kagami.workload-command/v1")
}
fn control_result(output: Output, code: i32, outcome: &str) -> serde_json::Value {
    versioned_result(output, code, outcome, "kagami.run-command/v1")
}
fn observation_result(output: Output, code: i32, outcome: &str) -> serde_json::Value {
    versioned_result(output, code, outcome, "kagami.object-observation/v1")
}
fn versioned_result(output: Output, code: i32, outcome: &str, version: &str) -> serde_json::Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&"a".repeat(64)));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&"a".repeat(64)));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["apiVersion"], version);
    assert_eq!(value["outcome"], outcome);
    value
}
fn token(path: &Path) {
    std::fs::write(path, "a".repeat(64)).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn request() -> LoadRequest {
    LoadRequest::new(
        "upload-1".parse().unwrap(),
        "formation-a".parse().unwrap(),
        format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
    )
}
fn intent(command: &mut Command, request: &LoadRequest) {
    command
        .arg("--formation-id")
        .arg(request.formation_id().as_str())
        .arg("--operation-id")
        .arg(String::from(request.operation_id().clone()))
        .arg("--workload-id")
        .arg(request.workload_id().to_string());
}
fn exact_run(command: &mut Command, run: &RunIdentity) {
    command
        .arg("--formation-id")
        .arg(run.formation_id().as_str())
        .arg("--workload-id")
        .arg(run.workload_id().to_string())
        .arg("--workload-epoch")
        .arg(run.workload_epoch().get().to_string());
}
fn control_intent(command: &mut Command, request: &RunCommandRequest) {
    exact_run(command, request.run());
    command
        .arg("--operation-id")
        .arg(String::from(request.operation_id().clone()))
        .arg("--expected-boundary")
        .arg(request.expected_boundary().to_string());
}
fn control_request(action: RunCommand) -> RunCommandRequest {
    RunCommandRequest::new(
        "control-1".parse().unwrap(),
        descriptor(&request()).identity().clone(),
        3,
        action,
    )
    .unwrap()
}
fn control_fact(request: &RunCommandRequest, state: RunCommandState) -> RunCommandReceipt {
    RunCommandReceipt::new(request.clone(), "historic-worker".parse().unwrap(), state).unwrap()
}
fn applied_status(request: &RunCommandRequest) -> RunStatus {
    let (boundary, phase) = match request.command() {
        RunCommand::Step => (request.expected_boundary() + 1, RunPhase::Ready),
        RunCommand::Finish => (request.expected_boundary(), RunPhase::Finished),
    };
    RunStatus::new(
        RunDescriptor::new(request.run().clone()),
        boundary,
        0.5,
        phase,
    )
    .unwrap()
}

#[test]
fn control_commands_preserve_exact_intent_and_unknown_delivery_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    for (name, action) in [("step", RunCommand::Step), ("finish", RunCommand::Finish)] {
        let request = control_request(action);
        for lost in [false, true] {
            let socket = dir.path().join(format!("{name}-{lost}.sock"));
            let receipt = control_fact(
                &request,
                RunCommandState::Finished(RunCommandOutcome::Applied {
                    status: applied_status(&request),
                }),
            );
            let server = serve(
                UnixListener::bind(&socket).unwrap(),
                (!lost).then(|| (200, ok(ResponseData::RunCommandReceipt(receipt.clone())))),
                4096,
            );
            let mut cmd = command(&socket, &credential);
            cmd.arg(name);
            control_intent(&mut cmd, &request);
            let report = control_result(
                run(cmd),
                if lost { 1 } else { 0 },
                if lost { "error" } else { "applied" },
            );
            assert_eq!(report["request"], serde_json::to_value(&request).unwrap());
            if lost {
                assert_eq!(report["error"]["submissionOutcomeUnknown"], true);
            } else {
                assert_eq!(report["receipt"], serde_json::to_value(&receipt).unwrap());
            }
            assert!(
                report.get("status").is_none(),
                "historical receipt is not live status"
            );
            let (head, body) = server.join().unwrap();
            assert!(head.starts_with("post /api/v1/run-commands http/1.1"));
            assert_eq!(
                ciborium::from_reader::<RunCommandRequest, _>(&body[..]).unwrap(),
                request
            );
        }
    }
}

#[test]
fn control_receipts_and_live_status_have_distinct_reports_and_exit_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let request = control_request(RunCommand::Step);
    for (state, code, outcome) in [
        (RunCommandState::Pending, 10, "pending"),
        (
            RunCommandState::Finished(RunCommandOutcome::Refused {
                reason: RunCommandRefusal::StaleBoundary { actual: 4 },
            }),
            11,
            "refused",
        ),
        (
            RunCommandState::Finished(RunCommandOutcome::Indeterminate),
            12,
            "indeterminate",
        ),
        (
            RunCommandState::Finished(RunCommandOutcome::Applied {
                status: applied_status(&request),
            }),
            0,
            "applied",
        ),
    ] {
        let socket = dir.path().join(format!("{outcome}.sock"));
        let receipt = control_fact(&request, state);
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((200, ok(ResponseData::RunCommandReceipt(receipt.clone())))),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        cmd.args(["command-receipt", "--action", "step"]);
        control_intent(&mut cmd, &request);
        let report = control_result(run(cmd), code, outcome);
        assert_eq!(report["receipt"], serde_json::to_value(&receipt).unwrap());
        assert!(report.get("status").is_none());
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("post /api/v1/run-commands/lookup http/1.1"));
        assert_eq!(
            ciborium::from_reader::<RunCommandRequest, _>(&body[..]).unwrap(),
            request
        );
    }
    for phase in [RunPhase::Ready, RunPhase::Finished] {
        let socket = dir.path().join(format!("{phase:?}.sock"));
        let status =
            RunStatus::new(RunDescriptor::new(request.run().clone()), 4, 0.5, phase).unwrap();
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((200, ok(ResponseData::RunStatus(status.clone())))),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg("status");
        exact_run(&mut cmd, request.run());
        let report = control_result(run(cmd), 0, "live");
        assert_eq!(report["status"], serde_json::to_value(&status).unwrap());
        assert!(report.get("receipt").is_none());
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("post /api/v1/run/status http/1.1"));
        assert_eq!(
            ciborium::from_reader::<RunStatusRequest, _>(&body[..]).unwrap(),
            RunStatusRequest::new(request.run().clone())
        );
    }
}

#[test]
fn control_errors_preserve_intent_without_exposing_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let request = control_request(RunCommand::Step);
    let other = RunCommandRequest::new(
        "other".parse().unwrap(),
        request.run().clone(),
        3,
        RunCommand::Step,
    )
    .unwrap();
    for (index, (name, status, reply, exit, outcome, error)) in [
        (
            "command-receipt",
            404,
            ApiResponse::Error {
                code: "OperationNotFound".into(),
                message: "missing".into(),
            },
            13,
            "notFound",
            None,
        ),
        (
            "status",
            404,
            ApiResponse::Error {
                code: "RunUnavailable".into(),
                message: "missing".into(),
            },
            13,
            "empty",
            None,
        ),
        (
            "status",
            404,
            ApiResponse::Error {
                code: "NotFound".into(),
                message: "missing API".into(),
            },
            1,
            "error",
            Some("NotFound"),
        ),
        (
            "step",
            200,
            ok(ResponseData::RunCommandReceipt(control_fact(
                &other,
                RunCommandState::Pending,
            ))),
            1,
            "error",
            Some("protocol"),
        ),
        (
            "step",
            401,
            ApiResponse::Error {
                code: "Unauthorized".into(),
                message: "a".repeat(64),
            },
            1,
            "error",
            Some("credential_reflection"),
        ),
        (
            "status",
            401,
            ApiResponse::Error {
                code: "Unauthorized".into(),
                message: "a".repeat(64),
            },
            1,
            "error",
            Some("credential_reflection"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let socket = dir.path().join(format!("error-{index}.sock"));
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((status, reply)),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg(name);
        if name == "status" {
            exact_run(&mut cmd, request.run());
        } else {
            control_intent(&mut cmd, &request);
        }
        if name == "command-receipt" {
            cmd.args(["--action", "step"]);
        }
        let report = control_result(run(cmd), exit, outcome);
        let expected = if name == "status" {
            serde_json::to_value(RunStatusRequest::new(request.run().clone())).unwrap()
        } else {
            serde_json::to_value(&request).unwrap()
        };
        assert_eq!(report["request"], expected);
        if let Some(error) = error {
            assert_eq!(report["error"]["code"], error);
            assert_eq!(report["error"]["submissionOutcomeUnknown"], name == "step");
        }
        server.join().unwrap();
    }
}

#[test]
fn control_invalid_intent_is_rejected_before_network() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    for (name, operation, boundary, expected_code) in [
        ("step", "a".repeat(64), 0, "credential_in_intent"),
        ("step", "overflow".into(), u64::MAX, "invalid_request"),
    ] {
        let mut cmd = command(&socket, &credential);
        cmd.arg(name);
        exact_run(&mut cmd, control_request(RunCommand::Step).run());
        cmd.arg("--operation-id")
            .arg(operation)
            .arg("--expected-boundary")
            .arg(boundary.to_string());
        let report = control_result(run(cmd), 1, "error");
        assert_eq!(report["error"]["code"], expected_code);
        assert_eq!(report["error"]["submissionOutcomeUnknown"], false);
        assert!(report.get("request").is_none());
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
fn fact(request: &LoadRequest, state: LoadState) -> LoadReceipt {
    LoadReceipt::new(request.clone(), "historical-node".parse().unwrap(), state).unwrap()
}
fn descriptor(request: &LoadRequest) -> RunDescriptor {
    RunDescriptor::new(RunIdentity::new(
        request.formation_id().clone(),
        request.workload_id(),
        WorkloadEpoch::new(1),
    ))
}
fn ok(data: ResponseData) -> ApiResponse {
    ApiResponse::Ok { data: Some(data) }
}
fn cbor(value: &impl serde::Serialize) -> Vec<u8> {
    let mut bytes = vec![];
    ciborium::into_writer(value, &mut bytes).unwrap();
    bytes
}

/// One bounded request only. No client auto-retry may be hidden by this fixture.
fn serve(
    listener: UnixListener,
    reply: Option<(u16, ApiResponse)>,
    max: usize,
) -> std::thread::JoinHandle<(String, Vec<u8>)> {
    serve_bytes(
        listener,
        reply.map(|(status, reply)| {
            (
                status,
                "Content-Type: application/cbor\r\n".into(),
                cbor(&reply),
            )
        }),
        max,
    )
}
fn serve_bytes(
    listener: UnixListener,
    reply: Option<(u16, String, Vec<u8>)>,
    max: usize,
) -> std::thread::JoinHandle<(String, Vec<u8>)> {
    serve_dynamic(listener, move |_, _| reply, max)
}
fn serve_dynamic(
    listener: UnixListener,
    reply: impl FnOnce(&str, &[u8]) -> Option<(u16, String, Vec<u8>)> + Send + 'static,
    max: usize,
) -> std::thread::JoinHandle<(String, Vec<u8>)> {
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((value, _)) => break value,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < end => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("bounded CLI fixture accept: {e}"),
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
            .find_map(|line| line.strip_prefix("content-length: "))
            .map_or(0, |v| v.parse().unwrap());
        assert!(length <= max);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        if let Some((status, headers, bytes)) = reply(&head, &body) {
            write!(stream, "HTTP/1.1 {status} Test\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
        }
        (head, body)
    })
}

fn observed_frame(query: &ObjectObservationRequest) -> (Vec<u8>, orishu_workload::ArtifactDigest) {
    use orishu_plugin::{FiniteF64, execution::*};
    let n = |value| FiniteF64::new(value).unwrap();
    let mut objects = Vec::new();
    let object = |id, mass| ObjectState {
        id: EntityId(id),
        kinematics: Kinematics {
            position_metres: [n(1.0), n(2.0), n(3.0)],
            velocity_metres_per_second: [n(4.0), n(5.0), n(6.0)],
        },
        inertial_mass_kilograms: mass,
    };
    encode_batch(
        &[object(1, None), object(2, Some(n(2.0)))],
        &mut objects,
        BulkLimits::default(),
    )
    .unwrap();
    let mut forces = Vec::new();
    encode_batch(
        &[Force {
            id: EntityId(2),
            newtons: [n(7.0), n(8.0), n(9.0)],
        }],
        &mut forces,
        BulkLimits::default(),
    )
    .unwrap();
    let source = SnapshotSource::Committed {
        workload: query.run().workload_id(),
        run: RunDescriptor::new(query.run().clone()).digest().unwrap(),
        epoch: query.run().workload_epoch().get(),
        boundary: query.boundary(),
        time_seconds: n(query.boundary() as f64 * 0.5),
    };
    let mut bytes = Vec::new();
    let digest = encode_object_observation(
        &source,
        &objects,
        (query.boundary() != 0).then_some(forces.as_slice()),
        &mut bytes,
        object_limits(),
    )
    .unwrap();
    (bytes, digest)
}

fn window_model(socket: &Path, token: &Path) -> kagami::model::Model {
    kagami::model::Model::new(kagami::launch::LaunchOptions {
        cluster_address: orishu::client::ClusterAddress::UnixSocket(socket.into()),
        operator_token_file: Some(token.into()),
        run_recovery: kagami::run::Recovery::open(&token.parent().unwrap().join("runs")),
        ..Default::default()
    })
}
fn window_action(model: &mut kagami::model::Model, action: kagami::run::Action) {
    let _ = kagami::update::update(model, kagami::message::Message::Run(action));
    let end = Instant::now() + Duration::from_secs(15);
    while model.run.is_pending() {
        assert!(Instant::now() < end, "window effect deadline");
        std::thread::sleep(Duration::from_millis(10));
        let _ = kagami::update::update(
            model,
            kagami::message::Message::Run(kagami::run::Action::Poll),
        );
    }
}

#[test]
fn window_run_workflow_gates_authoring_and_reconciles_exact_lost_commands() {
    use kagami::{
        message::{Authoritative, ClientLocal, Message, WorkspaceIntent},
        run::Action,
    };
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let socket = dir.path().join("window.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let descriptor = descriptor(&request());
    let original_run = descriptor.identity().clone();
    let status = RunStatus::new(descriptor.clone(), 3, 1.5, RunPhase::Ready).unwrap();
    let (bytes, _) = observed_frame(&ObjectObservationRequest::new(original_run.clone(), 3));
    let server = std::thread::spawn(move || {
        serve(
            listener.try_clone().unwrap(),
            Some((200, ok(ResponseData::RetainedRun(Some(descriptor.clone()))))),
            0,
        )
        .join()
        .unwrap();
        serve(
            listener.try_clone().unwrap(),
            Some((200, ok(ResponseData::RunStatus(status)))),
            4096,
        )
        .join()
        .unwrap();
        serve_bytes(
            listener.try_clone().unwrap(),
            Some(observation_reply(OBJECT_OBSERVATION_MEDIA_TYPE, bytes)),
            4096,
        )
        .join()
        .unwrap();
        let (_, command) = serve(listener.try_clone().unwrap(), None, 4096)
            .join()
            .unwrap();
        let command: RunCommandRequest = ciborium::from_reader(&command[..]).unwrap();
        assert_eq!(command.run(), &original_run);
        assert_eq!(command.expected_boundary(), 3);
        assert_eq!(command.command(), RunCommand::Step);
        let applied = RunStatus::new(descriptor.clone(), 4, 2.0, RunPhase::Ready).unwrap();
        let receipt = control_fact(
            &command,
            RunCommandState::Finished(RunCommandOutcome::Applied {
                status: applied.clone(),
            }),
        );
        let (_, absent_lookup) = serve(
            listener.try_clone().unwrap(),
            Some((
                404,
                ApiResponse::Error {
                    code: "OperationNotFound".into(),
                    message: "not recorded".into(),
                },
            )),
            4096,
        )
        .join()
        .unwrap();
        assert_eq!(
            ciborium::from_reader::<RunCommandRequest, _>(&absent_lookup[..]).unwrap(),
            command
        );
        let (head, resend) = serve(
            listener.try_clone().unwrap(),
            Some((
                202,
                ok(ResponseData::RunCommandReceipt(control_fact(
                    &command,
                    RunCommandState::Pending,
                ))),
            )),
            4096,
        )
        .join()
        .unwrap();
        assert!(head.starts_with("post /api/v1/run-commands "));
        assert_eq!(
            ciborium::from_reader::<RunCommandRequest, _>(&resend[..]).unwrap(),
            command
        );
        let (head, lookup) = serve(
            listener.try_clone().unwrap(),
            Some((200, ok(ResponseData::RunCommandReceipt(receipt)))),
            4096,
        )
        .join()
        .unwrap();
        assert!(head.starts_with("post /api/v1/run-commands/lookup "));
        assert_eq!(
            ciborium::from_reader::<RunCommandRequest, _>(&lookup[..]).unwrap(),
            command
        );
        serve(
            listener.try_clone().unwrap(),
            Some((200, ok(ResponseData::RunStatus(applied)))),
            4096,
        )
        .join()
        .unwrap();
        let (bytes, _) = observed_frame(&ObjectObservationRequest::new(original_run, 4));
        serve_bytes(
            listener.try_clone().unwrap(),
            Some(observation_reply(OBJECT_OBSERVATION_MEDIA_TYPE, bytes)),
            4096,
        )
        .join()
        .unwrap();
        serve_dynamic(
            listener,
            move |_, body| {
                let finish: RunCommandRequest = ciborium::from_reader(body).unwrap();
                assert_eq!(finish.command(), RunCommand::Finish);
                assert_eq!(finish.expected_boundary(), 4);
                Some((
                    200,
                    "Content-Type: application/cbor\r\n".into(),
                    cbor(&ok(ResponseData::RunCommandReceipt(control_fact(
                        &finish,
                        RunCommandState::Finished(RunCommandOutcome::Applied {
                            status: RunStatus::new(descriptor, 4, 2.0, RunPhase::Finished).unwrap(),
                        }),
                    )))),
                ))
            },
            4096,
        )
        .join()
        .unwrap();
    });
    let mut model = window_model(&socket, &credential);
    let _ = kagami::update::update(&mut model, Authoritative::CreateObject.into());
    let revision = model.document.snapshot().revision();
    let view = model.document.authoring_view();
    let count = model.document.snapshot().object_count();
    window_action(&mut model, Action::Discover);
    assert!(model.is_authoring());
    assert_eq!(model.run.status().unwrap().boundary(), 3);
    window_action(&mut model, Action::Observe);
    assert!(!model.is_authoring() && model.run.attached());
    assert_eq!(model.run.objects().unwrap().count, 2);
    assert!(model.run.can_control());
    let _ = kagami::view::view(&model);
    let _ = kagami::update::update(&mut model, Authoritative::CreateObject.into());
    let _ = kagami::update::update(
        &mut model,
        Message::Local(ClientLocal::SetProjection(
            kagami_session::Projection::Orthographic,
        )),
    );
    assert_eq!(model.document.snapshot().revision(), revision);
    assert_eq!(model.document.snapshot().object_count(), count);
    assert_eq!(model.document.authoring_view(), view);
    let frame = model.run.objects().unwrap();
    assert_eq!(frame.geometry.markers.markers().len(), 2);
    assert_eq!(frame.geometry.markers.markers()[0].position(), [1., 2., 3.]);
    assert_ne!(
        frame.geometry.markers.markers()[0].color(),
        frame.geometry.markers.markers()[1].color()
    );
    let source = frame.source.clone();
    let position = frame.rows[0].object.kinematics.position_metres;
    let _ = kagami::update::update(
        &mut model,
        ClientLocal::SetScale(kagami_session::SceneScale::NANOMETRE).into(),
    );
    window_action(&mut model, Action::Poll);
    let frame = model.run.objects().unwrap();
    assert_eq!(frame.geometry.omitted_scale, 2);
    assert!(frame.geometry.markers.markers().is_empty());
    assert_eq!(frame.source, source);
    assert_eq!(frame.rows[0].object.kinematics.position_metres, position);
    let _ = kagami::update::update(
        &mut model,
        ClientLocal::SetScale(kagami_session::SceneScale::METRE).into(),
    );
    window_action(&mut model, Action::Poll);
    assert_eq!(
        model.run.objects().unwrap().geometry.markers.markers()[0].position(),
        [1., 2., 3.]
    );
    assert_eq!(model.document.authoring_view(), view);
    window_action(&mut model, Action::NumericView(true));
    let _ = kagami::view::view(&model);
    window_action(&mut model, Action::NumericView(false));
    window_action(&mut model, Action::Step);
    let original_intent = model.run.command().unwrap().clone();
    assert!(!model.run.can_control());
    window_action(&mut model, Action::Step); // No second HTTP request/new ID.
    assert_eq!(model.run.command(), Some(&original_intent));
    window_action(&mut model, Action::Reconcile);
    assert_eq!(model.run.command(), Some(&original_intent));
    window_action(&mut model, Action::ResubmitOriginal);
    assert_eq!(model.run.command(), Some(&original_intent));
    assert!(matches!(
        model.run.receipt().unwrap().state(),
        RunCommandState::Pending
    ));
    window_action(&mut model, Action::Reconcile);
    assert!(model.run.command().is_none());
    assert_eq!(model.run.status().unwrap().boundary(), 4);
    assert!(model.run.objects().is_none(), "receipt is not a frame");
    window_action(&mut model, Action::Refresh);
    assert!(model.run.can_control());
    assert_eq!(model.run.objects().unwrap().force_boundary, Some(3));
    window_action(&mut model, Action::Finish);
    assert!(!model.run.can_control());
    assert_eq!(model.run.status().unwrap().phase(), RunPhase::Finished);
    let _ = kagami::update::update(&mut model, WorkspaceIntent::EditInitialConditions.into());
    assert!(model.is_authoring() && !model.run.attached());
    assert!(model.run.objects().is_none());
    assert_eq!(model.document.snapshot().revision(), revision);
    assert_eq!(model.document.authoring_view(), view);
    assert!(model.observing_view.is_none());
    server.join().unwrap();
}

#[test]
fn closed_window_read_cannot_reattach_and_keeps_capacity_until_completion() {
    use kagami::{
        message::{Authoritative, Message},
        run::Action,
    };
    for cancel in [
        Message::Run(Action::Close),
        Message::Authoritative(Authoritative::CreateObject),
        Message::Authoritative(Authoritative::New {
            discard_unsaved: true,
        }),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let credential = dir.path().join("token");
        token(&credential);
        let socket = dir.path().join("late.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let descriptor = descriptor(&request());
        let identity = descriptor.identity().clone();
        let status = RunStatus::new(descriptor.clone(), 3, 1.5, RunPhase::Ready).unwrap();
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            serve(
                listener.try_clone().unwrap(),
                Some((200, ok(ResponseData::RetainedRun(Some(descriptor))))),
                0,
            )
            .join()
            .unwrap();
            serve(
                listener.try_clone().unwrap(),
                Some((200, ok(ResponseData::RunStatus(status)))),
                4096,
            )
            .join()
            .unwrap();
            serve_dynamic(
                listener,
                move |_, _| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(3)).unwrap();
                    Some(observation_reply(
                        OBJECT_OBSERVATION_MEDIA_TYPE,
                        observed_frame(&ObjectObservationRequest::new(identity, 3)).0,
                    ))
                },
                4096,
            )
            .join()
            .unwrap();
        });
        let mut model = window_model(&socket, &credential);
        window_action(&mut model, Action::Discover);
        let _ = kagami::update::update(&mut model, Message::Run(Action::Observe));
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
        let _ = kagami::update::update(&mut model, cancel);
        assert!(model.run.is_pending());
        let _ = kagami::update::update(&mut model, Message::Run(Action::Discover));
        assert!(model.run.is_pending());
        release.send(()).unwrap();
        window_action(&mut model, Action::Poll);
        assert!(model.is_authoring() && !model.run.attached());
        assert!(model.run.objects().is_none());
        server.join().unwrap();
    }
}

#[test]
fn window_rejects_wrong_boundary_and_status_time_without_leaving_authoring() {
    use kagami::run::Action;
    for (frame_boundary, status_time) in [(4, 1.5), (3, 2.0)] {
        let dir = tempfile::tempdir().unwrap();
        let credential = dir.path().join("token");
        token(&credential);
        let socket = dir.path().join("mismatch.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let descriptor = descriptor(&request());
        let identity = descriptor.identity().clone();
        let status = RunStatus::new(descriptor.clone(), 3, status_time, RunPhase::Ready).unwrap();
        let server = std::thread::spawn(move || {
            serve(
                listener.try_clone().unwrap(),
                Some((200, ok(ResponseData::RetainedRun(Some(descriptor))))),
                0,
            )
            .join()
            .unwrap();
            serve(
                listener.try_clone().unwrap(),
                Some((200, ok(ResponseData::RunStatus(status)))),
                4096,
            )
            .join()
            .unwrap();
            serve_bytes(
                listener,
                Some(observation_reply(
                    OBJECT_OBSERVATION_MEDIA_TYPE,
                    observed_frame(&ObjectObservationRequest::new(identity, frame_boundary)).0,
                )),
                4096,
            )
            .join()
            .unwrap();
        });
        let mut model = window_model(&socket, &credential);
        window_action(&mut model, Action::Discover);
        window_action(&mut model, Action::Observe);
        assert!(model.is_authoring());
        assert!(!model.run.attached());
        assert!(model.run.objects().is_none());
        assert!(!model.run.can_control());
        assert!(model.run.notice.contains("failed validation"));
        server.join().unwrap();
    }
}

fn field_fixture() -> (
    FieldObservationRequest,
    orishu_plugin::execution::FieldObservation,
    Vec<u8>,
    Vec<u8>,
) {
    use orishu_plugin::{ArtifactDigest, ExecutionContractId, FiniteF64, execution::*};
    let intent = FieldObservationRequest::new(
        descriptor(&request()).identity().clone(),
        3,
        "newtonian".parse().unwrap(),
    );
    let config = orishu_runtime::Buffer {
        schema: "org.test.config/v1".into(),
        value_count: 1,
        bytes: b"config".to_vec().into(),
    };
    let context =
        reference_support::context(b"fixture kernel", ExecutionContractId::Field, &config, 8);
    let field = FieldObservation::new(
        SampleSnapshot {
            source: SnapshotSource::Committed {
                workload: intent.run().workload_id(),
                run: RunDescriptor::new(intent.run().clone()).digest().unwrap(),
                epoch: intent.run().workload_epoch().get(),
                boundary: 3,
                time_seconds: FiniteF64::new(1.5).unwrap(),
            },
            state: InputIdentity::of("org.test.state/v1".parse().unwrap(), 1, b"field"),
        },
        context,
    )
    .unwrap();
    let metadata = field
        .request(42, vec![field.context.observables[0].channel.clone()])
        .unwrap();
    let points = [
        SamplePoint {
            id: 0,
            position_metres: [FiniteF64::new(1.0).unwrap(); 3],
        },
        SamplePoint {
            id: 1,
            position_metres: [FiniteF64::new(0.0).unwrap(); 3],
        },
    ];
    let limits = field_sample_limits(&field.context);
    let mut query = vec![];
    encode_sample_request(
        &metadata,
        &points,
        &mut query,
        &mut SampleScratch::default(),
        limits,
    )
    .unwrap();
    let request = SampleRequest::read(&query, &mut SampleScratch::default(), limits).unwrap();
    let mut bytes = vec![];
    let mut output = SampleOutput::new(&mut bytes, &request, &field.context, limits).unwrap();
    output
        .valid(
            0,
            0,
            &vec![FiniteF64::new(2.0).unwrap(); metadata.channels[0].components()],
            1,
        )
        .unwrap();
    output.invalid(1, 0, SampleInvalidity::Singular).unwrap();
    output.finish().unwrap();
    assert_ne!(
        ArtifactDigest::sha256_of(&query),
        ArtifactDigest::sha256_of(&bytes)
    );
    (intent, field, query, bytes)
}
fn field_intent(cmd: &mut Command, intent: &FieldObservationRequest) {
    exact_run(cmd, intent.run());
    cmd.args([
        "--field",
        intent.field().as_str(),
        "--boundary",
        &intent.boundary().to_string(),
    ]);
}
fn field_result(output: Output, code: i32, outcome: &str) -> serde_json::Value {
    versioned_result(output, code, outcome, "kagami.field-observation/v1")
}
fn encoded(response: &ApiResponse) -> Vec<u8> {
    let mut bytes = vec![];
    ciborium::into_writer(response, &mut bytes).unwrap();
    bytes
}
fn observation_reply(media: &str, bytes: Vec<u8>) -> (u16, String, Vec<u8>) {
    let digest = orishu_workload::ArtifactDigest::sha256_of(&bytes);
    (
        200,
        format!("Content-Type: {media}\r\n{OBJECT_OBSERVATION_DIGEST_HEADER}: {digest}\r\n"),
        bytes,
    )
}

#[test]
fn field_cli_discovers_exact_channels_and_samples_without_losing_validity_or_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let (intent, field, expected_query, samples) = field_fixture();
    let socket = dir.path().join("field.sock");
    let server = serve_bytes(
        UnixListener::bind(&socket).unwrap(),
        Some(observation_reply(
            FIELD_OBSERVATION_MEDIA_TYPE,
            field.to_cbor().unwrap(),
        )),
        4096,
    );
    let mut cmd = command(&socket, &credential);
    cmd.arg("field");
    field_intent(&mut cmd, &intent);
    let report = field_result(run(cmd), 0, "observed");
    assert_eq!(report["field"], serde_json::to_value(&field).unwrap());
    assert_eq!(report["request"], serde_json::to_value(&intent).unwrap());
    let (head, _) = server.join().unwrap();
    assert!(head.starts_with("post /api/v1/run/field "));
    let socket = dir.path().join("sample.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let descriptor_bytes = field.to_cbor().unwrap();
    let server = std::thread::spawn(move || {
        let first = serve_bytes(
            listener.try_clone().unwrap(),
            Some(observation_reply(
                FIELD_OBSERVATION_MEDIA_TYPE,
                descriptor_bytes,
            )),
            4096,
        )
        .join()
        .unwrap();
        let second = serve_bytes(
            listener,
            Some(observation_reply(FIELD_SAMPLE_RESPONSE_MEDIA_TYPE, samples)),
            MAX_FIELD_SAMPLE_BYTES + 4100,
        )
        .join()
        .unwrap();
        (first, second)
    });
    let mut cmd = command(&socket, &credential);
    cmd.args([
        "sample",
        "--request-id",
        "42",
        "--channel",
        field.context.observables[0].slot.as_str(),
        "--point",
        "1,1,1",
        "--point",
        "0,0,0",
    ]);
    field_intent(&mut cmd, &intent);
    let report = field_result(run(cmd), 0, "observed");
    assert_eq!(report["field"], serde_json::to_value(&field).unwrap());
    assert_eq!(
        report["request"]["field"],
        serde_json::to_value(&intent).unwrap()
    );
    assert_eq!(report["samples"]["metadata"]["requestId"], 42);
    assert_eq!(report["samples"]["coverage"], "complete-request");
    assert_eq!(report["samples"]["readings"][0]["point"]["id"], 0);
    assert_eq!(
        report["samples"]["readings"][0]["channels"][0]["valid"],
        true
    );
    assert_eq!(
        report["samples"]["readings"][0]["channels"][0]["qualityFlags"],
        1
    );
    assert_eq!(
        report["samples"]["readings"][0]["channels"][0]["values"][0],
        2.0
    );
    let invalid = &report["samples"]["readings"][1]["channels"][0];
    assert_eq!(invalid["reason"], "Singular");
    assert_eq!(invalid["valid"], false);
    assert!(invalid.get("values").is_none() && invalid.get("qualityFlags").is_none());
    assert!(report.get("status").is_none() && report.get("receipt").is_none());
    let ((_, _), (head, body)) = server.join().unwrap();
    assert!(head.starts_with("post /api/v1/run/samples "));
    let size = u32::from_be_bytes(body[..4].try_into().unwrap()) as usize;
    assert_eq!(
        ciborium::from_reader::<FieldObservationRequest, _>(&body[4..4 + size]).unwrap(),
        intent
    );
    assert_eq!(&body[4 + size..], expected_query);
}

#[test]
fn sample_cli_preserves_read_intent_on_lost_stale_and_corrupt_replies() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let (intent, field, _, samples) = field_fixture();
    for case in 0..5 {
        let socket = dir.path().join(format!("error-{case}.sock"));
        let listener = UnixListener::bind(&socket).unwrap();
        let descriptor = field.to_cbor().unwrap();
        let reply = match case {
            0 => None,
            1 => Some((
                409,
                "Content-Type: application/cbor\r\n".into(),
                encoded(&ApiResponse::Error {
                    code: "StaleBoundary".into(),
                    message: "stale".into(),
                }),
            )),
            2 => {
                let mut bad = samples.clone();
                bad[4] ^= 1;
                Some(observation_reply(FIELD_SAMPLE_RESPONSE_MEDIA_TYPE, bad))
            }
            3 => Some((
                503,
                "Content-Type: application/cbor\r\n".into(),
                encoded(&ApiResponse::Error {
                    code: "ObserverBusy".into(),
                    message: "busy".into(),
                }),
            )),
            _ => Some((
                400,
                "Content-Type: application/cbor\r\n".into(),
                encoded(&ApiResponse::Error {
                    code: "Reflected".into(),
                    message: "a".repeat(64),
                }),
            )),
        };
        let server = std::thread::spawn(move || {
            serve_bytes(
                listener.try_clone().unwrap(),
                Some(observation_reply(FIELD_OBSERVATION_MEDIA_TYPE, descriptor)),
                4096,
            )
            .join()
            .unwrap();
            serve_bytes(listener, reply, MAX_FIELD_SAMPLE_BYTES + 4100)
                .join()
                .unwrap()
        });
        let mut cmd = command(&socket, &credential);
        cmd.args([
            "sample",
            "--request-id",
            "42",
            "--channel",
            field.context.observables[0].slot.as_str(),
            "--point",
            "1,1,1",
            "--point",
            "0,0,0",
        ]);
        field_intent(&mut cmd, &intent);
        let report = field_result(run(cmd), 1, "error");
        assert_eq!(
            report["request"]["field"],
            serde_json::to_value(&intent).unwrap()
        );
        assert_eq!(report["error"]["submissionOutcomeUnknown"], false);
        assert!(report.get("samples").is_none());
        if case == 4 {
            assert_eq!(report["error"]["code"], "credential_reflection");
            assert!(report.get("field").is_none());
        }
        server.join().unwrap();
    }
}

#[test]
fn field_cli_refuses_missing_channels_and_secret_descriptors_without_sampling() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("token");
    token(&credential);
    let (intent, field, _, _) = field_fixture();
    for secret in [false, true] {
        let socket = dir.path().join(format!("field-{secret}.sock"));
        let mut value = field.clone();
        if secret {
            value.context.kernel = format!("sha256:{}", "a".repeat(64)).parse().unwrap();
        }
        let server = serve_bytes(
            UnixListener::bind(&socket).unwrap(),
            Some(observation_reply(
                FIELD_OBSERVATION_MEDIA_TYPE,
                value.to_cbor().unwrap(),
            )),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        if secret {
            cmd.arg("field");
        } else {
            cmd.args([
                "sample",
                "--request-id",
                "42",
                "--channel",
                "missing",
                "--point",
                "1,1,1",
            ]);
        }
        field_intent(&mut cmd, &intent);
        let report = field_result(run(cmd), 1, "error");
        assert_eq!(
            report["error"]["code"],
            if secret {
                "credential_reflection"
            } else {
                "channel_unavailable"
            }
        );
        assert!(report.get("samples").is_none());
        server.join().unwrap();
    }
    for duplicate in [true, false] {
        let mut cmd = command(&dir.path().join("absent.sock"), &credential);
        cmd.args([
            "sample",
            "--request-id",
            "42",
            "--channel",
            "a",
            "--point",
            "0,0,0",
        ]);
        if duplicate {
            cmd.args(["--channel", "a"]);
        } else {
            for _ in 0..4096 {
                cmd.args(["--point", "0,0,0"]);
            }
        }
        field_intent(&mut cmd, &intent);
        let report = field_result(run(cmd), 1, "error");
        assert_eq!(report["error"]["code"], "sample_limits");
    }
}

#[test]
fn object_commands_report_numeric_state_and_force_phase_without_artifact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    for boundary in [0, 3] {
        let socket = dir.path().join(format!("objects-{boundary}.sock"));
        let query =
            ObjectObservationRequest::new(descriptor(&request()).identity().clone(), boundary);
        let (bytes, digest) = observed_frame(&query);
        let server = serve_bytes(
            UnixListener::bind(&socket).unwrap(),
            Some((
                200,
                format!(
                    "Content-Type: {OBJECT_OBSERVATION_MEDIA_TYPE}\r\n{OBJECT_OBSERVATION_DIGEST_HEADER}: {digest}\r\n"
                ),
                bytes,
            )),
            MAX_OBJECT_REQUEST_BYTES,
        );
        let mut cmd = command(&socket, &credential);
        cmd.args(["objects", "--boundary", &boundary.to_string()]);
        exact_run(&mut cmd, query.run());
        let report = observation_result(run(cmd), 0, "observed");
        assert_eq!(report["request"], serde_json::to_value(&query).unwrap());
        let view = &report["observation"];
        assert_eq!(view["digest"], digest.to_string());
        assert_eq!(view["source"]["boundary"], boundary);
        assert_eq!(view["coverage"], "complete-numeric-objects");
        assert_eq!(view["objects"].as_array().unwrap().len(), 2);
        assert_eq!(
            view["objects"][0]["kinematics"]["position_metres"],
            serde_json::json!([1.0, 2.0, 3.0])
        );
        assert!(view["objects"][0]["inertial_mass_kilograms"].is_null());
        assert_eq!(view["objects"][1]["inertial_mass_kilograms"], 2.0);
        if boundary == 0 {
            assert!(view["forces"].is_null());
            assert!(view["forceEvaluationBoundary"].is_null());
        } else {
            assert_eq!(
                view["forces"][0]["newtons"],
                serde_json::json!([7.0, 8.0, 9.0])
            );
            assert_eq!(view["forceEvaluationBoundary"], boundary - 1);
        }
        assert!(report.get("receipt").is_none());
        assert!(report.get("status").is_none());
        assert!(view.get("bytes").is_none());
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("post /api/v1/run/objects http/1.1"));
        assert_eq!(
            ciborium::from_reader::<ObjectObservationRequest, _>(&body[..]).unwrap(),
            query
        );
    }
}

#[test]
fn object_errors_are_read_only_preserve_intent_and_redact_reflected_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    let query = ObjectObservationRequest::new(descriptor(&request()).identity().clone(), 1);
    for (index, (status, code)) in [
        (404, "RunUnavailable"),
        (404, "NotFound"),
        (409, "StaleBoundary"),
        (503, "ObserverBusy"),
        (503, "secret"),
        (0, "transport"),
    ]
    .into_iter()
    .enumerate()
    {
        let socket = dir.path().join(format!("error-{index}.sock"));
        let reply = (status != 0).then(|| {
            (
                status,
                ApiResponse::Error {
                    code: code.into(),
                    message: if code == "secret" {
                        "a".repeat(64)
                    } else {
                        code.into()
                    },
                },
            )
        });
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            reply,
            MAX_OBJECT_REQUEST_BYTES,
        );
        let mut cmd = command(&socket, &credential);
        cmd.args(["objects", "--boundary", "1"]);
        exact_run(&mut cmd, query.run());
        let absent = code == "RunUnavailable";
        let report = observation_result(
            run(cmd),
            if absent { 13 } else { 1 },
            if absent { "empty" } else { "error" },
        );
        assert_eq!(report["request"], serde_json::to_value(&query).unwrap());
        assert!(report.get("observation").is_none());
        if !absent {
            assert_eq!(report["error"]["submissionOutcomeUnknown"], false);
            assert_eq!(
                report["error"]["code"],
                if code == "secret" {
                    "credential_reflection"
                } else {
                    code
                }
            );
        }
        server.join().unwrap();
    }
}

#[test]
fn object_cli_never_reports_corrupt_or_wrong_boundary_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    let query = ObjectObservationRequest::new(descriptor(&request()).identity().clone(), 1);
    for wrong_boundary in [false, true] {
        let fixture_query =
            ObjectObservationRequest::new(query.run().clone(), if wrong_boundary { 2 } else { 1 });
        let (mut bytes, digest) = observed_frame(&fixture_query);
        if !wrong_boundary {
            *bytes.last_mut().unwrap() ^= 1;
        }
        let socket = dir.path().join(format!("invalid-{wrong_boundary}.sock"));
        let server = serve_bytes(
            UnixListener::bind(&socket).unwrap(),
            Some((
                200,
                format!(
                    "Content-Type: {OBJECT_OBSERVATION_MEDIA_TYPE}\r\n{OBJECT_OBSERVATION_DIGEST_HEADER}: {digest}\r\n"
                ),
                bytes,
            )),
            MAX_OBJECT_REQUEST_BYTES,
        );
        let mut cmd = command(&socket, &credential);
        cmd.args(["objects", "--boundary", "1"]);
        exact_run(&mut cmd, query.run());
        let report = observation_result(run(cmd), 1, "error");
        assert_eq!(report["error"]["code"], "protocol");
        assert_eq!(report["error"]["submissionOutcomeUnknown"], false);
        assert_eq!(report["request"], serde_json::to_value(&query).unwrap());
        assert!(report.get("observation").is_none());
        server.join().unwrap();
    }
}
fn portable(path: &Path) -> LoadRequest {
    let fixture = fixture::Fixture::new();
    let compiled = fixture.compile();
    let blobs = fixture.export(&compiled);
    let bytes = orishu_plugin::workload::bundle::pack(
        compiled.manifest_bytes(),
        &fixture::borrowed(&blobs),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    std::fs::write(path, bytes).unwrap();
    LoadRequest::new(
        "upload-1".parse().unwrap(),
        "formation-a".parse().unwrap(),
        compiled.verified().root(),
    )
}

#[test]
fn local_refusals_precede_network_and_do_not_expose_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mut cmd = command(&socket, &credential);
    cmd.arg("current");
    let report = result(run(cmd), 1, "error");
    assert_eq!(report["error"]["stage"], "credential");
    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut cmd = command(&socket, &credential);
    cmd.arg("receipt");
    let secret_intent = LoadRequest::new(
        "a".repeat(64).parse().unwrap(),
        request().formation_id().clone(),
        request().workload_id(),
    );
    intent(&mut cmd, &secret_intent);
    let report = result(run(cmd), 1, "error");
    assert_eq!(report["error"]["code"], "credential_in_intent");
    assert!(report.get("request").is_none());
    let bundle = dir.path().join("run.orishu");
    std::fs::write(&bundle, b"not an archive").unwrap();
    let mut cmd = command(&socket, &credential);
    cmd.arg("submit").arg(&bundle);
    intent(&mut cmd, &request());
    let report = result(run(cmd), 1, "error");
    assert_eq!(report["error"]["code"], "invalid_bundle");
    assert_eq!(report["error"]["submissionOutcomeUnknown"], false);
    portable(&bundle);
    let mut cmd = command(&socket, &credential);
    cmd.arg("submit").arg(&bundle);
    intent(&mut cmd, &request());
    let report = result(run(cmd), 1, "error");
    assert_eq!(report["error"]["code"], "root_mismatch");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn actual_cli_submits_exact_portable_bytes_and_preserves_intent_on_lost_reply() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    let bundle = dir.path().join("run.orishu");
    let request = portable(&bundle);
    let bytes = std::fs::read(&bundle).unwrap();
    for lost in [false, true] {
        let socket = dir.path().join(if lost { "lost.sock" } else { "api.sock" });
        let reply = (!lost).then(|| {
            (
                202,
                ok(ResponseData::RunLoadReceipt(fact(
                    &request,
                    LoadState::Pending,
                ))),
            )
        });
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            reply,
            bytes.len() + 4096 + 4,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg("submit").arg(&bundle);
        intent(&mut cmd, &request);
        let report = result(
            run(cmd),
            if lost { 1 } else { 10 },
            if lost { "error" } else { "pending" },
        );
        assert_eq!(report["request"], serde_json::to_value(&request).unwrap());
        if lost {
            assert_eq!(report["error"]["submissionOutcomeUnknown"], true);
        }
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("post /api/v1/run-loads http/1.1"));
        assert!(head.contains("content-type: application/vnd.orishu.run-load.v1\r\n"));
        let size = u32::from_be_bytes(body[..4].try_into().unwrap()) as usize;
        assert_eq!(
            ciborium::from_reader::<LoadRequest, _>(&body[4..4 + size]).unwrap(),
            request
        );
        assert_eq!(&body[4 + size..], bytes);
    }
}

#[test]
fn actual_cli_reports_receipt_states_and_live_discovery_separately() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    let request = request();
    let cases = [
        (LoadState::Pending, 10, "pending"),
        (
            LoadState::Finished(LoadOutcome::Refused {
                reason: LoadRefusal::FormationUnavailable,
            }),
            11,
            "refused",
        ),
        (
            LoadState::Finished(LoadOutcome::Indeterminate),
            12,
            "indeterminate",
        ),
        (
            LoadState::Finished(LoadOutcome::Accepted {
                descriptor: descriptor(&request),
            }),
            0,
            "accepted",
        ),
    ];
    for (state, code, outcome) in cases {
        let socket = dir.path().join(format!("{outcome}.sock"));
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((200, ok(ResponseData::RunLoadReceipt(fact(&request, state))))),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg("receipt");
        intent(&mut cmd, &request);
        result(run(cmd), code, outcome);
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("post /api/v1/run-loads/lookup http/1.1"));
        assert_eq!(
            ciborium::from_reader::<LoadRequest, _>(&body[..]).unwrap(),
            request
        );
    }
    for run_state in [None, Some(descriptor(&request))] {
        let socket = dir.path().join(if run_state.is_some() {
            "live.sock"
        } else {
            "empty.sock"
        });
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((200, ok(ResponseData::RetainedRun(run_state.clone())))),
            0,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg("current");
        result(
            run(cmd),
            if run_state.is_some() { 0 } else { 13 },
            if run_state.is_some() { "live" } else { "empty" },
        );
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("get /api/v1/run http/1.1"));
        assert!(body.is_empty());
    }
}

#[test]
fn absent_history_mismatched_receipts_and_secret_reflection_are_not_success() {
    let dir = tempfile::tempdir().unwrap();
    let credential = dir.path().join("operator.token");
    token(&credential);
    let request = request();
    let other = LoadRequest::new(
        "other-operation".parse().unwrap(),
        request.formation_id().clone(),
        request.workload_id(),
    );
    for (index, (status, reply, code, outcome, error)) in [
        (
            404,
            ApiResponse::Error {
                code: "OperationNotFound".into(),
                message: "missing".into(),
            },
            13,
            "notFound",
            None,
        ),
        (
            404,
            ApiResponse::Error {
                code: "NotFound".into(),
                message: "missing API".into(),
            },
            1,
            "error",
            Some("NotFound"),
        ),
        (
            200,
            ok(ResponseData::RunLoadReceipt(fact(
                &other,
                LoadState::Pending,
            ))),
            1,
            "error",
            Some("protocol"),
        ),
        (
            401,
            ApiResponse::Error {
                code: "Unauthorized".into(),
                message: "a".repeat(64),
            },
            1,
            "error",
            Some("credential_reflection"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let socket = dir.path().join(format!("error-{index}.sock"));
        let server = serve(
            UnixListener::bind(&socket).unwrap(),
            Some((status, reply)),
            4096,
        );
        let mut cmd = command(&socket, &credential);
        cmd.arg("receipt");
        intent(&mut cmd, &request);
        let report = result(run(cmd), code, outcome);
        assert_eq!(report["request"], serde_json::to_value(&request).unwrap());
        if let Some(error) = error {
            assert_eq!(report["error"]["code"], error);
        }
        server.join().unwrap();
    }
}

#[test]
#[ignore = "requires built native worker; run make test-kagami-workload"]
fn actual_worker_accepts_kagami_cli_upload_and_preserves_history_after_restart() {
    use orishu::client::{
        ClientApi, ClusterAddress, ClusterApi, MembershipApi, credential_file,
        http_client::{HttClientOptions, HttpClusterClient},
    };
    struct Worker(std::process::Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn start(binary: &std::ffi::OsStr, state: &Path, socket: &Path) -> Worker {
        let mut command = Command::new(binary);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ORISHU_") {
                command.env_remove(name);
            }
        }
        Worker(
            command
                .arg("--state-dir")
                .arg(state)
                .arg("--listen.clients")
                .arg(socket)
                .args(["--scientific.enabled", "true"])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        )
    }
    let binary =
        std::env::var_os("ORISHU_TEST_WORKER").expect("explicit built worker path required");
    assert!(Path::new(&binary).is_absolute());
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.path().join("worker.sock");
    let state = dir.path().join("state");
    let bundle = dir.path().join("run.orishu");
    let root = portable(&bundle).workload_id();
    let mut worker = start(&binary, &state, &socket);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let observer = HttpClusterClient::new(
        ClusterAddress::UnixSocket(socket.clone()),
        Default::default(),
    )
    .unwrap();
    let wait_ready = |worker: &mut Worker| {
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    assert!(
                        worker.0.try_wait().unwrap().is_none(),
                        "worker exited before ready"
                    );
                    if let Ok(value) = observer.cluster().summary().await {
                        break value;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap()
        })
    };
    let initial = wait_ready(&mut worker);
    let credential = state.join("operator.token");
    let request = LoadRequest::new(
        "cli-real".parse().unwrap(),
        initial.formation_id.clone(),
        root,
    );
    let operator = HttpClusterClient::new(
        ClusterAddress::UnixSocket(socket.clone()),
        HttClientOptions {
            credentials: Some(credential_file::load(&credential).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    // Explicit operator action, never a side effect of Kagami submission.
    executor
        .block_on(
            operator
                .membership()
                .set_lock(&orishu::model::cluster::LockRequest {
                    schema_version: 1,
                    operation_id: "lock-cli-fixture".parse().unwrap(),
                    formation_id: initial.formation_id.clone(),
                    locked: true,
                }),
        )
        .unwrap();
    let mut cmd = command(&socket, &credential);
    cmd.arg("submit").arg(&bundle);
    intent(&mut cmd, &request);
    let output = run(cmd);
    assert!([Some(0), Some(10)].contains(&output.status.code()));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["request"], serde_json::to_value(&request).unwrap());
    let end = Instant::now() + Duration::from_secs(80);
    let accepted = loop {
        let mut cmd = command(&socket, &credential);
        cmd.arg("receipt");
        intent(&mut cmd, &request);
        let output = run(cmd);
        if output.status.code() == Some(10) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        break result(output, 0, "accepted");
    };
    let fact: LoadReceipt = serde_json::from_value(accepted["receipt"].clone()).unwrap();
    let LoadState::Finished(LoadOutcome::Accepted { descriptor }) = fact.state() else {
        panic!("accepted fixture");
    };
    let mut cmd = command(&socket, &credential);
    cmd.arg("current");
    let live = result(run(cmd), 0, "live");
    assert_eq!(live["run"], serde_json::to_value(descriptor).unwrap());
    let read_status = || {
        let mut cmd = command(&socket, &credential);
        cmd.arg("status");
        exact_run(&mut cmd, descriptor.identity());
        run(cmd)
    };
    let initial_status = control_result(read_status(), 0, "live");
    assert_eq!(initial_status["status"]["boundary"], 0);
    assert_eq!(initial_status["status"]["phase"], "ready");
    let read_objects = |boundary: u64| {
        let mut cmd = command(&socket, &credential);
        cmd.args(["objects", "--boundary", &boundary.to_string()]);
        exact_run(&mut cmd, descriptor.identity());
        run(cmd)
    };
    let initial_objects = observation_result(read_objects(0), 0, "observed");
    let read_field = |boundary: u64| {
        let mut cmd = command(&socket, &credential);
        cmd.args([
            "field",
            "--field",
            "newtonian",
            "--boundary",
            &boundary.to_string(),
        ]);
        exact_run(&mut cmd, descriptor.identity());
        run(cmd)
    };
    let initial_field = field_result(read_field(0), 0, "observed");
    let channels = [2, 0, 1].map(|i| {
        initial_field["field"]["context"]["observables"][i]["slot"]
            .as_str()
            .unwrap()
    });
    let sample_field = |boundary: u64| {
        let mut cmd = command(&socket, &credential);
        cmd.args([
            "sample",
            "--field",
            "newtonian",
            "--boundary",
            &boundary.to_string(),
            "--request-id",
            "42",
            "--point",
            "1,1,1",
            "--point",
            "0,0,0",
        ]);
        for channel in channels {
            cmd.args(["--channel", channel]);
        }
        exact_run(&mut cmd, descriptor.identity());
        run(cmd)
    };
    let initial_samples = field_result(sample_field(0), 0, "observed");
    assert_eq!(initial_samples["field"], initial_field["field"]);
    assert!(initial_objects["observation"]["forces"].is_null());
    assert_eq!(
        initial_objects["observation"]["source"]["workload"],
        request.workload_id().to_string()
    );
    let execute_control = |name: &str, request: &RunCommandRequest| {
        let mut cmd = command(&socket, &credential);
        cmd.arg(name);
        control_intent(&mut cmd, request);
        if name == "command-receipt" {
            cmd.args([
                "--action",
                match request.command() {
                    RunCommand::Step => "step",
                    RunCommand::Finish => "finish",
                },
            ]);
        }
        run(cmd)
    };
    let step = RunCommandRequest::new(
        "cli-step".parse().unwrap(),
        descriptor.identity().clone(),
        0,
        RunCommand::Step,
    )
    .unwrap();
    let stepped = control_result(execute_control("step", &step), 0, "applied");
    assert_eq!(stepped["request"], serde_json::to_value(&step).unwrap());
    assert_eq!(
        stepped["receipt"]["state"]["outcome"]["status"]["boundary"],
        1
    );
    assert_eq!(
        stepped["receipt"]["state"]["outcome"]["status"]["timeSeconds"],
        0.5
    );
    // Retrying and reconciliation preserve attribution without stepping twice.
    assert_eq!(
        control_result(execute_control("step", &step), 0, "applied")["receipt"],
        stepped["receipt"]
    );
    assert_eq!(
        control_result(execute_control("command-receipt", &step), 0, "applied")["receipt"],
        stepped["receipt"]
    );
    let after_step = control_result(read_status(), 0, "live");
    assert_eq!(after_step["status"]["boundary"], 1);
    let stepped_objects = observation_result(read_objects(1), 0, "observed");
    let stepped_samples = field_result(sample_field(1), 0, "observed");
    // The production window update path observes the same real worker without
    // adopting computed data or pretending its unrelated open draft is this run.
    let mut window = window_model(&socket, &credential);
    let revision = window.document.snapshot().revision();
    window_action(&mut window, kagami::run::Action::Discover);
    window_action(&mut window, kagami::run::Action::Observe);
    assert!(window.run.attached() && !window.is_authoring());
    assert_eq!(window.run.status().unwrap().boundary(), 1);
    assert_eq!(window.run.objects().unwrap().force_boundary, Some(0));
    assert_eq!(
        window.run.objects().unwrap().count,
        stepped_objects["observation"]["objects"]
            .as_array()
            .unwrap()
            .len()
    );
    let _ = kagami::update::update(
        &mut window,
        kagami::message::Authoritative::CreateObject.into(),
    );
    assert_eq!(window.document.snapshot().revision(), revision);
    let _ = kagami::update::update(
        &mut window,
        kagami::message::WorkspaceIntent::EditInitialConditions.into(),
    );
    assert!(window.is_authoring() && !window.run.attached());
    assert_eq!(
        control_result(read_status(), 0, "live")["status"],
        after_step["status"]
    );
    assert_ne!(
        stepped_samples["field"]["snapshot"]["state"],
        initial_field["field"]["snapshot"]["state"]
    );
    assert_eq!(
        stepped_samples["samples"]["metadata"]["snapshot"]["source"]["boundary"],
        1
    );
    let readings = &stepped_samples["samples"]["readings"];
    assert_eq!(
        stepped_samples["request"]["channels"],
        serde_json::json!(["potential", "acceleration", "jacobian"])
    );
    for (channel, size) in [1, 3, 9].into_iter().enumerate() {
        assert_eq!(
            readings[0]["channels"][channel]["values"]
                .as_array()
                .unwrap()
                .len(),
            size
        );
        assert_eq!(readings[0]["channels"][channel]["qualityFlags"], 1);
        assert_eq!(readings[1]["channels"][channel]["reason"], "Singular");
    }
    assert_eq!(readings[0]["channels"][0]["valid"], true);
    assert!(
        readings[0]["channels"][0]["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_f64().unwrap() != 0.0)
    );
    assert_eq!(readings[1]["channels"][0]["reason"], "Singular");
    assert_eq!(
        field_result(sample_field(0), 1, "error")["error"]["code"],
        "StaleBoundary"
    );
    assert_eq!(
        control_result(read_status(), 0, "live")["status"],
        after_step["status"]
    );
    assert_eq!(stepped_objects["observation"]["source"]["boundary"], 1);
    assert_eq!(stepped_objects["observation"]["source"]["timeSeconds"], 0.5);
    assert_eq!(stepped_objects["observation"]["forceEvaluationBoundary"], 0);
    assert!(stepped_objects["observation"]["forces"].is_array());
    assert_ne!(
        stepped_objects["observation"]["objects"],
        initial_objects["observation"]["objects"]
    );
    let stale_observation = observation_result(read_objects(0), 1, "error");
    assert_eq!(stale_observation["error"]["code"], "StaleBoundary");
    assert_eq!(
        stale_observation["error"]["submissionOutcomeUnknown"],
        false
    );
    let conflict = RunCommandRequest::new(
        step.operation_id().clone(),
        step.run().clone(),
        1,
        RunCommand::Step,
    )
    .unwrap();
    let report = control_result(execute_control("step", &conflict), 1, "error");
    assert_eq!(report["error"]["code"], "OperationConflict");
    let stale = RunCommandRequest::new(
        "cli-stale".parse().unwrap(),
        step.run().clone(),
        0,
        RunCommand::Step,
    )
    .unwrap();
    let report = control_result(execute_control("step", &stale), 11, "refused");
    assert_eq!(
        report["receipt"]["state"]["outcome"]["reason"]["code"],
        "staleBoundary"
    );
    let finish = RunCommandRequest::new(
        "cli-finish".parse().unwrap(),
        step.run().clone(),
        1,
        RunCommand::Finish,
    )
    .unwrap();
    let finished = control_result(execute_control("finish", &finish), 0, "applied");
    assert_eq!(
        field_result(sample_field(1), 0, "observed")["samples"],
        stepped_samples["samples"]
    );
    assert_eq!(
        finished["receipt"]["state"]["outcome"]["status"]["phase"],
        "finished"
    );
    assert_eq!(
        control_result(execute_control("command-receipt", &finish), 0, "applied")["receipt"],
        finished["receipt"]
    );
    assert_eq!(
        control_result(read_status(), 0, "live")["status"]["phase"],
        "finished"
    );
    let late = RunCommandRequest::new(
        "cli-late".parse().unwrap(),
        step.run().clone(),
        1,
        RunCommand::Step,
    )
    .unwrap();
    control_result(execute_control("step", &late), 11, "refused");
    assert_eq!(
        observation_result(read_objects(1), 0, "observed")["observation"],
        stepped_objects["observation"]
    );
    let before = std::fs::read(&bundle).unwrap();
    drop(worker);
    let mut worker = start(&binary, &state, &socket);
    assert_ne!(wait_ready(&mut worker).formation_id, initial.formation_id);
    let mut cmd = command(&socket, &credential);
    cmd.arg("receipt");
    intent(&mut cmd, &request);
    assert_eq!(
        result(run(cmd), 0, "accepted")["receipt"],
        accepted["receipt"]
    );
    let mut cmd = command(&socket, &credential);
    cmd.arg("current");
    result(run(cmd), 13, "empty");
    control_result(read_status(), 13, "empty");
    observation_result(read_objects(1), 13, "empty");
    field_result(read_field(1), 13, "empty");
    field_result(sample_field(1), 13, "empty");
    assert_eq!(
        control_result(execute_control("command-receipt", &step), 0, "applied")["receipt"],
        stepped["receipt"]
    );
    assert_eq!(
        control_result(execute_control("step", &step), 0, "applied")["receipt"],
        stepped["receipt"]
    );
    assert_eq!(
        std::fs::read(&bundle).unwrap(),
        before,
        "submission does not rewrite input"
    );
}
