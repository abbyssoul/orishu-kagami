//! What the closure validator refuses, and why.
//!
//! Each test below constructs one specific defect against an otherwise valid
//! workload, so a failure names the rule that broke rather than "the fixture is
//! wrong". The base workload is built in Rust rather than authored as YAML
//! because most of these defects are things the *authoring* layer would happily
//! accept — an unknown channel name is well-formed YAML — and the point is what
//! validation catches, not what parsing catches.

use orishu_workload::{
    ArtifactDescriptor, ArtifactDigest, ArtifactRole, ClosureError, ComponentInstance, ComputeSpec,
    Discretization, DomainBounds, DomainSpec, FiniteF64, Limits, Reduction, SchemaCompat,
    StateChannel, StepInvocation, StepPlan, WorkloadInputs, WorkloadManifest, WorkloadMeta,
    WorkloadRequirements, WorkloadSpec,
    closure::{self, InMemoryBlobs},
};

// ── building a valid workload to then break ─────────────────────────────────

const FIELD_WASM: &[u8] = b"field-component-wasm";
const DYNAMICS_WASM: &[u8] = b"dynamics-component-wasm";

fn name<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("a valid name in a test fixture")
}

fn finite(value: f64) -> FiniteF64 {
    FiniteF64::new(value).expect("a finite test value")
}

fn component_artifact(content: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor {
        role: ArtifactRole::component(),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: name("application/wasm"),
        schema: None,
    }
}

fn component(instance: &str, content: &[u8], owns: &[&str]) -> ComponentInstance {
    ComponentInstance {
        instance_id: name(instance),
        artifact: component_artifact(content),
        plugin_id: name("dev.orishu.test"),
        model_id: name("dev.orishu.test.model/v1"),
        schema_id: name("dev.orishu.test.state/v1"),
        engine: name("wasm-component"),
        lifecycle: name("orishu.component/v1"),
        roles: Vec::new(),
        state_ownership: owns.iter().map(|id| name(id)).collect(),
        config: Default::default(),
        limits: Default::default(),
    }
}

fn channel(id: &str, owner: Option<&str>, reduction: Reduction) -> StateChannel {
    StateChannel {
        channel_id: name(id),
        schema: SchemaCompat {
            schema_id: name("dev.orishu.test.state/v1"),
            version: 1,
        },
        shape: vec![3],
        owner: owner.map(name),
        reduction,
    }
}

fn invocation(id: &str, instance: &str, outputs: &[&str]) -> StepInvocation {
    StepInvocation {
        invocation_id: name(id),
        instance: name(instance),
        phase_id: name("update"),
        inputs: Vec::new(),
        outputs: outputs.iter().map(|id| name(id)).collect(),
        depends_on: Vec::new(),
    }
}

/// A workload that validates: two components, each owning and writing one
/// channel.
fn valid() -> WorkloadManifest {
    orishu_workload::manifest(
        WorkloadMeta::new(name("test workload")),
        WorkloadSpec {
            compute: ComputeSpec {
                workload_graph_profile: name("orishu.workload-graph/v1"),
                components: vec![
                    component("field", FIELD_WASM, &["e-field"]),
                    component("dynamics", DYNAMICS_WASM, &["kinematics"]),
                ],
                channels: vec![
                    channel("e-field", Some("field"), Reduction::Single),
                    channel("kinematics", Some("dynamics"), Reduction::Single),
                ],
                step_plan: StepPlan {
                    profile: name("orishu.workload-graph/v1"),
                    invocations: vec![
                        invocation("advance-field", "field", &["e-field"]),
                        invocation("integrate", "dynamics", &["kinematics"]),
                    ],
                },
                placement_constraints: Vec::new(),
            },
            domain: DomainSpec {
                dimensions: 3,
                bounds: DomainBounds::Cube {
                    side_metres: finite(1.0),
                },
                discretization: Discretization {
                    space_metres: finite(0.001),
                    time_seconds: finite(1.5e-11),
                    integration: None,
                },
            },
            inputs: WorkloadInputs::default(),
            requirements: WorkloadRequirements::default(),
        },
    )
}

fn blobs() -> InMemoryBlobs {
    [FIELD_WASM.to_vec(), DYNAMICS_WASM.to_vec()]
        .into_iter()
        .collect()
}

