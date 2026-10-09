use super::*;
use orishu::model::{node::NodeId, run::*};

fn formation() -> FormationId {
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
        incarnation: uuid::Uuid::from_u128(0x1234_5678_9abc_4def_8123_4567_89ab_cdef),
        revision: 7,
    }
}
fn load_request(operation: &str) -> LoadRequest {
    LoadRequest::new(operation.parse().unwrap(), formation(), workload())
}
fn node() -> NodeId {
    NodeId::new("node-7f3a").unwrap()
}
fn identity() -> RunIdentity {
    RunIdentity::new(formation(), workload(), WorkloadEpoch::new(1))
}
fn load_receipt(request: &LoadRequest, state: LoadState) -> LoadReceipt {
    LoadReceipt::new(request.clone(), node(), state).unwrap()
}
fn accepted() -> LoadState {
    LoadState::Finished(LoadOutcome::Accepted {
        descriptor: RunDescriptor::new(identity()),
    })
}
fn refused() -> LoadState {
    LoadState::Finished(LoadOutcome::Refused {
        reason: LoadRefusal::InvalidWorkload,
    })
}
fn command_request(operation: &str) -> RunCommandRequest {
    RunCommandRequest::new(operation.parse().unwrap(), identity(), 3, RunCommand::Step).unwrap()
}
fn command_receipt(request: &RunCommandRequest, state: RunCommandState) -> RunCommandReceipt {
    RunCommandReceipt::new(request.clone(), node(), state).unwrap()
}
fn applied(request: &RunCommandRequest) -> RunCommandState {
    let status = RunStatus::new(
        RunDescriptor::new(request.run().clone()),
        4,
        2.5,
        RunPhase::Ready,
    )
    .unwrap();
    RunCommandState::Finished(RunCommandOutcome::Applied { status })
}
fn indeterminate() -> RunCommandState {
    RunCommandState::Finished(RunCommandOutcome::Indeterminate)
}
fn both() -> Ledger {
    let load = load_request("kagami-load-1");
    let command = command_request("kagami-1");
    Ledger::default()
        .begin_load(target(), load.clone(), source())
        .and_then(|l| l.record_load(Some(load_receipt(&load, LoadState::Pending))))
        .and_then(|l| l.begin_command(target(), command.clone()))
        .and_then(|l| l.record_command(Some(command_receipt(&command, indeterminate()))))
        .unwrap()
}
fn json(ledger: &Ledger) -> serde_json::Value {
    serde_json::from_slice(&ledger.encode().unwrap()).unwrap()
}
fn decode_value(value: &serde_json::Value) -> Result<Ledger, DecodeError> {
    Ledger::decode(&serde_json::to_vec(value).unwrap())
}

#[test]
fn every_recorded_state_round_trips_to_identical_bytes() {
    let load = load_request("kagami-load-1");
    let command = command_request("kagami-1");
    let empty = Ledger::default();
    let begun = empty.begin_load(target(), load.clone(), source()).unwrap();
    let absent = begun.record_load(None).unwrap();
    let accepted = begun
        .record_load(Some(load_receipt(&load, accepted())))
        .unwrap();
    let commanded = accepted.begin_command(target(), command.clone()).unwrap();
    let pending = commanded
        .record_command(Some(command_receipt(&command, RunCommandState::Pending)))
        .unwrap();
    for ledger in [empty, begun, absent, accepted, commanded, pending, both()] {
        let bytes = ledger.encode().unwrap();
        let decoded = Ledger::decode(&bytes).unwrap();
        assert_eq!(decoded, ledger);
        assert_eq!(decoded.encode().unwrap(), bytes);
    }
    let restored = Ledger::decode(&both().encode().unwrap()).unwrap();
    assert_eq!(restored.load().unwrap().source(), source());
    assert_eq!(restored.load().unwrap().target(), &target());
}

