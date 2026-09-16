//! Actual field/Euler Component composition, not host-native replacement physics.
//! The full admitted workload/run owner remains a separate integration gate.
use orishu_plugin::{ArtifactDigest, ExecutionContractId, FiniteF64, execution::*};
use orishu_runtime::{
    Buffer, CompiledKernel, DynamicsOperation, DynamicsResult, FieldForces, FieldOperation,
    FieldResult, ForceReducer, OperationControl, OutputExtent, ReductionLimits, Sandbox,
    SandboxLimits, StepContext,
};
use orishu_workload::ComponentInstanceId;
use std::time::Duration;
mod reference_support;
const GRAVITY: &[u8] = include_bytes!("fixtures/newtonian.component.wasm");
const EULER: &[u8] = include_bytes!("fixtures/euler.component.wasm");
const STATE: &str = "org.orishu.reference.newtonian.state/v1";
const HISTORY: &str = "org.orishu.reference.euler.history/v1";
fn n(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}
fn kin(x: f64) -> Kinematics {
    Kinematics {
        position_metres: [n(x), n(0.0), n(0.0)],
        velocity_metres_per_second: [n(0.0); 3],
    }
}
fn body(id: u64, mass: f64, x: f64) -> DynamicEntity {
    DynamicEntity {
        id: EntityId(id),
        kinematics: kin(x),
        inertial_mass_kilograms: n(mass),
    }
}
fn coupling(
    id: u64,
    x: f64,
    source: Option<f64>,
    response: Option<f64>,
    dynamic: bool,
) -> CoupledEntity {
    CoupledEntity {
        id: EntityId(id),
        slot: CouplingSlot(0),
        kinematics: kin(x),
        has_dynamics: dynamic,
        source_si: source.map(n),
        response_si: response.map(n),
    }
}
fn raw(schema: &str, bytes: Vec<u8>, count: u64) -> Buffer {
    Buffer {
        schema: schema.into(),
        bytes: bytes.into(),
        value_count: count,
    }
}
fn packet<R: BulkRecord>(values: &[R]) -> Buffer {
    let mut bytes = vec![];
    encode_batch(values, &mut bytes, BulkLimits::default()).unwrap();
    raw(R::SCHEMA, bytes, values.len() as u64)
}
fn shape<R: BulkRecord>(count: usize) -> OutputExtent<'static> {
    OutputExtent {
        schema: R::SCHEMA,
        bytes: packet_bytes::<R>(count, BulkLimits::default()).unwrap(),
        values: count as u64,
    }
}
fn history_shape(count: usize) -> OutputExtent<'static> {
    OutputExtent {
        schema: HISTORY,
        bytes: 16 + 8 * count,
        values: count as u64,
    }
}
fn step(boundary: u64) -> StepContext {
    StepContext {
        workload: "newtonian-proof".into(),
        run: "proof".into(),
        epoch: 0,
        boundary,
        invocation: boundary + 1,
        partition: "whole".into(),
        coverage: "whole".into(),
        time_seconds: boundary as f64 * 0.5,
        dt_seconds: 0.5,
    }
}
fn control() -> OperationControl {
    OperationControl::new(Duration::from_secs(2)).unwrap()
}
struct Fixture {
    host: Sandbox,
    gravity: CompiledKernel,
    euler: CompiledKernel,
    capacity: usize,
    g: f64,
    radius: f64,
}
struct Sampled {
    samples: Buffer,
    query: Buffer,
    context: InstanceContext,
}
impl Fixture {
    fn new(g: f64, radius: f64) -> Self {
        let host = Sandbox::new(SandboxLimits::default()).unwrap();
        let gravity = host
            .compile(
                GRAVITY,
                ArtifactDigest::sha256_of(GRAVITY),
                ExecutionContractId::Field,
            )
            .unwrap();
        let euler = host
            .compile(
                EULER,
                ArtifactDigest::sha256_of(EULER),
                ExecutionContractId::Dynamics,
            )
            .unwrap();
        Self {
            host,
            gravity,
            euler,
            capacity: 8,
            g,
            radius,
        }
    }
    fn state_shape(&self) -> OutputExtent<'static> {
        OutputExtent {
            schema: STATE,
            bytes: 80 + 40 * self.capacity,
            values: 1,
        }
    }
    fn field_config(&self) -> Buffer {
        use orishu_plugin::Dimension;
        reference_support::resolved(vec![
            (
                "capacity",
                ConfigurationValue::Quantity {
                    value_si: n(self.capacity as f64),
                    dimension: Dimension::DIMENSIONLESS,
                },
            ),
            (
                "gravitational-constant",
                ConfigurationValue::Quantity {
                    value_si: n(self.g),
                    dimension: Dimension::new([3, -1, -2, 0, 0, 0, 0]),
                },
            ),
            (
                "exclusion-radius",
                ConfigurationValue::Quantity {
                    value_si: n(self.radius),
                    dimension: Dimension::LENGTH,
                },
            ),
            (
                "boundary",
                ConfigurationValue::Text {
                    value: "isolated".into(),
                },
            ),
        ])
    }
    fn validation(&self, contract: ExecutionContractId, entities: &Buffer, dt: f64) -> Buffer {
        let (code, config) = if contract == ExecutionContractId::Field {
            (GRAVITY, self.field_config())
        } else {
            (EULER, reference_support::euler_config(self.capacity as u32))
        };
        let ctx = reference_support::context(code, contract, &config, self.capacity as u32);
        reference_support::validation(&ctx, &config, entities, dt)
    }
    fn field(&self, op: FieldOperation<'_>) -> wasmtime::Result<FieldResult> {
        let config = self.field_config();
        let ctx = reference_support::context(
            GRAVITY,
            ExecutionContractId::Field,
            &config,
            self.capacity as u32,
        );
        self.host
            .invoke_field_bound(&self.gravity, &ctx, config, op, control())
    }
    fn dynamics(&self, op: DynamicsOperation<'_>) -> wasmtime::Result<DynamicsResult> {
        let config = reference_support::euler_config(self.capacity as u32);
        let ctx = reference_support::context(
            EULER,
            ExecutionContractId::Dynamics,
            &config,
            self.capacity as u32,
        );
        self.host
            .invoke_dynamics_bound(&self.euler, &ctx, config, op, control())
    }
    fn initialize(&self) -> Buffer {
        state(
            self.field(FieldOperation::Initialize {
                domain: reference_support::domain(),
                output: self.state_shape(),
            })
            .unwrap(),
        )
    }
    fn advance(
        &self,
        prior: Buffer,
        coupled: Buffer,
        context: &StepContext,
    ) -> wasmtime::Result<(Buffer, Buffer)> {
        let count = coupled
            .scientific_batch::<CoupledEntity>(BulkLimits::default())
            .unwrap()
            .iter()
            .filter(CoupledEntity::responds_dynamically)
            .count();
        match self.field(FieldOperation::Advance {
            step: context,
            prior,
            validation: self.validation(ExecutionContractId::Field, &coupled, context.dt_seconds),
            entities: coupled,
            field: self.state_shape(),
            forces: shape::<Force>(count),
        })? {
            FieldResult::Advanced { field, forces } => Ok((field, forces)),
            _ => panic!("expected field and force"),
        }
    }
    fn sample(&self, snapshot: Buffer, points: &[(u64, [f64; 3])]) -> wasmtime::Result<Sampled> {
        let channels = reference_support::gravity_channels::bindings();
        self.sample_channels(
            snapshot,
            points,
            [0, 2, 1].map(|i| channels[i].channel.clone()).to_vec(),
        )
    }
    fn sample_channels(
        &self,
        snapshot: Buffer,
        points: &[(u64, [f64; 3])],
        channels: Vec<SampleChannel>,
    ) -> wasmtime::Result<Sampled> {
        let config = self.field_config();
        let context = reference_support::context(
            GRAVITY,
            ExecutionContractId::Field,
            &config,
            self.capacity as u32,
        );
        let metadata = SampleMetadata {
            api_version: SAMPLE_METADATA_SCHEMA.parse().unwrap(),
            request_id: 1,
            snapshot: SampleSnapshot {
                source: SnapshotSource::Authored {
                    revision: ArtifactDigest::sha256_of(b"reference authored fixture"),
                },
                state: InputIdentity::of(
                    snapshot.schema.parse().unwrap(),
                    snapshot.value_count,
                    &snapshot.bytes,
                ),
            },
            field: context.instance.clone(),
            context: ArtifactDigest::sha256_of(&context.to_cbor().unwrap()),
            channels,
        };
        let points: Vec<_> = points
            .iter()
            .map(|(id, p)| SamplePoint {
                id: *id,
                position_metres: p.map(n),
            })
            .collect();
        let mut bytes = vec![];
        let mut scratch = SampleScratch::default();
        encode_sample_request(
            &metadata,
            &points,
            &mut bytes,
            &mut scratch,
            SampleLimits::default(),
        )?;
        let layout = SampleRequest::read(&bytes, &mut scratch, SampleLimits::default())?
            .layout(SampleLimits::default())?;
        let query = raw(SAMPLE_REQUEST_SCHEMA, bytes, points.len() as u64);
        match self.field(FieldOperation::Sample {
            snapshot,
            query: query.clone(),
            output: OutputExtent {
                schema: SAMPLE_RESPONSE_SCHEMA,
                bytes: layout.bytes,
                values: points.len() as u64,
            },
        })? {
            FieldResult::Samples(samples) => Ok(Sampled {
                samples,
                query,
                context,
            }),
            _ => panic!("expected samples"),
        }
    }
    fn initial_history(&self, entities: &Buffer) -> Buffer {
        history(
            self.dynamics(DynamicsOperation::InitializeHistory {
                entities: entities.clone(),
                history: history_shape(entities.value_count as usize),
            })
            .unwrap(),
        )
    }
    // Candidate-only composition for numerical evidence. No product run owner,
    // workload admission or host-owned physics is hidden in this test helper.
    fn coupled_step(
        &self,
        entities: &Buffer,
        prior: &Buffer,
        prior_history: &Buffer,
        boundary: u64,
    ) -> (Buffer, Buffer, Buffer) {
        let coupling: Vec<_> = entities
            .scientific_batch::<DynamicEntity>(BulkLimits::default())
            .unwrap()
            .iter()
            .map(|e| CoupledEntity {
                id: e.id,
                slot: CouplingSlot(0),
                kinematics: e.kinematics,
                has_dynamics: true,
                source_si: Some(n(if e.id.0 == 1 { 2.0 } else { 3.0 })),
                response_si: Some(n(if e.id.0 == 1 { 2.0 } else { 3.0 })),
            })
            .collect();
        let coupled = packet(&coupling);
        let context = step(boundary);
        let (field, forces) = self
            .advance(prior.clone(), coupled.clone(), &context)
            .unwrap();
        let id = ComponentInstanceId::new("gravity").unwrap();
        let mut reducer = ForceReducer::default();
        let reduced = reducer
            .reduce(
                &entities
                    .scientific_batch::<DynamicEntity>(BulkLimits::default())
                    .unwrap(),
                std::slice::from_ref(&id),
                &[FieldForces {
                    instance: &id,
                    coupling_slots: 1,
                    coupled: coupled.scientific_batch(BulkLimits::default()).unwrap(),
                    forces: forces.scientific_batch(BulkLimits::default()).unwrap(),
                }],
                ReductionLimits::default(),
            )
            .unwrap();
        match self
            .dynamics(DynamicsOperation::Integrate {
                step: &context,
                entities: entities.clone(),
                forces: packet(reduced),
                prior_history: prior_history.clone(),
                validation: self.validation(
                    ExecutionContractId::Dynamics,
                    entities,
                    context.dt_seconds,
                ),
                next_entities: shape::<DynamicEntity>(entities.value_count as usize),
                history: history_shape(entities.value_count as usize),
            })
            .unwrap()
        {
            DynamicsResult::Integrated { entities, history } => (entities, field, history),
            _ => panic!("expected integrated output"),
        }
    }
}
fn state(result: FieldResult) -> Buffer {
    match result {
        FieldResult::State(state) => state,
        _ => panic!("expected state"),
    }
}
fn history(result: DynamicsResult) -> Buffer {
    match result {
        DynamicsResult::History(state) => state,
        _ => panic!("expected history"),
    }
}
fn sample_row(sampled: &Sampled, index: usize) -> (u64, [u32; 3], [u32; 3], [f64; 13]) {
    let query = SampleRequest::read(
        &sampled.query.bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let response = SampleResponse::read(
        &sampled.samples.bytes,
        &query,
        &sampled.context,
        SampleLimits::default(),
    )
    .unwrap();
    let id = query.points().nth(index).unwrap().id;
    let mut status = [0; 3];
    let mut quality = [0; 3];
    let mut values = [0.0; 13];
    for (channel, offset) in [0, 3, 4].into_iter().enumerate() {
        match response.cell(index, channel).unwrap() {
            SampleCell::Valid {
                values: row,
                quality_flags,
            } => {
                quality[channel] = quality_flags;
                for (i, value) in row.iter().enumerate() {
                    values[offset + i] = value.get();
                }
            }
            SampleCell::Invalid(reason) => status[channel] = reason as u32,
        }
    }
    (id, status, quality, values)
}

#[test]
fn field_state_config_domain_and_padding_are_checked_before_returning_candidates() {
    let fixture = Fixture::new(1.0, 0.0);
    let initial = fixture.initialize();
    let empty = packet::<CoupledEntity>(&[]);
    for (offset, value) in [(4, 0xff_u8), (8, 0xff), (24, 1), (72, 9), (76, 1), (80, 1)] {
        let mut corrupt = initial.bytes.to_vec();
        corrupt[offset] = value;
        let corrupt = raw(STATE, corrupt, 1);
        assert!(
            fixture.advance(corrupt, empty.clone(), &step(0)).is_err(),
            "offset {offset}"
        );
    }
    // Editing the domain requires explicit reinitialization and a new context,
    // never reusing a state whose opaque embedded domain has changed.
    let mut other_domain = reference_support::domain();
    let mut descriptor =
        DomainDescriptor::from_cbor(&other_domain.bytes, DomainLimits::default()).unwrap();
    descriptor.lower_metres[0] = n(-5.0);
    other_domain.bytes = descriptor.to_cbor(DomainLimits::default()).unwrap().into();
    assert!(
        fixture
            .field(FieldOperation::Initialize {
                domain: other_domain,
                output: fixture.state_shape()
            })
            .is_err()
    );
    let (next, forces) = fixture.advance(initial.clone(), empty, &step(0)).unwrap();
    assert_eq!(next.bytes, initial.bytes);
    assert_eq!(forces.value_count, 0);
}

#[test]
fn real_kernel_refuses_incompatible_configuration_dimensions_boundaries_and_grid() {
    use orishu_runtime::KernelRejection;
    let fixture = Fixture::new(1.0, 0.0);
    let initial = fixture.initialize();
    let config = fixture.field_config();
    let base =
        ResolvedConfiguration::from_cbor(&config.bytes, &orishu_plugin::Limits::default()).unwrap();
    for (key, value, expected) in [
        (
            "boundary",
            ConfigurationValue::Text {
                value: "periodic".into(),
            },
            KernelRejection::Unsupported,
        ),
        (
            "capacity",
            ConfigurationValue::Quantity {
                value_si: n(1.5),
                dimension: orishu_plugin::Dimension::DIMENSIONLESS,
            },
            KernelRejection::InvalidInput,
        ),
        (
            "gravitational-constant",
            ConfigurationValue::Quantity {
                value_si: n(1.0),
                dimension: orishu_plugin::Dimension::LENGTH,
            },
            KernelRejection::InvalidInput,
        ),
    ] {
        let mut changed = base.clone();
        changed
            .properties
            .iter_mut()
            .find(|p| p.id.as_str() == key)
            .unwrap()
            .value = value;
        let changed = raw(
            CONFIGURATION_SCHEMA,
            changed.to_cbor(&orishu_plugin::Limits::default()).unwrap(),
            1,
        );
        let ctx = reference_support::context(
            GRAVITY,
            ExecutionContractId::Field,
            &changed,
            fixture.capacity as u32,
        );
        let error = fixture
            .host
            .invoke_field_bound(
                &fixture.gravity,
                &ctx,
                changed,
                FieldOperation::Initialize {
                    domain: reference_support::domain(),
                    output: fixture.state_shape(),
                },
                control(),
            )
            .unwrap_err();
        assert_eq!(error.downcast_ref::<KernelRejection>(), Some(&expected));
    }
    let mut descriptor =
        DomainDescriptor::from_cbor(&reference_support::domain().bytes, DomainLimits::default())
            .unwrap();
    descriptor.discretization = SpatialDiscretization::CartesianCells { cells: [2, 2, 2] };
    let domain = raw(
        DOMAIN_SCHEMA,
        descriptor.to_cbor(DomainLimits::default()).unwrap(),
        1,
    );
    let mut ctx = reference_support::context(
        GRAVITY,
        ExecutionContractId::Field,
        &config,
        fixture.capacity as u32,
    );
    ctx.domain = InputIdentity::of(DOMAIN_SCHEMA.parse().unwrap(), 1, &domain.bytes);
    let error = fixture
        .host
        .invoke_field_bound(
            &fixture.gravity,
            &ctx,
            config,
            FieldOperation::Initialize {
                domain,
                output: fixture.state_shape(),
            },
            control(),
        )
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<KernelRejection>(),
        Some(&KernelRejection::Unsupported)
    );
    assert_eq!(fixture.initialize().bytes, initial.bytes);
}