/// Validates `manifest` against `blobs` and returns the errors it produced.
fn errors_for(manifest: &WorkloadManifest, blobs: &InMemoryBlobs) -> Vec<ClosureError> {
    closure::validate_closure(manifest, blobs, &Limits::DEFAULT)
        .err()
        .map(|report| report.errors().to_vec())
        .unwrap_or_default()
}

/// Asserts exactly one error, and returns it.
fn sole_error(manifest: &WorkloadManifest, blobs: &InMemoryBlobs) -> ClosureError {
    let mut errors = errors_for(manifest, blobs);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one defect to be reported, got: {errors:#?}"
    );
    errors.pop().expect("checked above")
}

// ── the baseline ────────────────────────────────────────────────────────────

#[test]
fn a_well_formed_workload_validates() {
    let verified = closure::validate_closure(&valid(), &blobs(), &Limits::DEFAULT)
        .unwrap_or_else(|report| panic!("the baseline must validate:\n{report}"));
    assert_eq!(verified.len(), 2);
}

// ── the provider is trusted for nothing ─────────────────────────────────────

#[test]
fn a_missing_artifact_is_reported_with_its_role() {
    // A worker that reports what is missing can be sent only that.
    let error = sole_error(&valid(), &InMemoryBlobs::from_iter([FIELD_WASM.to_vec()]));
    assert!(matches!(
        error,
        ClosureError::MissingArtifact { ref role, .. } if role.as_str() == ArtifactRole::COMPONENT
    ));
}

/// A blob source that answers every request with the same bytes, whatever
/// digest was asked for.
///
/// `InMemoryBlobs` cannot be made to lie — it keys content by its own digest —
/// so a hostile or merely broken provider needs its own implementation. This is
/// what "the source is trusted for nothing" has to survive.
struct LyingBlobs(Vec<u8>);

impl closure::BlobSource for LyingBlobs {
    fn blob(&self, _digest: &ArtifactDigest) -> Option<&[u8]> {
        Some(&self.0)
    }
}

#[test]
fn bytes_that_do_not_hash_to_the_declared_digest_are_refused() {
    // A provider that hands over the wrong bytes for the right digest gets
    // nowhere: re-hashing is what decides whether bytes are the artifact, and
    // nothing the provider says participates.
    let mut manifest = valid();
    let impostor = b"not-the-real-componen".to_vec();
    manifest.spec.compute.components[0].artifact.size_bytes = impostor.len() as u64;
    manifest.spec.compute.components[1].artifact.size_bytes = impostor.len() as u64;

    let report = closure::validate_closure(&manifest, &LyingBlobs(impostor), &Limits::DEFAULT)
        .expect_err("bytes that do not hash to the declared digest are not the artifact");
    assert_eq!(report.len(), 2, "got {report}");
    for error in report.errors() {
        let ClosureError::DigestMismatch { declared, actual } = error else {
            panic!("expected a digest mismatch, got {error:?}");
        };
        assert_ne!(declared, actual);
    }
}

#[test]
fn a_provider_cannot_substitute_one_declared_artifact_for_another() {
    // Supplying the Dynamics component wherever the field component was asked
    // for is the realistic form of the attack, and the size check does not
    // catch it when both happen to be plausible.
    let mut manifest = valid();
    manifest.spec.compute.components[0].artifact.size_bytes = DYNAMICS_WASM.len() as u64;
    let report = closure::validate_closure(
        &manifest,
        &LyingBlobs(DYNAMICS_WASM.to_vec()),
        &Limits::DEFAULT,
    )
    .expect_err("one component is not another");
    assert_eq!(report.len(), 1, "got {report}");
    assert!(matches!(
        report.errors()[0],
        ClosureError::DigestMismatch { .. }
    ));
}

#[test]
fn a_size_that_disagrees_with_the_bytes_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.components[0].artifact.size_bytes = 9999;
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::SizeMismatch {
            declared: 9999,
            actual: 20,
            ..
        }
    ));
}