#[test]
fn transitions_refuse_overlap_foreign_receipts_and_unresolved_clears() {
    let load = load_request("kagami-load-1");
    let ledger = Ledger::default()
        .begin_load(target(), load.clone(), source())
        .unwrap();
    assert_eq!(
        ledger.begin_load(target(), load_request("kagami-load-2"), source()),
        Err(IntentError::LoadRetained)
    );
    let elsewhere =
        Target::new(target().address().clone(), "formation-b".parse().unwrap()).unwrap();
    assert_eq!(
        Ledger::default().begin_load(elsewhere.clone(), load.clone(), source()),
        Err(IntentError::TargetMismatch)
    );
    assert_eq!(
        Ledger::default().begin_command(elsewhere, command_request("kagami-1")),
        Err(IntentError::TargetMismatch)
    );
    let foreign = load_receipt(&load_request("kagami-load-2"), accepted());
    assert_eq!(
        ledger.record_load(Some(foreign)),
        Err(IntentError::ForeignReceipt)
    );
    for state in [
        None,
        Some(LoadState::Pending),
        Some(LoadState::Finished(LoadOutcome::Indeterminate)),
    ] {
        let unresolved = ledger
            .record_load(state.map(|state| load_receipt(&load, state)))
            .unwrap();
        assert!(!unresolved.load().unwrap().is_final());
        assert_eq!(unresolved.clear_load(), Err(IntentError::Unresolved));
    }
    let rejected = ledger
        .record_load(Some(load_receipt(&load, refused())))
        .unwrap();
    assert!(rejected.load().unwrap().is_final());
    // A final receipt never changes, but repeating it is harmless.
    assert_eq!(
        rejected.record_load(Some(load_receipt(&load, accepted()))),
        Err(IntentError::ConflictingReceipt)
    );
    assert_eq!(
        rejected.record_load(None),
        Err(IntentError::ConflictingReceipt)
    );
    assert_eq!(
        rejected.record_load(Some(load_receipt(&load, refused()))),
        Ok(rejected.clone())
    );
    assert!(rejected.clear_load().unwrap().is_empty());
    assert_eq!(Ledger::default().clear_load(), Err(IntentError::NoLoad));
    assert_eq!(
        Ledger::default().record_load(None),
        Err(IntentError::NoLoad)
    );
    assert_eq!(
        Ledger::default().record_command(None),
        Err(IntentError::NoCommand)
    );
}

#[test]
fn a_final_command_outcome_removes_only_the_command() {
    let ledger = both();
    let command = ledger.command().unwrap().request().clone();
    assert_eq!(
        ledger.begin_command(target(), command_request("kagami-2")),
        Err(IntentError::CommandRetained)
    );
    assert_eq!(
        ledger.record_command(Some(command_receipt(
            &command_request("kagami-2"),
            RunCommandState::Pending
        ))),
        Err(IntentError::ForeignReceipt)
    );
    let refusal = RunCommandState::Finished(RunCommandOutcome::Refused {
        reason: RunCommandRefusal::Busy,
    });
    for state in [applied(&command), refusal] {
        let resolved = ledger
            .record_command(Some(command_receipt(&command, state)))
            .unwrap();
        assert!(resolved.command().is_none());
        assert_eq!(resolved.load(), ledger.load());
    }
    for state in [None, Some(RunCommandState::Pending), Some(indeterminate())] {
        let unresolved = ledger
            .record_command(state.map(|state| command_receipt(&command, state)))
            .unwrap();
        assert_eq!(unresolved.command().unwrap().request(), &command);
    }
}

