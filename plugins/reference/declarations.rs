//! Shared declarative vocabulary/examples used by native packaging and tests.
//! Does not compile/link guests or contain host-owned integration equations.
use orishu_plugin::*;
use std::collections::BTreeMap;
#[path = "newtonian/src/channels.rs"]
pub mod channels;

/// Assemble example releases through the same public identity/validation APIs as
/// external plugin tooling. No filesystem, installer or special built-in path.
pub fn release(
    name: &str,
    payloads: Vec<(LocalContributionId, Payload)>,
    code: &[&[u8]],
) -> Result<(Release, BTreeMap<ArtifactDigest, Vec<u8>>), Error> {
    let mut blobs = BTreeMap::new();
    let mut artifacts = BTreeMap::new();
    let mut contributions = Vec::new();
    for (local_id, payload) in payloads {
        let bytes = payload.canonical_bytes(&Limits::default())?;
        let digest = ArtifactDigest::sha256_of(&bytes);
        artifacts.insert(
            digest,
            Artifact {
                digest,
                size_bytes: bytes.len() as u64,
                media_type: "application/cbor".into(),
            },
        );
        contributions.push(Contribution {
            local_id,
            extension_point: payload.point().as_str().parse()?,
            payload: digest,
            requirements: payload
                .requirements()
                .iter()
                .map(|r| Requirement::Contract {
                    slot: r.slot.clone(),
                    contract: r.contract.clone(),
                })
                .collect(),
            annotations: None,
        });
        blobs.insert(digest, bytes);
    }
    for bytes in code {
        let digest = ArtifactDigest::sha256_of(bytes);
        artifacts.insert(
            digest,
            Artifact {
                digest,
                size_bytes: bytes.len() as u64,
                media_type: "application/wasm".into(),
            },
        );
        blobs.insert(digest, bytes.to_vec());
    }
    Ok((
        Release::new(
            ReleaseMetadata {
                plugin_id: name.parse()?,
                version_label: "reference-v1".into(),
                display_name: None,
                description: None,
            },
            ReleaseSpec {
                contributions,
                artifacts: artifacts.into_values().collect(),
            },
        ),
        blobs,
    ))
}
fn id(s: &str) -> LocalContributionId {
    s.parse().unwrap()
}
fn quantity(name: &str, dimension: Dimension, default: &str) -> Property {
    Property {
        id: id(name),
        required: true,
        schema: PropertyType::Quantity {
            dimension,
            default_expression: Some(default.into()),
            minimum_si: None,
            maximum_si: None,
        },
    }
}
fn requirement(slot: &str, payload: &Payload) -> ContractRequirement {
    ContractRequirement {
        slot: id(slot),
        contract: payload.contract_ref(&Limits::default()).unwrap(),
    }
}
pub fn vocabulary() -> Vec<(LocalContributionId, Payload)> {
    let mut result = vec![
        (
            id("mass"),
            Payload::Components(Declaration {
                scientific: ComponentSchema {
                    name: "org.orishu.reference.gravity.mass".parse().unwrap(),
                    version: 1.try_into().unwrap(),
                    requirements: vec![],
                    properties: vec![
                        quantity("source", Dimension::MASS, "1 kg"),
                        quantity("response", Dimension::MASS, "1 kg"),
                    ],
                    role: ComponentRole::FieldCoupling,
                    bindings: RoleBindings {
                        source: Some(id("source")),
                        response: Some(id("response")),
                        inertial_mass: None,
                    },
                },
                presentation: None,
            }),
        ),
        (
            id("dynamics"),
            Payload::Components(Declaration {
                scientific: ComponentSchema {
                    name: "org.orishu.reference.dynamics".parse().unwrap(),
                    version: 1.try_into().unwrap(),
                    requirements: vec![],
                    properties: vec![quantity("inertial-mass", Dimension::MASS, "1 kg")],
                    role: ComponentRole::Dynamics,
                    bindings: RoleBindings {
                        inertial_mass: Some(id("inertial-mass")),
                        ..RoleBindings::default()
                    },
                },
                presentation: None,
            }),
        ),
    ];
    for channel in channels::bindings() {
        result.push((
            channel.slot,
            Payload::Observables(Declaration {
                scientific: channel.channel.schema,
                presentation: None,
            }),
        ));
    }
    let acceleration = &result
        .iter()
        .find(|(id, _)| id.as_str() == "acceleration")
        .unwrap()
        .1;
    result.push((
        id("gravity"),
        Payload::Fields(Declaration {
            scientific: FieldSchema {
                name: "org.orishu.reference.gravity".parse().unwrap(),
                version: 1.try_into().unwrap(),
                requirements: vec![requirement("acceleration", acceleration)],
                domain_dimension: 3,
                domain_requirements: vec![id("box")],
                required_observables: vec![id("acceleration")],
            },
            presentation: None,
        }),
    ));
    result
}
pub fn newtonian(code: ArtifactDigest) -> Payload {
    let vocabulary = vocabulary();
    let get = |id: &str| &vocabulary.iter().find(|(i, _)| i.as_str() == id).unwrap().1;
    Payload::FieldModels(Declaration {
        scientific: FieldModelSchema {
            name: "org.orishu.reference.newtonian".parse().unwrap(),
            version: 1.try_into().unwrap(),
            requirements: ["gravity", "mass", "acceleration", "jacobian", "potential"]
                .into_iter()
                .map(|slot| requirement(slot, get(slot)))
                .collect(),
            field: id("gravity"),
            couplings: vec![id("mass")],
            configuration: vec![
                quantity("capacity", Dimension::DIMENSIONLESS, "64"),
                quantity(
                    "gravitational-constant",
                    Dimension::new([3, -1, -2, 0, 0, 0, 0]),
                    "6.67430e-11 m^3 / kg / s^2",
                ),
                quantity("exclusion-radius", Dimension::LENGTH, "0 m"),
                Property {
                    id: id("boundary"),
                    required: true,
                    schema: PropertyType::Text {
                        max_bytes: 32,
                        default: Some("isolated".into()),
                    },
                },
            ],
            state_format: StateFormat {
                id: "org.orishu.reference.newtonian.state".parse().unwrap(),
                version: 1.try_into().unwrap(),
            },
            kernel: code,
            execution_contract: ExecutionContractId::Field,
            observables: ["acceleration", "jacobian", "potential"].map(id).to_vec(),
            profile: ExecutionProfile::ForceThenIntegrate,
            field_time_convention: "advanced field from previous committed object kinematics"
                .into(),
            max_state_bytes: 128 * 1024 * 1024,
        },
        presentation: None,
    })
}
pub fn euler(code: ArtifactDigest) -> Payload {
    let vocabulary = vocabulary();
    let dynamics = &vocabulary
        .iter()
        .find(|(i, _)| i.as_str() == "dynamics")
        .unwrap()
        .1;
    Payload::Integrators(Declaration {
        scientific: IntegratorSchema {
            name: "org.orishu.reference.euler".parse().unwrap(),
            version: 1.try_into().unwrap(),
            requirements: vec![requirement("dynamics", dynamics)],
            dynamics: id("dynamics"),
            configuration: vec![quantity("capacity", Dimension::DIMENSIONLESS, "64")],
            kernel: code,
            execution_contract: ExecutionContractId::Dynamics,
            profile: ExecutionProfile::ForceThenIntegrate,
            history: HistorySchema {
                format: StateFormat {
                    id: "org.orishu.reference.euler.history".parse().unwrap(),
                    version: 1.try_into().unwrap(),
                },
                max_bytes_per_entity: 8,
                samples: 0,
                cold_start: "memoryless integration; retain canonical member IDs".into(),
            },
        },
        presentation: None,
    })
}