#[test]
fn an_artifact_over_the_per_artifact_budget_is_refused_before_it_is_fetched() {
    let mut manifest = valid();
    manifest.spec.compute.components[0].artifact.size_bytes = 32;
    let tight = Limits {
        max_artifact_bytes: 16,
        ..Limits::DEFAULT
    };
    let report = closure::validate_closure(&manifest, &blobs(), &tight)
        .expect_err("an oversized artifact is refused");
    assert!(
        report
            .errors()
            .iter()
            .any(|error| matches!(error, ClosureError::ArtifactTooLarge { limit: 16, .. })),
        "got {report}"
    );
}

#[test]
fn a_closure_over_the_aggregate_budget_is_refused_before_any_blob_is_fetched() {
    let tight = Limits {
        max_aggregate_declared_bytes: 8,
        ..Limits::DEFAULT
    };
    let report = closure::validate_closure(&valid(), &blobs(), &tight)
        .expect_err("an oversized closure is refused");
    // Exactly one error: the aggregate check stops before per-blob work, so a
    // submitter is told the closure is too big rather than given a list of
    // consequences of that.
    assert_eq!(report.len(), 1, "got {report}");
    assert!(matches!(
        report.errors()[0],
        ClosureError::ClosureTooLarge {
            declared: 43,
            limit: 8
        }
    ));
}

#[test]
fn shared_content_counts_once_against_the_budget() {
    // Two roles naming identical bytes are transferred and stored once, so the
    // aggregate must not double-count them — and, because the two descriptors
    // make the same claims about those bytes, they do not conflict either.
    let mut manifest = valid();
    manifest.spec.inputs.additional = vec![ArtifactDescriptor {
        role: name(ArtifactRole::SCHEMA),
        ..component_artifact(FIELD_WASM)
    }];
    let exact = Limits {
        // The two distinct blobs, and not a byte more.
        max_aggregate_declared_bytes: (FIELD_WASM.len() + DYNAMICS_WASM.len()) as u64,
        ..Limits::DEFAULT
    };
    let errors = errors_for(&manifest, &blobs());
    assert!(errors.is_empty(), "got {errors:#?}");
    assert!(
        closure::validate_closure(&manifest, &blobs(), &exact).is_ok(),
        "identical content must count once"
    );
}

#[test]
fn one_digest_declared_with_two_different_descriptors_is_refused() {
    let mut manifest = valid();
    // Same bytes, but claimed to be a different size in a second place.
    manifest.spec.inputs.additional = vec![ArtifactDescriptor {
        size_bytes: 1,
        ..component_artifact(FIELD_WASM)
    }];
    let errors = errors_for(&manifest, &blobs());
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ClosureError::ConflictingDescriptor { .. })),
        "got {errors:#?}"
    );
}

#[test]
fn a_second_geometry_is_refused_because_a_domain_has_one() {
    let mut manifest = valid();
    let geometry = |content: &[u8]| ArtifactDescriptor {
        role: name(ArtifactRole::GEOMETRY),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: name("model/3mf"),
        schema: None,
    };
    manifest.spec.inputs.geometry = Some(geometry(b"mesh-a"));
    manifest.spec.inputs.additional = vec![geometry(b"mesh-b")];

    let mut blobs = blobs();
    blobs.insert(b"mesh-a".to_vec());
    blobs.insert(b"mesh-b".to_vec());

    let error = sole_error(&manifest, &blobs);
    assert!(matches!(
        error,
        ClosureError::DuplicateRole { count: 2, ref role } if role.as_str() == ArtifactRole::GEOMETRY
    ));
}

#[test]
fn several_initial_conditions_are_not_a_duplicate_role() {
    // One per owned state channel is the normal case, unlike geometry.
    let mut manifest = valid();
    let input = |content: &[u8]| ArtifactDescriptor {
        role: name(ArtifactRole::INITIAL_CONDITIONS),
        digest: ArtifactDigest::sha256_of(content),
        size_bytes: content.len() as u64,
        media_type: name("application/octet-stream"),
        schema: None,
    };
    manifest.spec.inputs.initial_conditions = vec![input(b"field-state"), input(b"particle-state")];

    let mut blobs = blobs();
    blobs.insert(b"field-state".to_vec());
    blobs.insert(b"particle-state".to_vec());

    assert!(errors_for(&manifest, &blobs).is_empty());
}

// ── the graph must be internally consistent ─────────────────────────────────

#[test]
fn a_component_instance_must_name_component_code() {
    let mut manifest = valid();
    manifest.spec.compute.components[0].artifact.role = name(ArtifactRole::GEOMETRY);
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::NotAComponentArtifact { ref instance, .. } if instance.as_str() == "field"
    ));
}