#[test]
fn decode_refuses_damaged_foreign_and_contradictory_files() {
    let valid = json(&both());
    let mut cases: Vec<(serde_json::Value, DecodeError)> = vec![];
    let mut edit = |change: &dyn Fn(&mut serde_json::Value), error| {
        let mut value = valid.clone();
        change(&mut value);
        cases.push((value, error));
    };
    edit(&|v| v["extra"] = 1.into(), DecodeError::Malformed);
    edit(
        &|v| v["load"]["target"]["extra"] = 1.into(),
        DecodeError::Malformed,
    );
    edit(
        &|v| v["apiVersion"] = "kagami.run-intents/v2".into(),
        DecodeError::UnsupportedVersion,
    );
    // Relative paths need the explicit scheme; a bare name means a host.
    edit(
        &|v| v["load"]["target"]["address"] = "/run/orishu/worker.sock".into(),
        DecodeError::Malformed,
    );
    edit(
        &|v| v["load"]["target"]["address"] = "".into(),
        DecodeError::Malformed,
    );
    edit(
        &|v| v["load"]["source"]["incarnation"] = "12345678-9ABC-4DEF-8123-456789ABCDEF".into(),
        DecodeError::Malformed,
    );
    edit(
        &|v| v["load"]["target"]["formationId"] = "formation-b".into(),
        DecodeError::Inconsistent,
    );
    edit(
        &|v| v["command"]["target"]["formationId"] = "formation-b".into(),
        DecodeError::Inconsistent,
    );
    edit(
        &|v| v["load"]["request"]["operationId"] = "kagami-load-9".into(),
        DecodeError::Inconsistent,
    );
    edit(
        &|v| v["command"]["receipt"]["request"]["operationId"] = "kagami-9".into(),
        DecodeError::Inconsistent,
    );
    edit(
        &|v| v["load"]["request"]["formationId"] = "formation-b".into(),
        DecodeError::Inconsistent,
    );
    for (value, error) in cases {
        assert_eq!(decode_value(&value), Err(error), "{value}");
    }

    // A command record is removed when it is final; a final one is damage.
    let command = command_request("kagami-1");
    let mut value = valid.clone();
    value["command"]["receipt"] =
        serde_json::to_value(command_receipt(&command, applied(&command))).unwrap();
    assert_eq!(decode_value(&value), Err(DecodeError::Inconsistent));

    let bytes = both().encode().unwrap();
    assert_eq!(
        Ledger::decode(&bytes[..bytes.len() - 1]),
        Err(DecodeError::Malformed)
    );
    for bad in [&b""[..], b"null", b"[]", b"{}", b"\xff"] {
        assert_eq!(Ledger::decode(bad), Err(DecodeError::Malformed));
    }
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
        "\"load\":",
        "\"command\": null,\n  \"load\":",
        1,
    );
    assert_eq!(
        Ledger::decode(duplicate.as_bytes()),
        Err(DecodeError::Malformed)
    );
    let mut oversized = bytes;
    oversized.resize(MAX_LEDGER_BYTES + 1, b' ');
    assert_eq!(Ledger::decode(&oversized), Err(DecodeError::Oversized));
}

#[test]
fn worker_addresses_persist_exactly_or_are_refused() {
    for text in [
        "unix:/run/orishu/worker.sock",
        "unix:custom.sock",
        "192.0.2.1:6680",
        "[2001:db8::1]:6680",
        "cluster.orishu.local:6680",
    ] {
        let address: ClusterAddress = text.parse().unwrap();
        let target = Target::new(address.clone(), formation()).unwrap();
        let ledger = Ledger::default()
            .begin_load(target, load_request("kagami-load-1"), source())
            .unwrap();
        let value = json(&ledger);
        assert_eq!(value["load"]["target"]["address"], text);
        assert_eq!(
            Ledger::decode(&ledger.encode().unwrap())
                .unwrap()
                .load()
                .unwrap()
                .target()
                .address(),
            &address
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::OsStr::from_bytes(b"/run/\xff.sock");
        assert_eq!(
            Target::new(ClusterAddress::UnixSocket(path.into()), formation()),
            Err(IntentError::Address)
        );
    }
}

#[test]
fn an_oversized_ledger_is_refused_before_it_can_be_written() {
    let host = "a".repeat(MAX_LEDGER_BYTES);
    let target = Target::new(ClusterAddress::Host { host, port: 6680 }, formation()).unwrap();
    let ledger = Ledger::default()
        .begin_load(target, load_request("kagami-load-1"), source())
        .unwrap();
    assert_eq!(ledger.encode(), Err(IntentError::Oversized));
}