#[test]
fn natural_state_and_direct_field_potential_jacobian_preserve_validity_and_point_order() {
    let fixture = Fixture::new(1.0, 0.25);
    let initial = fixture.initialize();
    let natural = fixture.sample(initial.clone(), &[(1, [0.0; 3])]).unwrap();
    assert_eq!(sample_row(&natural, 0), (1, [0; 3], [1; 3], [0.0; 13]));
    let (field, forces) = fixture
        .advance(
            initial.clone(),
            packet(&[coupling(1, 0.0, Some(2.0), None, false)]),
            &step(0),
        )
        .unwrap();
    assert!(
        forces
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .is_empty()
    );
    let sampled = fixture
        .sample(
            field.clone(),
            &[
                (99, [2.0, 0.0, 0.0]),
                (1, [0.0; 3]),
                (77, [20.0, 0.0, 0.0]),
                (5, [0.1, 0.0, 0.0]),
            ],
        )
        .unwrap();
    let (id, status, quality, value) = sample_row(&sampled, 0);
    assert_eq!((id, status, quality), (99, [0; 3], [1; 3]));
    assert_eq!(
        value,
        [
            -0.5, 0.0, 0.0, -1.0, 0.5, 0.0, 0.0, 0.0, -0.25, 0.0, 0.0, 0.0, -0.25
        ]
    );
    assert_eq!(
        (
            sample_row(&sampled, 1).0,
            sample_row(&sampled, 1).1,
            sample_row(&sampled, 1).2
        ),
        (1, [SampleInvalidity::Singular as u32; 3], [0; 3])
    );
    assert_eq!(sample_row(&sampled, 2).1, [1; 3]);
    assert_eq!(
        sample_row(&sampled, 3).1,
        [SampleInvalidity::Singular as u32; 3]
    );
    assert!(
        fixture
            .sample(field.clone(), &[(1, [2.0, 0.0, 0.0]), (1, [3.0, 0.0, 0.0])])
            .is_err()
    );
    assert_ne!(field.bytes, initial.bytes);
    assert_eq!(
        sample_row(
            &fixture.sample(initial, &[(1, [2.0, 0.0, 0.0])]).unwrap(),
            0
        )
        .3,
        [0.0; 13]
    );
}