#[test]
fn two_instances_may_not_share_an_id() {
    let mut manifest = valid();
    manifest.spec.compute.components[1].instance_id = name("field");
    let errors = errors_for(&manifest, &blobs());
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ClosureError::DuplicateInstance { .. })),
        "got {errors:#?}"
    );
}

#[test]
fn an_invocation_naming_an_absent_instance_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].instance = name("ghost");
    let errors = errors_for(&manifest, &blobs());
    assert!(
        errors.iter().any(|error| matches!(
            error,
            ClosureError::UnknownInstance { instance, .. } if instance.as_str() == "ghost"
        )),
        "got {errors:#?}"
    );
}

#[test]
fn an_invocation_naming_an_absent_channel_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].inputs = vec![name("no-such-channel")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::UnknownChannel { ref channel, .. } if channel.as_str() == "no-such-channel"
    ));
}

#[test]
fn a_dependency_on_an_absent_invocation_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[1].depends_on = vec![name("no-such-node")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::UnknownDependency { ref dependency, .. }
            if dependency.as_str() == "no-such-node"
    ));
}

#[test]
fn a_consumed_channel_with_no_producer_is_refused() {
    // A consumer would otherwise be invoked with input nothing supplies.
    let mut manifest = valid();
    manifest
        .spec
        .compute
        .channels
        .push(channel("orphan", None, Reduction::Sum));
    manifest.spec.compute.step_plan.invocations[1].inputs = vec![name("orphan")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::ChannelWithoutProducer { ref channel, .. } if channel.as_str() == "orphan"
    ));
}

#[test]
fn two_writers_of_one_authoritative_channel_are_refused() {
    // ADR 0024: authoritative state has a single allowed writer. Several
    // producers are legal only for a contribution channel under a reduction.
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[1].outputs =
        vec![name("kinematics"), name("e-field")];
    let errors = errors_for(&manifest, &blobs());
    assert!(
        errors.iter().any(|error| matches!(
            error,
            ClosureError::MultipleWriters { channel, count: 2 }
                if channel.as_str() == "e-field"
        )),
        "got {errors:#?}"
    );
}

#[test]
fn several_producers_of_a_contribution_channel_are_allowed() {
    let mut manifest = valid();
    manifest
        .spec
        .compute
        .channels
        .push(channel("force", None, Reduction::Sum));
    manifest.spec.compute.step_plan.invocations[0]
        .outputs
        .push(name("force"));
    manifest.spec.compute.step_plan.invocations[1]
        .outputs
        .push(name("force"));
    assert!(errors_for(&manifest, &blobs()).is_empty());
}

#[test]
fn an_instance_writing_state_it_does_not_own_is_refused() {
    let mut manifest = valid();
    // `integrate` runs `dynamics` but writes the channel `field` owns.
    manifest.spec.compute.step_plan.invocations[1].outputs = vec![name("e-field")];
    let errors = errors_for(&manifest, &blobs());
    assert!(
        errors.iter().any(|error| matches!(
            error,
            ClosureError::WriterIsNotOwner { channel, owner, .. }
                if channel.as_str() == "e-field" && owner.as_str() == "field"
        )),
        "got {errors:#?}"
    );
}

#[test]
fn a_cyclic_step_plan_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].depends_on = vec![name("integrate")];
    manifest.spec.compute.step_plan.invocations[1].depends_on = vec![name("advance-field")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::CyclicStepPlan { count: 2 }));
}

#[test]
fn a_node_behind_a_cycle_is_counted_as_unreachable() {
    let mut manifest = valid();
    let mut third = invocation("finalise", "dynamics", &[]);
    third.depends_on = vec![name("advance-field")];
    manifest.spec.compute.step_plan.invocations.push(third);
    manifest.spec.compute.step_plan.invocations[0].depends_on = vec![name("integrate")];
    manifest.spec.compute.step_plan.invocations[1].depends_on = vec![name("advance-field")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::CyclicStepPlan { count: 3 }));
}

#[test]
fn a_self_dependency_is_a_cycle() {
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].depends_on = vec![name("advance-field")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::CyclicStepPlan { count: 1 }));
}

