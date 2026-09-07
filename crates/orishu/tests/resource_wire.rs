//! Golden wire fixtures for every Orishu resource that uses the shared
//! resource envelope, plus the two provenance records that borrow its
//! identity newtypes.
//!
//! These exist because extracting the structural envelope into
//! `orishu-resource` must not change a single byte a client already sees. The
//! fixtures are captured from the real serde path — `serde_json` and
//! `serde_yaml` over the public model types — rather than asserted field by
//! field, so a reordered key, a dropped `skip_serializing_if`, or a changed
//! discriminator shows up as a diff in a checked-in file rather than as a
//! silently passing structural test.
//!
//! Regenerate deliberately, never to make a red test green:
//!
//! ```sh
//! BLESS_RESOURCE_FIXTURES=1 cargo test -p orishu --test resource_wire
//! ```
//!
//! and review the resulting diff as a wire change.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use orishu::model::{
    checkpoint::{self, CheckpointId},
    cluster::{self, ClusterSpec, ClusterStatus, Version},
    manifest::{Name, ResourceUid},
    node::{
        self, ConnectionStats, MemberState, NodeAccepts, NodeCapabilities, NodeId, NodeLimits,
        NodeListen, NodeSpec, NodeStatus, NodeStorage,
    },
    result::{self, ResultId},
    storage::StorageBackend,
    workload::{self, SimulationState, WorkloadStatus},
};

// ── golden-file plumbing ─────────────────────────────────────────────────────

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/resource-wire")
        .join(name)
}

/// Compare `actual` against the checked-in fixture, or rewrite it when
/// blessing.
fn golden(name: &str, actual: &str) {
    let path = fixture_path(name);
    if std::env::var_os("BLESS_RESOURCE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture directory"))
            .expect("create the fixture directory");
        std::fs::write(&path, actual).expect("write the fixture");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}\n\
             capture it with BLESS_RESOURCE_FIXTURES=1 and review the diff",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "the serialized form of `{name}` changed; this is a wire change, not a refactor"
    );
}

/// Assert both codecs against their fixtures and prove the value survives a
/// round trip through each of them.
fn assert_wire<T>(stem: &str, value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string_pretty(value).expect("JSON encodes");
    golden(&format!("{stem}.json"), &format!("{json}\n"));
    let yaml = serde_yaml::to_string(value).expect("YAML encodes");
    golden(&format!("{stem}.yaml"), &yaml);

    // Decoding is part of the contract too: a fixture that only ever
    // serializes would not catch an envelope that stopped accepting its own
    // output.
    let from_json: T = serde_json::from_str(&json).expect("JSON round-trips");
    assert_eq!(
        serde_json::to_string_pretty(&from_json).expect("re-encodes"),
        json,
        "{stem} did not survive a JSON round trip"
    );
    let from_yaml: T = serde_yaml::from_str(&yaml).expect("YAML round-trips");
    assert_eq!(
        serde_yaml::to_string(&from_yaml).expect("re-encodes"),
        yaml,
        "{stem} did not survive a YAML round trip"
    );
}

fn timestamp(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("a valid RFC 3339 timestamp")
        .with_timezone(&Utc)
}

// ── workload ─────────────────────────────────────────────────────────────────

const MINIMAL_WORKLOAD: &str = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: test-workload
spec:
  domainType: hydrodynamics
  model:
    image:
      uri: "urn:orishu:superseded-prototype-artifact"
  domain:
    dimensions: 2
    bounds: 1m
    discretization:
      space: 1mm
      time: 15ms
"#;

const FULL_WORKLOAD: &str = r#"
apiVersion: orishu.dev/v1
kind: Workload
metadata:
  name: em-cavity-resonance
  namespace: cluster_xyz
  labels:
    domain: electrodynamics
spec:
  domainType: electromagnetic
  model:
    image:
      uri: "urn:orishu:superseded-prototype-artifact"
  domain:
    dimensions: 3
    bounds:
      - 100cm
      - 20cm
      - 30cm
    discretization:
      space: 1mm
      time: 15ms
  inputs:
    geometry:
      mesh:
        uri: "urn:orishu:superseded-prototype-geometry"
    initialConditions:
      image:
        uri: "urn:orishu:superseded-prototype-initial-state"
  requirements:
    hardware:
      minCpuCores: 4
      minMemoryBytes: 8589934592
      architecture: x86_64
      accelerators:
        - "NVIDIA A100"
      minOpenclVersion: "3.0"
    runtimeAbiVersion: "1.2.0"
    executionProfile:
      numericMode: deterministic
