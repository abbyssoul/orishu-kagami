//! Every collection a workload can declare is bounded while it is read.
//!
//! One test per collection family, each shaped the same way: build the same
//! document twice, once holding exactly the limit and once holding one more,
//! and require the first to parse and the second to be refused by name. A bound
//! that is never reached is a bound nobody is relying on, and a bound whose
//! error does not say which collection it is about cannot be acted on by the
//! person who wrote the document.
//!
//! Both codecs are exercised for every case. JSON and YAML are two spellings of
//! one authoring path, so a bound that held for one and not the other would
//! mean the choice of syntax decided what a reader would accept.
//!
//! The documents are built as `serde_json::Value` rather than as text precisely
//! so that both renderings come from one source. Limits are tightened per test
//! instead of generating hundreds of entries: what is under test is the
//! comparison, not the arithmetic.
//!
//! The *ordering* claim — that the entry past the bound is never deserialized —
//! is asserted against the reusable machinery in `src/authoring/seed.rs`, where
//! an element seed can be made to panic if it is ever reached. Here the
//! observable behaviour is the refusal and its reason.

use orishu_workload::{AuthoringError, CollectionLimit, Limits, authoring};
use serde_json::{Value, json};

// ── shaping a document ──────────────────────────────────────────────────────

/// A descriptor for `role`, distinct per `index`.
fn descriptor(role: &str, index: usize) -> Value {
    json!({
        "role": role,
        "digest": format!("sha256:{index:064x}"),
        "sizeBytes": 0,
        "mediaType": "application/octet-stream",
    })
}

fn component(index: usize) -> Value {
    json!({
        "instanceId": format!("instance-{index}"),
        "artifact": descriptor("component", index),
        "pluginId": "dev.orishu.test",
        "modelId": "dev.orishu.test.model/v1",
        "schemaId": "dev.orishu.test.state/v1",
        "engine": "wasm-component",
        "lifecycle": "orishu.component/v1",
    })
}

fn channel(index: usize) -> Value {
    json!({
        "channelId": format!("channel-{index}"),
        "schema": {"schemaId": "dev.orishu.test.state/v1", "version": 1},
        "reduction": "sum",
    })
}

fn invocation(index: usize) -> Value {
    json!({
        "invocationId": format!("invocation-{index}"),
        "instance": "instance-0",
        "phaseId": "update",
    })
}

/// `count` values produced by `each`, as a JSON array.
fn list(count: usize, each: impl Fn(usize) -> Value) -> Value {
    Value::Array((0..count).map(each).collect())
}

/// `count` distinct symbols sharing `prefix`.
fn names(prefix: &str, count: usize) -> Value {
    list(count, |index| json!(format!("{prefix}-{index}")))
}

/// `count` distinct entries in a map of resolved scalars.
fn parameters(count: usize) -> Value {
    Value::Object(
        (0..count)
            .map(|index| (format!("parameter-{index}"), json!(index)))
            .collect(),
    )
}

/// A workload that parses: one component, one channel, one invocation.
///
/// Not necessarily one that *validates* — nothing here reaches the closure
/// validator, and keeping the base minimal is what lets each test change one
/// collection and nothing else.
fn base() -> Value {
    json!({
        "apiVersion": "orishu.dev/v2",
        "kind": "Workload",
        "metadata": {"name": "bounds"},
        "spec": {
            "compute": {
                "workloadGraphProfile": "orishu.workload-graph/v1",
                "components": [component(0)],
                "channels": [channel(0)],
                "stepPlan": {
                    "profile": "orishu.workload-graph/v1",
                    "invocations": [invocation(0)],
                },
            },
            "domain": {
                "dimensions": 3,
                "bounds": {"shape": "cube", "sideMetres": 1.0},
                "discretization": {"spaceMetres": 0.001, "timeSeconds": 1.5e-11},
            },
        },
    })
}

