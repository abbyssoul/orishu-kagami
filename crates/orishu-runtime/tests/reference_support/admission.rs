#![allow(dead_code)] // Shared test fixture: each test binary uses a different subset.
//! Real selected releases -> canonical workload -> independent runtime admission.
use super::reference_support::{self, declarations};
use orishu_plugin::{execution::*, resolution::*, selected, workload, *};
use orishu_runtime::*;
use std::{collections::BTreeMap, time::Duration};
pub const GRAVITY: &[u8] = include_bytes!("../fixtures/newtonian.component.wasm");
pub const EULER: &[u8] = include_bytes!("../fixtures/euler.component.wasm");
pub fn n(v: f64) -> FiniteF64 {
    FiniteF64::new(v).unwrap()
}
pub fn control() -> OperationControl {
    // Admission includes native JIT compilation, and these real-Component tests
    // run concurrently. Guest operations retain the host's 5-second deadline;
    // this outer test budget must also allow cold compilation under contention.
    OperationControl::new(Duration::from_secs(60)).unwrap()
}
pub fn borrowed(blobs: &BTreeMap<ArtifactDigest, Vec<u8>>) -> BTreeMap<ArtifactDigest, &[u8]> {
    blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect()
}
pub fn packet<R: BulkRecord>(records: &[R]) -> Buffer {
    let mut bytes = vec![];
    encode_batch(records, &mut bytes, BulkLimits::default()).unwrap();
    Buffer {
        schema: R::SCHEMA.into(),
        value_count: records.len() as u64,
        bytes: bytes.into(),
    }
}
pub fn put(blobs: &mut BTreeMap<ArtifactDigest, Vec<u8>>, buffer: &Buffer) -> InputIdentity {
    let identity = InputIdentity::of(
        buffer.schema.parse().unwrap(),
        buffer.value_count,
        &buffer.bytes,
    );
    blobs.insert(identity.digest, buffer.bytes.to_vec());
    identity
}
pub fn release(
    name: &str,
    payloads: Vec<(LocalContributionId, Payload)>,
    code: &[&[u8]],
    blobs: &mut BTreeMap<ArtifactDigest, Vec<u8>>,
) -> VerifiedRelease {
    let (root, artifacts) = declarations::release(name, payloads, code).unwrap();
    blobs.extend(artifacts);
    VerifiedRelease::verify(root, &borrowed(blobs), &Limits::default()).unwrap()
}
pub struct Fixture {
    pub selection: selected::CompiledSelection,
    pub execution: ExecutionDefinition,
    pub blobs: BTreeMap<ArtifactDigest, Vec<u8>>,
    pub releases: Vec<VerifiedRelease>,
}
impl Fixture {
    pub fn new() -> Self {
        let mut blobs = BTreeMap::new();
        // Independent vocabulary/solver providers. A ships a deliberately unused
        // executable as well; it must never reach the exported workload closure.
        let a = release(
            "org.orishu.reference.vocabulary",
            declarations::vocabulary(),
            &[b"unused native-looking executable"],
            &mut blobs,
        );
        let b = release(
            "org.orishu.reference.solvers",
            vec![
                (
                    "newtonian".parse().unwrap(),
                    declarations::newtonian(ArtifactDigest::sha256_of(GRAVITY)),
                ),
                (
                    "euler".parse().unwrap(),
                    declarations::euler(ArtifactDigest::sha256_of(EULER)),
                ),
            ],
            &[GRAVITY, EULER],
            &mut blobs,
        );
        let roots =
            ["euler", "newtonian"].map(|id| b.contribution_ref(&id.parse().unwrap()).unwrap());
        let entries = [&a, &b].map(|release| InventoryEntry {
            release,
            enabled: true,
            is_default: true,
        });
        let inventory = Inventory::new(1, &entries, Default::default()).unwrap();
        let mut root_list = roots.to_vec();
        root_list.sort();
        let ResolutionOutcome::Resolved { selection, .. } = inventory
            .resolve(&ResolutionRequest {
                expected_inventory_revision: 1,
                roots: root_list,
                bindings: vec![],
            })
            .unwrap()
        else {
            panic!("reference resolves")
        };
        let kernels = [
            selected::SelectedKernel {
                instance_id: "euler".parse().unwrap(),
                contribution: roots[0].clone(),
                execution_contract: ExecutionContractId::Dynamics,
            },
            selected::SelectedKernel {
                instance_id: "newtonian".parse().unwrap(),
                contribution: roots[1].clone(),
                execution_contract: ExecutionContractId::Field,
            },
        ];
        let selection = selected::compile(
            &selection,
            &kernels,
            &[&a, &b],
            &borrowed(&blobs),
            Default::default(),
        )
        .unwrap();
        let gconfig = reference_support::resolved(vec![
            (
                "capacity",
                ConfigurationValue::Quantity {
                    value_si: n(8.0),
                    dimension: Dimension::DIMENSIONLESS,
                },
            ),
            (
                "gravitational-constant",
                ConfigurationValue::Quantity {
                    value_si: n(1.0),
                    dimension: Dimension::new([3, -1, -2, 0, 0, 0, 0]),
                },
            ),
            (
                "exclusion-radius",
                ConfigurationValue::Quantity {
                    value_si: n(0.0),
                    dimension: Dimension::LENGTH,
                },
            ),
            (
                "boundary",
                ConfigurationValue::Text {
                    value: "isolated".into(),
                },
            ),
        ]);
        let econfig = reference_support::euler_config(8);
        let domain = reference_support::domain();
        put(&mut blobs, &gconfig);
        put(&mut blobs, &econfig);
        put(&mut blobs, &domain);
        let mut gcontext =
            reference_support::context(GRAVITY, ExecutionContractId::Field, &gconfig, 8);
        let mut econtext =
            reference_support::context(EULER, ExecutionContractId::Dynamics, &econfig, 8);
        for (c, reference) in [(&mut gcontext, &roots[1]), (&mut econtext, &roots[0])] {
            c.contribution = reference.clone();
            c.scientific = selection.verified().payloads()[reference]
                .contract_ref(&Limits::default())
                .unwrap();
        }
        let mass = selection
            .verified()
            .payloads()
            .iter()
            .find(|(c, _)| c.local_id.as_str() == "mass")
            .unwrap()
            .1;
        gcontext.couplings[0].component = mass.contract_ref(&Limits::default()).unwrap();
        gcontext.couplings[0].source.as_mut().unwrap().property = "source".parse().unwrap();
        gcontext.couplings[0].response.as_mut().unwrap().property = "response".parse().unwrap();
        selected::verify_context(selection.verified(), &gcontext, &Limits::default()).unwrap();
        selected::verify_context(selection.verified(), &econtext, &Limits::default()).unwrap();
        let objects = [
            ObjectState {
                id: EntityId(1),
                kinematics: Kinematics {
                    position_metres: [n(0.0); 3],
                    velocity_metres_per_second: [n(0.0); 3],
                },
                inertial_mass_kilograms: None,
            },
            ObjectState {
                id: EntityId(2),
                kinematics: Kinematics {
                    position_metres: [n(2.0), n(0.0), n(0.0)],
                    velocity_metres_per_second: [n(0.0); 3],
                },
                inertial_mass_kilograms: Some(n(4.0)),
            },
        ];
        let coupled: Vec<_> = objects
            .iter()
            .enumerate()
            .map(|(i, o)| CoupledEntity {
                id: o.id,
                slot: CouplingSlot(0),
                kinematics: o.kinematics,
                has_dynamics: o.inertial_mass_kilograms.is_some(),
                source_si: Some(n(if i == 0 { 2.0 } else { 0.0 })),
                response_si: Some(n(if i == 0 { 0.0 } else { 3.0 })),
            })
            .collect();
        // Authoring capture through actual kernels, with generic host ceilings.
        // No reference-specific state/history sizing formula enters the caller.
        let author = Sandbox::new(SandboxLimits::default()).unwrap();
        let g = author
            .compile(
                GRAVITY,
                ArtifactDigest::sha256_of(GRAVITY),
                ExecutionContractId::Field,
            )
            .unwrap();
        let e = author
            .compile(
                EULER,
                ArtifactDigest::sha256_of(EULER),
                ExecutionContractId::Dynamics,
            )
            .unwrap();
        let FieldResult::State(state) = author
            .invoke_field_bound(
                &g,
                &gcontext,
                gconfig,
                FieldOperation::InitializeBounded {
                    domain,
                    output: OutputCapacity {
                        schema: "org.orishu.reference.newtonian.state/v1",
                        bytes: 1024 * 1024,
                        values: 1024,
                    },
                },
                control(),
            )
            .unwrap()
        else {
            panic!("field capture")
        };
        let DynamicsResult::History(history) = author
            .invoke_dynamics_bound(
                &e,
                &econtext,
                econfig,
                DynamicsOperation::InitializeHistoryBounded {
                    entities: packet(&[objects[1].dynamic().unwrap()]),
                    history: OutputCapacity {
                        schema: "org.orishu.reference.euler.history/v1",
                        bytes: 1024 * 1024,
                        values: 1024,
                    },
                },
                control(),
            )
            .unwrap()
        else {
            panic!("history capture")
        };
        let mut captured = |c: &InstanceContext, state: &Buffer| CapturedKernel {
            instance: c.instance.clone(),
            context: put(
                &mut blobs,
                &Buffer {
                    schema: INSTANCE_SCHEMA.into(),
                    value_count: 1,
                    bytes: c.to_cbor().unwrap().into(),
                },
            ),
            state: put(&mut blobs, state),
        };
        let field = captured(&gcontext, &state);
        let dynamics = captured(&econtext, &history);
        let execution = ExecutionDefinition {
            scene: None,
            api_version: EXECUTION_SCHEMA.parse().unwrap(),
            profile: ExecutionProfile::ForceThenIntegrate,
            timestep_seconds: n(0.5),
            objects: put(&mut blobs, &packet(&objects)),
            fields: vec![CapturedField {
                kernel: field,
                coupled: put(&mut blobs, &packet(&coupled)),
            }],
            dynamics,
        };
        Self {
            selection,
            execution,
            blobs,
            releases: vec![a, b],
        }
    }
    pub fn compile(&self) -> workload::CompiledWorkload {
        workload::compile(
            orishu_workload::WorkloadMeta::new("gravity-demo".parse().unwrap()),
            &self.selection,
            &self.execution,
            &borrowed(&self.blobs),
            Default::default(),
        )
        .unwrap()
    }
    pub fn export(
        &self,
        compiled: &workload::CompiledWorkload,
    ) -> BTreeMap<ArtifactDigest, Vec<u8>> {
        let m = compiled.verified().manifest();
        m.spec
            .compute
            .components
            .iter()
            .map(|c| &c.artifact)
            .chain([&m.spec.selection, &m.spec.execution])
            .chain(&m.spec.artifacts)
            .map(|a| {
                (
                    a.digest,
                    compiled
                        .generated()
                        .get(&a.digest)
                        .or_else(|| self.blobs.get(&a.digest))
                        .unwrap()
                        .clone(),
                )
            })
            .collect()
    }
}
