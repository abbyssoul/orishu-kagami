use orishu_plugin::{FiniteF64, execution::*};
use orishu_runtime::{FieldForces, ForceReducer, ReductionError, ReductionLimits};
use orishu_workload::ComponentInstanceId;

fn n(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}
fn kinematics() -> Kinematics {
    Kinematics {
        position_metres: [n(0.0); 3],
        velocity_metres_per_second: [n(0.0); 3],
    }
}
fn entity(id: u64) -> DynamicEntity {
    DynamicEntity {
        id: EntityId(id),
        kinematics: kinematics(),
        inertial_mass_kilograms: n(1.0),
    }
}
fn coupled(id: u64) -> CoupledEntity {
    CoupledEntity {
        id: EntityId(id),
        slot: CouplingSlot(0),
        kinematics: kinematics(),
        has_dynamics: true,
        source_si: Some(n(1.0)),
        response_si: Some(n(1.0)),
    }
}
fn force(id: u64, x: f64) -> Force {
    Force {
        id: EntityId(id),
        newtons: [n(x), n(0.0), n(0.0)],
    }
}
fn encode<R: BulkRecord>(records: &[R]) -> Vec<u8> {
    let mut output = vec![];
    encode_batch(records, &mut output, BulkLimits::default()).unwrap();
    output
}
struct Field {
    id: ComponentInstanceId,
    coupled: Vec<u8>,
    forces: Vec<u8>,
}
impl Field {
    fn new(id: &str, coupled: &[CoupledEntity], forces: &[Force]) -> Self {
        Self {
            id: ComponentInstanceId::new(id).unwrap(),
            coupled: encode(coupled),
            forces: encode(forces),
        }
    }
    fn view(&self) -> FieldForces<'_> {
        FieldForces {
            instance: &self.id,
            coupling_slots: 1,
            coupled: Batch::read(&self.coupled, BulkLimits::default()).unwrap(),
            forces: Batch::read(&self.forces, BulkLimits::default()).unwrap(),
        }
    }
}

#[test]
fn reductions_follow_admitted_field_order_not_completion_order_and_reuse_capacity() {
    let bytes = encode(&[entity(1), entity(2)]);
    let entities = Batch::read(&bytes, BulkLimits::default()).unwrap();
    let fields = [
        Field::new("a", &[coupled(1)], &[force(1, 1e16)]),
        Field::new("b", &[coupled(1)], &[force(1, -1e16)]),
        Field::new("c", &[coupled(1)], &[force(1, 1.0)]),
    ];
    let selected: Vec<_> = fields.iter().map(|f| f.id.clone()).collect();
    let mut reducer = ForceReducer::default();
    let mut address = None;
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let input = order.map(|i| fields[i].view());
        let result = reducer
            .reduce(&entities, &selected, &input, ReductionLimits::default())
            .unwrap();
        assert_eq!(result, &[force(1, 1.0), force(2, 0.0)]);
        if let Some(previous) = address {
            assert_eq!(result.as_ptr(), previous);
        }
        address = Some(result.as_ptr());
    }
    let zero = reducer
        .reduce(&entities, &[], &[], ReductionLimits::default())
        .unwrap();
    assert_eq!(zero, &[force(1, 0.0), force(2, 0.0)]);
}

#[test]
fn every_selected_field_and_dynamic_response_must_be_present_exactly() {
    let bytes = encode(&[entity(1)]);
    let entities = Batch::read(&bytes, BulkLimits::default()).unwrap();
    let valid = Field::new("a", &[coupled(1)], &[force(1, 0.0)]);
    let selected = [valid.id.clone()];
    let mut reducer = ForceReducer::default();
    assert_eq!(
        reducer
            .reduce(&entities, &selected, &[], ReductionLimits::default())
            .unwrap_err(),
        ReductionError::FieldCoverage
    );
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &selected,
                &[valid.view(), valid.view()],
                ReductionLimits::default()
            )
            .unwrap_err(),
        ReductionError::FieldCoverage
    );
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &[valid.id.clone(), valid.id.clone()],
                &[valid.view(), valid.view()],
                ReductionLimits::default()
            )
            .unwrap_err(),
        ReductionError::FieldCoverage
    );
    let unexpected = Field::new("b", &[coupled(1)], &[force(1, 0.0)]);
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &selected,
                &[unexpected.view()],
                ReductionLimits::default()
            )
            .unwrap_err(),
        ReductionError::FieldCoverage
    );
    for field in [
        Field::new("a", &[coupled(1)], &[]),
        Field::new("a", &[coupled(1)], &[force(2, 0.0)]),
        Field::new("a", &[], &[force(1, 0.0)]),
        Field::new(
            "a",
            &[CoupledEntity {
                response_si: Some(n(0.0)),
                ..coupled(1)
            }],
            &[],
        ),
    ] {
        assert_eq!(
            reducer
                .reduce(
                    &entities,
                    &selected,
                    &[field.view()],
                    ReductionLimits::default()
                )
                .unwrap_err(),
            ReductionError::ResponseCoverage
        );
    }
    for c in [
        coupled(2),
        CoupledEntity {
            has_dynamics: false,
            ..coupled(1)
        },
        CoupledEntity {
            kinematics: Kinematics {
                position_metres: [n(1.0); 3],
                ..kinematics()
            },
            ..coupled(1)
        },
    ] {
        let field = Field::new("a", &[c], &[]);
        assert_eq!(
            reducer
                .reduce(
                    &entities,
                    &selected,
                    &[field.view()],
                    ReductionLimits::default()
                )
                .unwrap_err(),
            ReductionError::EntityProjection
        );
    }
    // Source-only dynamic and static entities participate in fields but emit no forces.
    let source_only = Field::new(
        "a",
        &[
            CoupledEntity {
                response_si: None,
                ..coupled(1)
            },
            CoupledEntity {
                has_dynamics: false,
                ..coupled(2)
            },
        ],
        &[],
    );
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &selected,
                &[source_only.view()],
                ReductionLimits::default()
            )
            .unwrap(),
        &[force(1, 0.0)]
    );
    assert!(
        reducer
            .reduce(
                &entities,
                &selected,
                &[valid.view()],
                ReductionLimits::default()
            )
            .is_ok()
    );
}

