use orishu_plugin::{FiniteF64, execution::*};

fn n(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}
fn kinematics() -> Kinematics {
    Kinematics {
        position_metres: [n(1.0), n(-2.0), n(0.0)],
        velocity_metres_per_second: [n(3.0), n(4.0), n(5.0)],
    }
}
fn bytes<R: BulkRecord>(records: &[R]) -> Vec<u8> {
    let mut bytes = vec![];
    encode_batch(records, &mut bytes, BulkLimits::default()).unwrap();
    bytes
}

#[test]
fn whole_object_packet_keeps_static_state_and_derives_dynamics_without_motion_authority() {
    let records = [
        ObjectState {
            id: EntityId(1),
            kinematics: kinematics(),
            inertial_mass_kilograms: None,
        },
        ObjectState {
            id: EntityId(2),
            kinematics: kinematics(),
            inertial_mass_kilograms: Some(n(4.0)),
        },
    ];
    let encoded = bytes(&records);
    assert_eq!(
        &encoded[..12],
        &[b'O', b'S', b'B', b'1', 5, 0, 0, 0, 2, 0, 0, 0]
    );
    assert_eq!(encoded.len(), 156);
    assert_eq!(&encoded[68..84], &[0; 16]);
    assert_eq!(
        &encoded[140..156],
        &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 64]
    );
    let batch = Batch::<ObjectState>::read(&encoded, BulkLimits::default()).unwrap();
    assert_eq!(batch.iter().collect::<Vec<_>>(), records);
    assert!(batch.get(0).unwrap().dynamic().is_none());
    assert_eq!(
        batch
            .get(1)
            .unwrap()
            .dynamic()
            .unwrap()
            .inertial_mass_kilograms,
        n(4.0)
    );
    for end in 0..encoded.len() {
        assert!(Batch::<ObjectState>::read(&encoded[..end], BulkLimits::default()).is_err());
    }
    for (offset, value) in [(68, 2), (69, 1), (76, 1), (147, 1)] {
        let mut bad = encoded.clone();
        bad[offset] = value;
        assert!(Batch::<ObjectState>::read(&bad, BulkLimits::default()).is_err());
    }
    let mut bad = encoded.clone();
    bad[148..156].fill(0);
    assert!(Batch::<ObjectState>::read(&bad, BulkLimits::default()).is_err());
    let mut output = vec![7];
    let invalid = ObjectState {
        inertial_mass_kilograms: Some(n(0.0)),
        ..records[1]
    };
    assert!(encode_batch(&[invalid], &mut output, BulkLimits::default()).is_err());
    assert_eq!(output, [7]);
}