"#;

#[test]
fn a_minimal_workload_manifest_keeps_its_wire_form() {
    let manifest = workload::parse(MINIMAL_WORKLOAD).expect("the minimal manifest parses");
    assert_wire("workload-minimal", &manifest);
}

#[test]
fn a_full_workload_manifest_keeps_its_wire_form() {
    let manifest = workload::parse(FULL_WORKLOAD).expect("the full manifest parses");
    assert_wire("workload-full", &manifest);
}

#[test]
fn a_workload_manifest_carries_its_status_when_it_has_one() {
    let mut manifest = workload::parse(MINIMAL_WORKLOAD).expect("the minimal manifest parses");
    manifest.status = Some(WorkloadStatus::Ready);
    assert_wire("workload-with-status", &manifest);
}

// ── cluster: a synthetic projection, never durable authored configuration ────

#[test]
fn the_synthetic_cluster_projection_keeps_its_wire_form() {
    let mut manifest = cluster::manifest(
        "shared label",
        ClusterSpec {
            membership_locked: false,
            workload: None,
        },
    );
    manifest.status = Some(ClusterStatus {
        version: Version {
            epoch: 7,
            counter: 42,
        },
        node_count: 3,
        simulation_status: Some(SimulationState::Stopped),
    });
    assert_wire("cluster", &manifest);
}

// ── node: a cluster-owned membership projection ─────────────────────────────

#[test]
fn a_node_manifest_keeps_its_wire_form() {
    let mut manifest = node::manifest(
        &NodeId::new("node-7f3a").expect("a valid node ID"),
        "worker-a",
        NodeSpec {
            accepts: NodeAccepts {
                clients: true,
                peers: true,
                work: false,
            },
            limits: NodeLimits {
                peers: Some(64),
                clients: None,
            },
            listen: NodeListen {
                peers: vec!["node-a.example.com:6681".to_owned()],
                clients: vec!["node-a.example.com:6680".to_owned()],
            },
        },
    );
    manifest.status = Some(NodeStatus {
        version: "0.1.0".to_owned(),
        cert_fingerprint: vec![0xde, 0xad, 0xbe, 0xef],
        connected: ConnectionStats {
            peers: 2,
            clients: 1,
        },
        capabilities: NodeCapabilities {
            cpu_cores: 8,
            memory_bytes: 17_179_869_184,
            architecture: "x86_64".to_owned(),
            storage: NodeStorage {
                driver: StorageBackend::Local,
                replicas: Some(2),
            },
            accelerators: vec!["NVIDIA A100".to_owned()],
        },
        member: MemberState::Alive,
    });
    assert_wire("node", &manifest);
}

// ── provenance records that borrow the envelope's identity newtypes ──────────
//
// These do not use the envelope, but they do use `manifest::{Name,
// ResourceUid}` for workload provenance. S-WORKLOAD owns what a canonical
// workload identity becomes; until then the `workloadName`/`workloadId`
// spellings must not drift, and they must never be reinterpreted as formation,
// node, run, or artifact identity.

#[test]
fn checkpoint_provenance_keeps_its_workload_identity_spellings() {
    let record = checkpoint::Record {
        id: CheckpointId("chk-0001".to_owned()),
        workload_name: Name("em-cavity-resonance".to_owned()),
        workload_id: ResourceUid("wl-7f3a".to_owned()),
        workload_epoch: 3,
        recorded_at: timestamp("2026-01-02T03:04:05Z"),
        simulation_time: 1.5,
        graceful: true,
        resumable: true,
    };
    assert_wire("checkpoint-record", &record);
}

#[test]
fn result_provenance_keeps_its_workload_identity_spellings() {
    let record = result::Record {
        id: ResultId::from_str("res-0001").expect("infallible"),
        workload_id: ResourceUid("wl-7f3a".to_owned()),
        workload_name: Name("em-cavity-resonance".to_owned()),
        workload_epoch: 3,
        started_at: timestamp("2026-01-02T03:00:00Z"),
        recorded_at: timestamp("2026-01-02T03:04:05Z"),
        simulation_time_range: [0.0, 1.5],
        total_size_bytes: 4096,
        graceful: true,
        storage_backend: StorageBackend::Local,
        chunks: [("p0".to_owned(), "node-a".to_owned())]
            .into_iter()
            .collect(),
        missing_chunks: Vec::new(),
    };
    assert_wire("result-record", &record);
}