/// The base document with `value` placed at a `/`-separated `path`.
///
/// Missing intermediate objects are created, so a test can reach into a part of
/// the schema the minimal document leaves out without restating the rest of it.
fn with(path: &str, value: Value) -> Value {
    let mut document = base();
    let steps: Vec<&str> = path.split('/').collect();
    let (last, parents) = steps.split_last().expect("a non-empty path");
    let mut cursor = &mut document;
    for step in parents {
        cursor = match step.parse::<usize>() {
            Ok(index) => cursor
                .get_mut(index)
                .unwrap_or_else(|| panic!("no [{index}] on the way to {path}")),
            Err(_) => {
                if cursor.get(*step).is_none() {
                    cursor[*step] = json!({});
                }
                &mut cursor[*step]
            }
        };
    }
    match last.parse::<usize>() {
        Ok(index) => cursor[index] = value,
        Err(_) => cursor[*last] = value,
    }
    document
}

// ── asserting on both codecs ────────────────────────────────────────────────

/// One document, as each authoring codec spells it.
fn renderings(document: &Value) -> [(&'static str, String); 2] {
    [
        ("JSON", serde_json::to_string(document).expect("it encodes")),
        ("YAML", serde_yaml::to_string(document).expect("it encodes")),
    ]
}

fn accepted(document: &Value, limits: &Limits) {
    for (codec, text) in renderings(document) {
        authoring::parse_str(&text, limits)
            .unwrap_or_else(|error| panic!("{codec} must parse but did not: {error}\n{text}"));
    }
}

fn refused(document: &Value, limits: &Limits, collection: &str, limit: usize) {
    for (codec, text) in renderings(document) {
        match authoring::parse_str(&text, limits) {
            Err(AuthoringError::CollectionTooLarge(violation)) => {
                assert_eq!(
                    violation.collection, collection,
                    "{codec} named the wrong collection"
                );
                assert_eq!(violation.limit, limit, "{codec} reported the wrong bound");
                assert!(
                    violation.to_string().contains(collection),
                    "{codec}: the message must name the collection, got: {violation}"
                );
            }
            other => panic!("{codec}: expected `{collection}` to be refused, got {other:?}"),
        }
    }
}

/// Exactly `limit` entries parse; `limit + 1` are refused by name.
///
/// `shape(count)` must produce the same document but for the size of the one
/// collection under test, so that the only difference between the accepted and
/// the refused document is the entry that crosses the bound.
fn bounded(collection: &str, limit: usize, limits: &Limits, shape: impl Fn(usize) -> Value) {
    accepted(&shape(limit), limits);
    refused(&shape(limit + 1), limits, collection, limit);
}

// ── metadata ────────────────────────────────────────────────────────────────

#[test]
fn metadata_labels_are_bounded() {
    let limits = Limits {
        max_labels: 3,
        ..Limits::DEFAULT
    };
    bounded("the label count", 3, &limits, |count| {
        with(
            "metadata/labels",
            Value::Object(
                (0..count)
                    .map(|index| (format!("label-{index}"), json!("value")))
                    .collect(),
            ),
        )
    });
}

// ── the component graph ─────────────────────────────────────────────────────

#[test]
fn component_instances_are_bounded() {
    let limits = Limits {
        max_components: 3,
        ..Limits::DEFAULT
    };
    bounded("the component count", 3, &limits, |count| {
        with("spec/compute/components", list(count, component))
    });
}

#[test]
fn channels_are_bounded() {
    let limits = Limits {
        max_channels: 3,
        ..Limits::DEFAULT
    };
    bounded("the channel count", 3, &limits, |count| {
        with("spec/compute/channels", list(count, channel))
    });
}

#[test]
fn a_channels_shape_rank_is_bounded() {
    // Not the domain's dimensionality: a rank-2 tensor field in three
    // dimensions has shape `[3, 3]`, so the two counts are independent.
    let limits = Limits {
        max_channel_shape_rank: 2,
        ..Limits::DEFAULT
    };
    bounded("a channel's shape rank", 2, &limits, |count| {
        with(
            "spec/compute/channels/0/shape",
            list(count, |index| json!(index + 1)),
        )
    });
}

#[test]
fn component_roles_are_bounded() {
    let limits = Limits {
        max_roles_per_component: 2,
        ..Limits::DEFAULT
    };
    bounded("a component's role count", 2, &limits, |count| {
        with("spec/compute/components/0/roles", names("role", count))
    });
}

#[test]
fn component_state_ownership_is_bounded() {
    let limits = Limits {
        max_state_ownership_per_component: 2,
        ..Limits::DEFAULT
    };
    bounded("a component's owned-channel count", 2, &limits, |count| {
        with(
            "spec/compute/components/0/stateOwnership",
            names("channel", count),
        )
    });
}

#[test]
fn component_configuration_is_bounded() {
    let limits = Limits {
        max_config_entries: 2,
        ..Limits::DEFAULT
    };
    bounded("a component's configuration size", 2, &limits, |count| {
        with("spec/compute/components/0/config", parameters(count))
    });
}

#[test]
fn component_resource_limits_are_bounded() {
    let limits = Limits {
        max_limit_entries: 2,
        ..Limits::DEFAULT
    };
    bounded("a component's limit count", 2, &limits, |count| {
        with("spec/compute/components/0/limits", parameters(count))
    });
}

// ── the step plan ───────────────────────────────────────────────────────────

#[test]
fn step_invocations_are_bounded() {
    let limits = Limits {
        max_step_invocations: 3,
        ..Limits::DEFAULT
    };
    bounded("the step-invocation count", 3, &limits, |count| {
        with("spec/compute/stepPlan/invocations", list(count, invocation))
    });
}

#[test]
fn invocation_inputs_are_bounded() {
    let limits = Limits {
        max_inputs_per_invocation: 2,
        ..Limits::DEFAULT
    };
    bounded("an invocation's input count", 2, &limits, |count| {
        with(
            "spec/compute/stepPlan/invocations/0/inputs",
            names("channel", count),
        )
    });
}

#[test]
fn invocation_outputs_are_bounded() {
    let limits = Limits {
        max_outputs_per_invocation: 2,
        ..Limits::DEFAULT
    };
    bounded("an invocation's output count", 2, &limits, |count| {
        with(
            "spec/compute/stepPlan/invocations/0/outputs",
            names("channel", count),
        )
    });
}

#[test]
fn invocation_dependencies_are_bounded() {
    let limits = Limits {
        max_dependencies_per_invocation: 2,
        ..Limits::DEFAULT
    };
    bounded("an invocation's dependency count", 2, &limits, |count| {
        with(
            "spec/compute/stepPlan/invocations/0/dependsOn",
            names("invocation", count),
        )
    });
}

// ── placement ───────────────────────────────────────────────────────────────

#[test]
fn placement_constraints_are_bounded() {
    let limits = Limits {
        max_placement_constraints: 2,
        ..Limits::DEFAULT
    };
    bounded("the placement-constraint count", 2, &limits, |count| {
        with(
            "spec/compute/placementConstraints",
            list(
                count,
                |index| json!({"constraint": format!("constraint-{index}")}),
            ),
        )
    });
}

#[test]
fn the_instances_a_placement_constraint_names_are_bounded_by_the_graph() {
    // A constraint names instances of the graph it constrains, so it can never
    // usefully name more than the graph may hold. The bound is deliberately the
    // graph's own, and the message still names the constraint's list so an
    // author knows which one to shorten.
    let limits = Limits {
        max_components: 2,
        ..Limits::DEFAULT
    };
    bounded(
        "a placement constraint's instance count",
        2,
        &limits,
        |count| {
            with(
                "spec/compute/placementConstraints",
                json!([{"constraint": "co-locate", "instances": names("instance", count)}]),
            )
        },
    );
}

#[test]
fn placement_constraint_parameters_are_bounded() {
    let limits = Limits {
        max_parameter_entries: 2,
        ..Limits::DEFAULT
    };
    bounded(
        "a placement constraint's parameter count",
        2,
        &limits,
        |count| {
            with(
                "spec/compute/placementConstraints",
                json!([{"constraint": "co-locate", "parameters": parameters(count)}]),
            )
        },
    );
}

// ── the domain ──────────────────────────────────────────────────────────────

#[test]
fn a_box_domains_side_lengths_are_bounded() {
    // The rank must equal the declared dimensionality, which
    // `DomainSpec::validate` checks later. This bound is what stops a document
    // declaring a million of them in order to be told so.
    let limits = Limits {
        max_domain_dimensions: 3,
        ..Limits::DEFAULT
    };
    bounded("the domain side-length count", 3, &limits, |count| {
        with(
            "spec/domain/bounds",
            json!({"shape": "box", "sideMetres": list(count, |index| json!(index as f64 + 1.0))}),
        )
    });
}

#[test]
fn integration_parameters_are_bounded() {
    let limits = Limits {
        max_parameter_entries: 2,
        ..Limits::DEFAULT
    };
    bounded("the integration parameter count", 2, &limits, |count| {
        with(
            "spec/domain/discretization/integration",
            json!({"scheme": "velocity-verlet", "parameters": parameters(count)}),
        )
    });
}

#[test]
fn a_box_domain_parses_however_its_keys_are_ordered() {
    // The shape tag decides how `sideMetres` is read, and an author may write
    // them either way round. The bounded reader resolves that without buffering
    // the map, so this is the test that it still accepts both.
    let limits = Limits::DEFAULT;
    for bounds in [
        json!({"shape": "box", "sideMetres": [1.0, 2.0, 3.0]}),
        json!({"sideMetres": [1.0, 2.0, 3.0], "shape": "box"}),
    ] {
        let text = serde_yaml::to_string(&with("spec/domain/bounds", bounds)).expect("it encodes");
        authoring::parse_str(&text, &limits).expect("either key order parses");
    }
}

#[test]
fn a_shape_and_its_side_lengths_must_agree_about_their_form() {
    let limits = Limits::DEFAULT;
    for (what, bounds) in [
        (
            "a cube given a list",
            json!({"shape": "cube", "sideMetres": [1.0, 2.0]}),
        ),
        (
            "a box given one length",
            json!({"shape": "box", "sideMetres": 1.0}),
        ),
        (
            "an unknown shape",
            json!({"shape": "sphere", "sideMetres": 1.0}),
        ),
        (
            "an unknown key",
            json!({"shape": "cube", "sideMetres": 1.0, "radius": 2.0}),
        ),
    ] {
        let text = serde_json::to_string(&with("spec/domain/bounds", bounds)).expect("it encodes");
        assert!(
            authoring::parse_str(&text, &limits).is_err(),
            "{what} must be refused"
        );
    }
}

// ── artifacts ───────────────────────────────────────────────────────────────

#[test]
fn initial_conditions_are_bounded() {
    let limits = Limits {
        max_initial_conditions: 2,
        ..Limits::DEFAULT
    };
    bounded("the initial-condition count", 2, &limits, |count| {
        with(
            "spec/inputs/initialConditions",
            list(count, |index| descriptor("initial-conditions", index + 100)),
        )
    });
}

#[test]
fn the_artifact_count_is_bounded_across_every_collection_that_declares_one() {
    // The one bound that spans collections: a component's artifact, the
    // geometry, the initial conditions, and the additional inputs all draw on
    // it. Counting them separately would let a manifest hold four times as many
    // descriptors as the bound names.
    let limits = Limits {
        max_artifacts: 4,
        ..Limits::DEFAULT
    };
    // One component artifact and one geometry are already declared, so `count`
    // additional inputs make `count + 2` in total.
    bounded("the artifact count", 4, &limits, |count| {
        let mut document = with("spec/inputs/geometry", descriptor("geometry", 900));
        document["spec"]["inputs"]["additional"] = list(count.saturating_sub(2), |index| {
            descriptor("material-table", index + 200)
        });
        document
    });
}

// ── requirements ────────────────────────────────────────────────────────────

#[test]
fn hardware_requirements_are_bounded() {
    let limits = Limits {
        max_parameter_entries: 2,
        ..Limits::DEFAULT
    };
    bounded("the hardware-requirement count", 2, &limits, |count| {
        with("spec/requirements/hardware", parameters(count))
    });
}

#[test]
fn the_execution_profile_is_bounded() {
    let limits = Limits {
        max_parameter_entries: 2,
        ..Limits::DEFAULT
    };
    bounded("the execution-profile count", 2, &limits, |count| {
        with("spec/requirements/executionProfile", parameters(count))
    });
}

// ── the same bounds on the other two entry points ───────────────────────────

#[test]
fn every_entry_point_reads_through_the_bounded_path() {
    // `parse_bytes` and `from_reader` are conveniences over `parse_str`, and a
    // convenience that skipped the bounds would be the one a network-facing
    // caller reached for.
    let limits = Limits {
        max_components: 1,
        ..Limits::DEFAULT
    };
    let document = with("spec/compute/components", list(2, component));
    let text = serde_yaml::to_string(&document).expect("it encodes");

    let expected = CollectionLimit {
        collection: "the component count",
        limit: 1,
        found: Some(2),
    };
    for (what, error) in [
        ("parse_str", authoring::parse_str(&text, &limits)),
        (
            "parse_bytes",
            authoring::parse_bytes(text.as_bytes(), &limits),
        ),
        (
            "from_reader",
            authoring::from_reader(text.as_bytes(), &limits),
        ),
    ] {
        match error {
            Err(AuthoringError::CollectionTooLarge(violation)) => {
                assert_eq!(violation, expected, "{what}");
            }
            other => panic!("{what}: expected a bounded refusal, got {other:?}"),
        }
    }
}

// ── the bounded reader did not change what a workload is ────────────────────

#[test]
fn a_bounded_read_refuses_the_same_documents_it_always_did() {
    let limits = Limits::DEFAULT;

    // An unknown key, at each level, is an error rather than a dropped field.
    for path in [
        "spec/compute/components/0/installPath",
        "spec/compute/channels/0/uri",
        "spec/compute/stepPlan/invocations/0/when",
        "spec/domain/resolution",
        "metadata/uid",
        "metadata/namespace",
    ] {
        let text = serde_json::to_string(&with(path, json!("value"))).expect("it encodes");
        let error = authoring::parse_str(&text, &limits)
            .expect_err(&format!("{path} must be refused"))
            .to_string();
        let key = path.rsplit('/').next().expect("a final segment");
        assert!(
            error.contains(key),
            "the error should name `{key}`, got: {error}"
        );
    }

    // A status, in either spelling, is unrepresentable rather than ignored.
    for status in [json!({"phase": "Running"}), Value::Null] {
        let mut document = base();
        document["status"] = status;
        let text = serde_json::to_string(&document).expect("it encodes");
        assert!(authoring::parse_str(&text, &limits).is_err());
    }

    // A required field cannot be omitted.
    for path in ["spec/compute", "spec/domain", "metadata/name"] {
        let mut document = base();
        let steps: Vec<&str> = path.split('/').collect();
        let (last, parents) = steps.split_last().expect("a path");
        let mut cursor = &mut document;
        for step in parents {
            cursor = &mut cursor[*step];
        }
        cursor
            .as_object_mut()
            .expect("an object")
            .remove(*last)
            .expect("the field was there");
        let text = serde_json::to_string(&document).expect("it encodes");
        let error = authoring::parse_str(&text, &limits)
            .expect_err(&format!("{path} is required"))
            .to_string();
        assert!(
            error.contains(last),
            "the error should name the missing `{last}`, got: {error}"
        );
    }

    // A repeated key is a duplicate rather than a last-one-wins overwrite.
    let duplicated = r#"
apiVersion: orishu.dev/v2
apiVersion: orishu.dev/v2
kind: Workload
metadata: {name: bounds}
spec:
  compute:
    workloadGraphProfile: orishu.workload-graph/v1
    components: []
    stepPlan: {profile: orishu.workload-graph/v1, invocations: []}
  domain:
    dimensions: 3
    bounds: {shape: cube, sideMetres: 1.0}
    discretization: {spaceMetres: 0.001, timeSeconds: 1.0}
"#;
    let error = authoring::parse_str(duplicated, &limits)
        .expect_err("a duplicate field is refused")
        .to_string();
    assert!(
        error.contains("apiVersion"),
        "the error should name the duplicated field, got: {error}"
    );
}