#[test]
fn requested_channel_subset_order_and_unavailability_work_through_the_real_guest() {
    let fixture = Fixture::new(1.0, 0.0);
    let (field, _) = fixture
        .advance(
            fixture.initialize(),
            packet(&[coupling(1, 0.0, Some(2.0), None, false)]),
            &step(0),
        )
        .unwrap();
    let channels = reference_support::gravity_channels::bindings();
    let mut unavailable = channels[2].channel.clone();
    unavailable.schema.name = "org.test.another-potential".parse().unwrap();
    unavailable.contract = orishu_plugin::Payload::Observables(orishu_plugin::Declaration {
        scientific: unavailable.schema.clone(),
        presentation: None,
    })
    .contract_ref(&orishu_plugin::Limits::default())
    .unwrap();
    for selection in [
        vec![channels[2].channel.clone()],
        vec![unavailable, channels[0].channel.clone()],
    ] {
        let sampled = fixture
            .sample_channels(field.clone(), &[(91, [2.0, 0.0, 0.0])], selection.clone())
            .unwrap();
        let request = SampleRequest::read(
            &sampled.query.bytes,
            &mut SampleScratch::default(),
            SampleLimits::default(),
        )
        .unwrap();
        let response = SampleResponse::read(
            &sampled.samples.bytes,
            &request,
            &sampled.context,
            SampleLimits::default(),
        )
        .unwrap();
        assert_eq!(request.metadata().channels, selection);
        if selection.len() == 1 {
            assert_eq!(sampled.samples.bytes.len(), 56);
            match response.cell(0, 0).unwrap() {
                SampleCell::Valid {
                    values,
                    quality_flags,
                } => {
                    assert_eq!(
                        values.iter().map(FiniteF64::get).collect::<Vec<_>>(),
                        [-1.0]
                    );
                    assert_eq!(quality_flags, 1);
                }
                _ => panic!("potential must be available"),
            }
        } else {
            assert!(matches!(
                response.cell(0, 0),
                Some(SampleCell::Invalid(SampleInvalidity::ChannelUnavailable))
            ));
            match response.cell(0, 1).unwrap() {
                SampleCell::Valid { values, .. } => assert_eq!(
                    values.iter().map(FiniteF64::get).collect::<Vec<_>>(),
                    [-0.5, 0.0, 0.0]
                ),
                _ => panic!("acceleration must remain available"),
            }
        }
    }
}