#[test]
fn aggregate_limits_and_nonfinite_intermediate_sums_reject_without_poisoning_next_operation() {
    let bytes = encode(&[entity(1)]);
    let entities = Batch::read(&bytes, BulkLimits::default()).unwrap();
    let fields = [
        Field::new("a", &[coupled(1)], &[force(1, f64::MAX)]),
        Field::new("b", &[coupled(1)], &[force(1, f64::MAX)]),
    ];
    let selected = fields.each_ref().map(|f| f.id.clone());
    let views = fields.each_ref().map(|f| f.view());
    let mut reducer = ForceReducer::default();
    assert_eq!(
        reducer
            .reduce(&entities, &selected, &views, ReductionLimits::default())
            .unwrap_err(),
        ReductionError::NonFiniteSum
    );
    for limits in [
        ReductionLimits {
            fields: 1,
            ..ReductionLimits::default()
        },
        ReductionLimits {
            entities: 0,
            ..ReductionLimits::default()
        },
        ReductionLimits {
            field_records: 3,
            ..ReductionLimits::default()
        },
    ] {
        assert_eq!(
            reducer
                .reduce(&entities, &selected, &views, limits)
                .unwrap_err(),
            ReductionError::LimitExceeded
        );
    }
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &selected[..1],
                &views[..1],
                ReductionLimits {
                    fields: 1,
                    entities: 1,
                    field_records: 2
                }
            )
            .unwrap(),
        &[force(1, f64::MAX)]
    );
}

#[test]
fn several_response_slots_require_one_combined_force_per_entity_from_each_field() {
    let bytes = encode(&[entity(1)]);
    let entities = Batch::read(&bytes, BulkLimits::default()).unwrap();
    let field = Field::new(
        "a",
        &[
            coupled(1),
            CoupledEntity {
                slot: CouplingSlot(1),
                ..coupled(1)
            },
        ],
        &[force(1, 9.0)],
    );
    let mut input = field.view();
    let selected = [field.id.clone()];
    let mut reducer = ForceReducer::default();
    assert_eq!(
        reducer
            .reduce(
                &entities,
                &selected,
                &[field.view()],
                ReductionLimits::default()
            )
            .unwrap_err(),
        ReductionError::CouplingSlot
    );
    input.coupling_slots = 2;
    // The kernel combined its response slots. The runtime must not double the force.
    assert_eq!(
        reducer
            .reduce(&entities, &selected, &[input], ReductionLimits::default())
            .unwrap(),
        &[force(1, 9.0)]
    );
}

#[test]
fn a_valid_packet_cannot_hide_a_mismatched_host_descriptor() {
    use orishu_runtime::{Buffer, ScientificBufferError};
    let bytes = encode(&[force(1, 2.0)]);
    let mut buffer = Buffer {
        schema: Force::SCHEMA.into(),
        bytes: bytes.into(),
        value_count: 1,
    };
    assert_eq!(
        buffer
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap()
            .get(0),
        Some(force(1, 2.0))
    );
    buffer.value_count = 0;
    assert_eq!(
        buffer
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap_err(),
        ScientificBufferError::Count
    );
    buffer.value_count = 1;
    buffer.schema = DynamicEntity::SCHEMA.into();
    assert_eq!(
        buffer
            .scientific_batch::<Force>(BulkLimits::default())
            .unwrap_err(),
        ScientificBufferError::Schema
    );
}