#[test]
fn an_unknown_dependency_is_reported_once_rather_than_also_as_a_cycle() {
    // Kahn's algorithm would strand the node behind an unsatisfiable edge and
    // report it as a cycle too. One mistake, one error.
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].depends_on = vec![name("no-such-node")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::UnknownDependency { .. }));
}

#[test]
fn a_component_owning_an_absent_channel_is_refused() {
    let mut manifest = valid();
    manifest.spec.compute.components[0].state_ownership = vec![name("no-such-channel")];
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::UnknownChannel { .. }));
}

#[test]
fn a_placement_constraint_naming_an_absent_instance_is_refused() {
    let mut manifest = valid();
    manifest
        .spec
        .compute
        .placement_constraints
        .push(orishu_workload::PlacementConstraint {
            constraint: name("co-locate"),
            instances: vec![name("field"), name("ghost")],
            parameters: Default::default(),
        });
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(
        error,
        ClosureError::UnknownInstance { instance, .. } if instance.as_str() == "ghost"
    ));
}

// ── bounds ──────────────────────────────────────────────────────────────────

#[test]
fn a_graph_over_the_component_bound_is_refused() {
    let tight = Limits {
        max_components: 1,
        ..Limits::DEFAULT
    };
    let report = closure::validate_closure(&valid(), &blobs(), &tight).expect_err("over the bound");
    assert!(
        report.errors().iter().any(|error| matches!(
            error,
            ClosureError::LimitExceeded {
                what: "the component count",
                found: 2,
                limit: 1
            }
        )),
        "got {report}"
    );
}

#[test]
fn a_plan_over_the_invocation_bound_is_refused() {
    let tight = Limits {
        max_step_invocations: 1,
        ..Limits::DEFAULT
    };
    let report = closure::validate_closure(&valid(), &blobs(), &tight).expect_err("over the bound");
    assert!(
        report.errors().iter().any(|error| matches!(
            error,
            ClosureError::LimitExceeded {
                what: "the step-invocation count",
                ..
            }
        )),
        "got {report}"
    );
}

// ── the whole picture, not the first problem ────────────────────────────────

#[test]
fn every_defect_is_reported_rather_than_only_the_first() {
    // A submitter fixing a workload wants the list. Re-running the validator
    // once per mistake is a poor substitute.
    let mut manifest = valid();
    manifest.spec.compute.step_plan.invocations[0].instance = name("ghost");
    manifest.spec.compute.components[1].artifact.role = name(ArtifactRole::GEOMETRY);
    manifest.spec.domain.dimensions = 0;

    let report = closure::validate_closure(&manifest, &InMemoryBlobs::new(), &Limits::DEFAULT)
        .expect_err("a broken workload is refused");
    assert!(
        report.len() >= 4,
        "expected the domain, graph, and both missing artifacts, got {} in:\n{report}",
        report.len()
    );
    let rendered = report.to_string();
    for expected in ["domain", "ghost", "component", "candidate bytes"] {
        assert!(
            rendered.contains(expected),
            "the report should mention {expected:?}, got:\n{rendered}"
        );
    }
}

#[test]
fn a_document_of_the_wrong_kind_stops_before_anything_else_is_checked() {
    // Nothing below the discriminator is meaningful for a document that is not
    // a workload, so reporting graph defects in it would be noise.
    let valid = valid();
    let (_, _, metadata, spec, _) = valid.into_parts();
    let wrong_kind = WorkloadManifest::new(
        orishu_workload::api_version(),
        orishu_workload::manifest::Kind::from_static("Cluster"),
        metadata,
        spec,
    );
    let report = closure::validate_closure(&wrong_kind, &InMemoryBlobs::new(), &Limits::DEFAULT)
        .expect_err("a Cluster is not a Workload");
    assert_eq!(report.len(), 1, "got {report}");
    assert!(matches!(report.errors()[0], ClosureError::Discriminator(_)));
}

// ── the domain ──────────────────────────────────────────────────────────────

#[test]
fn an_inconsistent_domain_is_reported_as_a_closure_defect() {
    let mut manifest = valid();
    manifest.spec.domain.discretization.space_metres = finite(10.0);
    let error = sole_error(&manifest, &blobs());
    assert!(matches!(error, ClosureError::Domain(_)));
}