#[test]
fn exact_portable_force_golden_and_reusable_encoding() {
    let record = Force {
        id: EntityId(5),
        newtons: [n(1.0), n(-2.0), n(0.0)],
    };
    // Independently spelled header, ID and IEEE-754 little-endian bit patterns.
    let golden: &[u8] = &[
        0x4f, 0x53, 0x42, 0x31, 3, 0, 0, 0, 1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0xf0, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(bytes(&[record]), golden);
    let batch = Batch::<Force>::read(golden, BulkLimits::default()).unwrap();
    assert_eq!(batch.get(0), Some(record));
    assert_eq!(batch.get(1), None);
    assert_eq!(batch.iter().collect::<Vec<_>>(), vec![record]);
    assert_eq!(batch.bytes().as_ptr(), golden.as_ptr());
    let mut output = Vec::with_capacity(1000);
    let pointer = output.as_ptr();
    for _ in 0..8 {
        encode_batch(&[record], &mut output, BulkLimits::default()).unwrap();
    }
    assert_eq!(output.as_ptr(), pointer);
    let before = output.clone();
    assert_eq!(
        encode_batch(&[record, record], &mut output, BulkLimits::default()),
        Err(BulkError::EntityOrder)
    );
    assert_eq!(output, before);
}

#[test]
fn role_projections_preserve_independent_strengths_and_static_behavior() {
    let dynamic = DynamicEntity {
        id: EntityId(42),
        kinematics: kinematics(),
        inertial_mass_kilograms: n(7.0),
    };
    let encoded = bytes(&[dynamic]);
    assert_eq!(
        Batch::<DynamicEntity>::read(&encoded, BulkLimits::default())
            .unwrap()
            .get(0),
        Some(dynamic)
    );
    let coupled = CoupledEntity {
        id: EntityId(42),
        slot: CouplingSlot(0),
        kinematics: kinematics(),
        has_dynamics: false,
        source_si: Some(n(-3.0)),
        response_si: Some(n(0.0)),
    };
    assert!(!coupled.responds_dynamically());
    let encoded = bytes(&[coupled]);
    assert_eq!(encoded.len(), HEADER_BYTES + 80);
    assert_eq!(
        Batch::<CoupledEntity>::read(&encoded, BulkLimits::default())
            .unwrap()
            .get(0),
        Some(coupled)
    );
    assert!(
        CoupledEntity {
            has_dynamics: true,
            ..coupled
        }
        .responds_dynamically()
    );
    assert!(
        !CoupledEntity {
            has_dynamics: true,
            response_si: None,
            ..coupled
        }
        .responds_dynamically()
    );
    let mut output = vec![9];
    assert_eq!(
        encode_batch(
            &[DynamicEntity {
                inertial_mass_kilograms: n(0.0),
                ..dynamic
            }],
            &mut output,
            BulkLimits::default()
        ),
        Err(BulkError::InvalidRecord)
    );
    assert_eq!(
        encode_batch(
            &[CoupledEntity {
                source_si: None,
                response_si: None,
                ..coupled
            }],
            &mut output,
            BulkLimits::default()
        ),
        Err(BulkError::InvalidRecord)
    );
    assert_eq!(output, &[9]);
}

#[test]
fn hostile_frames_limits_and_numeric_values_are_refused_before_exposure() {
    let record = Force {
        id: EntityId(1),
        newtons: [n(0.0); 3],
    };
    let encoded = bytes(&[record]);
    for end in 0..encoded.len() {
        assert!(Batch::<Force>::read(&encoded[..end], BulkLimits::default()).is_err());
    }
    let mut bad = encoded.clone();
    bad.push(0);
    assert_eq!(
        Batch::<Force>::read(&bad, BulkLimits::default()).unwrap_err(),
        BulkError::Framing
    );
    for at in [0, 4, 6] {
        let mut bad = encoded.clone();
        bad[at] ^= 0x80;
        assert_eq!(
            Batch::<Force>::read(&bad, BulkLimits::default()).unwrap_err(),
            BulkError::Framing
        );
    }
    let exact = BulkLimits {
        bytes: encoded.len(),
        records: 1,
    };
    assert!(Batch::<Force>::read(&encoded, exact).is_ok());
    assert_eq!(
        Batch::<Force>::read(
            &encoded,
            BulkLimits {
                bytes: encoded.len() - 1,
                ..exact
            }
        )
        .unwrap_err(),
        BulkError::LimitExceeded
    );
    let mut bad = encoded.clone();
    bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Batch::<Force>::read(&bad, exact).unwrap_err(),
        BulkError::LimitExceeded
    );
    assert_eq!(
        packet_bytes::<CoupledEntity>(
            usize::MAX,
            BulkLimits {
                records: usize::MAX,
                bytes: usize::MAX
            }
        ),
        Err(BulkError::LimitExceeded)
    );
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
        let mut bad = encoded.clone();
        bad[HEADER_BYTES + 8..HEADER_BYTES + 16].copy_from_slice(&invalid.to_le_bytes());
        assert_eq!(
            Batch::<Force>::read(&bad, exact).unwrap_err(),
            BulkError::InvalidRecord
        );
    }
    let encoded = bytes(&[
        record,
        Force {
            id: EntityId(2),
            ..record
        },
    ]);
    let mut bad = encoded.clone();
    bad[HEADER_BYTES + Force::BYTES..HEADER_BYTES + Force::BYTES + 8]
        .copy_from_slice(&1_u64.to_le_bytes());
    assert_eq!(
        Batch::<Force>::read(&bad, BulkLimits::default()).unwrap_err(),
        BulkError::EntityOrder
    );
    let empty = bytes::<Force>(&[]);
    assert_eq!(empty.len(), HEADER_BYTES);
    assert!(
        Batch::<Force>::read(&empty, BulkLimits::default())
            .unwrap()
            .is_empty()
    );
    assert!(Batch::<DynamicEntity>::read(&empty, BulkLimits::default()).is_err());
}