#[test]
fn bound_sampling_refuses_state_context_descriptor_and_extent_substitutions() {
    let fixture = Fixture::new(1.0, 0.0);
    let state = fixture.initialize();
    let sampled = fixture
        .sample(state.clone(), &[(1, [2.0, 0.0, 0.0])])
        .unwrap();
    let request = SampleRequest::read(
        &sampled.query.bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    let other = fixture
        .advance(
            state.clone(),
            packet(&[coupling(1, 0.0, Some(2.0), None, false)]),
            &step(0),
        )
        .unwrap()
        .0;
    for case in 0..6 {
        let mut snapshot = state.clone();
        let mut query = sampled.query.clone();
        let mut output = OutputExtent {
            schema: SAMPLE_RESPONSE_SCHEMA,
            bytes: sampled.samples.bytes.len(),
            values: 1,
        };
        match case {
            0 => snapshot = other.clone(),
            1 => query.schema = "org.test.query/v1".into(),
            2 => query.value_count = 0,
            3 => output.bytes += 8,
            4 => output.schema = "org.test.samples/v1",
            _ => {
                let mut meta = request.metadata().clone();
                meta.context = ArtifactDigest::sha256_of(b"another context");
                let mut bytes = vec![];
                encode_sample_request(
                    &meta,
                    &request.points().collect::<Vec<_>>(),
                    &mut bytes,
                    &mut SampleScratch::default(),
                    SampleLimits::default(),
                )
                .unwrap();
                query.bytes = bytes.into();
            }
        }
        let error = fixture
            .field(FieldOperation::Sample {
                snapshot,
                query,
                output,
            })
            .expect_err("substitution refused");
        assert!(
            error
                .downcast_ref::<orishu_runtime::ContextRejection>()
                .is_some(),
            "case {case}: {error}"
        );
    }
    // A rejected observation does not modify the retained immutable scientific state.
    assert_eq!(
        sample_row(&fixture.sample(state, &[(1, [2.0, 0.0, 0.0])]).unwrap(), 0).3,
        [0.0; 13]
    );
}

#[test]
fn force_uses_independent_gravitational_response_and_excludes_only_self() {
    let fixture = Fixture::new(6.67430e-11, 0.0);
    let initial = fixture.initialize();
    let (_, forces) = fixture
        .advance(
            initial.clone(),
            packet(&[
                coupling(1, -1.0, Some(2.0), Some(2.0), true),
                coupling(2, 1.0, Some(3.0), Some(3.0), true),
            ]),
            &step(0),
        )
        .unwrap();
    let rows: Vec<_> = forces
        .scientific_batch::<Force>(BulkLimits::default())
        .unwrap()
        .iter()
        .collect();
    let expected = 6.67430e-11 * 6.0 / 4.0;
    assert!((rows[0].newtons[0].get() - expected).abs() < 1e-24);
    assert_eq!(rows[0].newtons[0].get(), -rows[1].newtons[0].get());
    let (_, forces) = fixture
        .advance(
            initial.clone(),
            packet(&[coupling(1, 0.0, Some(2.0), Some(9.0), true)]),
            &step(0),
        )
        .unwrap();
    assert_eq!(
        forces
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap()
            .newtons,
        [n(0.0); 3]
    );
    let (_, forces) = fixture
        .advance(
            initial.clone(),
            packet(&[
                coupling(1, 0.0, Some(2.0), None, false),
                coupling(2, 2.0, None, Some(7.0), true),
            ]),
            &step(0),
        )
        .unwrap();
    assert!(
        (forces
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap()
            .newtons[0]
            .get()
            + fixture.g * 3.5)
            .abs()
            < 1e-24
    );
    assert!(
        fixture
            .advance(
                initial,
                packet(&[
                    coupling(1, 0.0, Some(2.0), None, false),
                    coupling(2, 0.0, None, Some(1.0), true)
                ]),
                &step(0)
            )
            .is_err()
    );
}

#[test]
fn real_field_reduction_and_euler_compose_and_restart_in_a_new_engine() {
    let fixture = Fixture::new(1.0, 0.0);
    let initial = packet(&[body(1, 4.0, -1.0), body(2, 3.0, 1.0)]);
    let initial_field = fixture.initialize();
    let initial_history = fixture.initial_history(&initial);
    let (entities, field, history_state) =
        fixture.coupled_step(&initial, &initial_field, &initial_history, 0);
    let rows: Vec<_> = entities
        .scientific_batch::<DynamicEntity>(BulkLimits::default())
        .unwrap()
        .iter()
        .collect();
    assert_eq!(rows[0].kinematics.position_metres[0], n(-0.90625));
    assert_eq!(rows[0].kinematics.velocity_metres_per_second[0], n(0.1875));
    assert_eq!(rows[1].kinematics.position_metres[0], n(0.875));
    assert_eq!(rows[1].kinematics.velocity_metres_per_second[0], n(-0.25));
    let checkpoint = state(
        fixture
            .field(FieldOperation::Checkpoint {
                state: field.clone(),
                output: fixture.state_shape(),
            })
            .unwrap(),
    );
    let history_checkpoint = history(
        fixture
            .dynamics(DynamicsOperation::Checkpoint {
                history: history_state.clone(),
                output: history_shape(2),
            })
            .unwrap(),
    );
    let expected = fixture.coupled_step(&entities, &field, &history_state, 1);
    let restarted = Fixture::new(1.0, 0.0);
    let projection = packet(
        &rows
            .iter()
            .map(|e| CoupledEntity {
                id: e.id,
                slot: CouplingSlot(0),
                kinematics: e.kinematics,
                has_dynamics: true,
                source_si: Some(n(if e.id.0 == 1 { 2.0 } else { 3.0 })),
                response_si: Some(n(if e.id.0 == 1 { 2.0 } else { 3.0 })),
            })
            .collect::<Vec<_>>(),
    );
    let restored = state(
        restarted
            .field(FieldOperation::Restore {
                checkpoint,
                validation: restarted.validation(ExecutionContractId::Field, &projection, 0.5),
                output: restarted.state_shape(),
            })
            .unwrap(),
    );
    let restored_history = history(
        restarted
            .dynamics(DynamicsOperation::Restore {
                checkpoint: history_checkpoint,
                validation: restarted.validation(ExecutionContractId::Dynamics, &entities, 0.5),
                output: history_shape(2),
            })
            .unwrap(),
    );
    let actual = restarted.coupled_step(&entities, &restored, &restored_history, 1);
    assert_eq!(actual.0.bytes, expected.0.bytes);
    assert_eq!(actual.1.bytes, expected.1.bytes);
    assert_eq!(actual.2.bytes, expected.2.bytes);
    assert_ne!(initial_field.bytes, field.bytes);
}

#[test]
fn optional_jacobian_overflow_does_not_poison_valid_force_or_other_channels() {
    let fixture = Fixture::new(1.0, 0.0);
    let (field, forces) = fixture
        .advance(
            fixture.initialize(),
            packet(&[
                coupling(1, 0.0, Some(1.0), None, false),
                coupling(2, 1e-120, None, Some(1.0), true),
            ]),
            &step(0),
        )
        .unwrap();
    assert!(
        forces
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .get(0)
            .unwrap()
            .newtons[0]
            .get()
            .is_finite()
    );
    let sampled = fixture.sample(field, &[(1, [1e-120, 0.0, 0.0])]).unwrap();
    let (_, status, quality, value) = sample_row(&sampled, 0);
    assert_eq!(status, [0, 0, SampleInvalidity::Undefined as u32]);
    assert_eq!(quality, [1, 1, 0]);
    assert!(value[0].is_finite() && value[0] < 0.0);
    assert!(value[3].is_finite() && value[3] < 0.0);
}