#[test]
fn coupling_reserved_flags_and_absent_value_slots_are_canonical() {
    let record = CoupledEntity {
        id: EntityId(1),
        slot: CouplingSlot(0),
        kinematics: kinematics(),
        has_dynamics: false,
        source_si: Some(n(2.0)),
        response_si: None,
    };
    let encoded = bytes(&[record]);
    for at in [HEADER_BYTES + 56, HEADER_BYTES + 57, HEADER_BYTES + 59] {
        let mut bad = encoded.clone();
        bad[at] |= 128;
        assert!(Batch::<CoupledEntity>::read(&bad, BulkLimits::default()).is_err());
    }
    let mut bad = encoded.clone();
    bad[HEADER_BYTES + 72..HEADER_BYTES + 80].copy_from_slice(&1.0_f64.to_le_bytes());
    assert!(Batch::<CoupledEntity>::read(&bad, BulkLimits::default()).is_err());
    let mut bad = encoded;
    bad[HEADER_BYTES + 56] = 0;
    bad[HEADER_BYTES + 64..HEADER_BYTES + 72].fill(0);
    assert!(Batch::<CoupledEntity>::read(&bad, BulkLimits::default()).is_err());
}

#[test]
fn multiple_coupling_slots_remain_independent_but_share_entity_kinematics() {
    let first = CoupledEntity {
        id: EntityId(1),
        slot: CouplingSlot(0),
        kinematics: kinematics(),
        has_dynamics: true,
        source_si: Some(n(5.0)),
        response_si: None,
    };
    let second = CoupledEntity {
        slot: CouplingSlot(7),
        source_si: None,
        response_si: Some(n(-2.0)),
        ..first
    };
    let encoded = bytes(&[first, second]);
    let decoded = Batch::<CoupledEntity>::read(&encoded, BulkLimits::default()).unwrap();
    assert_eq!(decoded.iter().collect::<Vec<_>>(), &[first, second]);
    assert_eq!(
        &encoded
            [HEADER_BYTES + CoupledEntity::BYTES + 60..HEADER_BYTES + CoupledEntity::BYTES + 64],
        &7_u32.to_le_bytes()
    );
    let mut output = vec![9];
    assert_eq!(
        encode_batch(&[first, first], &mut output, BulkLimits::default()),
        Err(BulkError::EntityOrder)
    );
    assert_eq!(
        encode_batch(&[second, first], &mut output, BulkLimits::default()),
        Err(BulkError::EntityOrder)
    );
    assert_eq!(
        encode_batch(
            &[
                first,
                CoupledEntity {
                    has_dynamics: false,
                    ..second
                }
            ],
            &mut output,
            BulkLimits::default()
        ),
        Err(BulkError::InvalidRecord)
    );
    assert_eq!(output, &[9]);
    let mut bad = encoded;
    bad[HEADER_BYTES + CoupledEntity::BYTES + 56] &= !1;
    assert_eq!(
        Batch::<CoupledEntity>::read(&bad, BulkLimits::default()).unwrap_err(),
        BulkError::InvalidRecord
    );
}

#[test]
fn membership_packet_is_a_distinct_complete_sorted_identity_set() {
    let ids = [EntityId(0), EntityId(u64::MAX)];
    let encoded = bytes(&ids);
    assert_eq!(&encoded[..HEADER_BYTES], b"OSB1\x04\0\0\0\x02\0\0\0");
    assert_eq!(
        &encoded[HEADER_BYTES..],
        &[
            0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255
        ]
    );
    assert_eq!(
        Batch::<EntityId>::read(&encoded, BulkLimits::default())
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        ids
    );
    assert!(Batch::<Force>::read(&encoded, BulkLimits::default()).is_err());
    assert!(encode_batch(&[ids[0], ids[0]], &mut Vec::new(), BulkLimits::default()).is_err());
}
