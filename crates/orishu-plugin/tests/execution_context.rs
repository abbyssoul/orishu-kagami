use orishu_plugin::{ArtifactDigest, ExecutionContractId, FiniteF64, execution::*};

fn context() -> InstanceContext {
    let digest = ArtifactDigest::sha256_of(b"fixture").to_string();
    let fixture = serde_json::json!({
        "apiVersion":INSTANCE_SCHEMA,"instance":"dynamics","kernel":digest,
        "contribution":{"release":digest,"extensionPoint":"orishu.compute.integrators/v1","localId":"euler"},
        "scientific":{"name":"org.test.euler","version":1,"digest":digest},
        "executionContract":"orishu:simulation/dynamics@1",
        "stateFormat":{"id":"org.test.history","version":1},
        "profile":"orishu.force-then-integrate/v1",
        "configuration":{"schema":"org.test.config/v1","byteLength":3,"valueCount":1,"digest":digest},
        "domain":{"schema":"org.test.domain/v1","byteLength":6,"valueCount":1,"digest":digest},
        "couplings":[],"observables":[],"computePrecision":"binary64","bounds":{"projectionRecords":5,"stateBytes":1000,"samplePoints":0,"sampleChannels":0}
    });
    let mut context: InstanceContext = serde_json::from_str(&fixture.to_string()).unwrap();
    context.configuration.digest = ArtifactDigest::sha256_of(b"cfg");
    context.domain.digest = ArtifactDigest::sha256_of(b"domain");
    context
}
fn entities() -> Vec<u8> {
    let mut bytes = vec![];
    encode_batch(
        &[DynamicEntity {
            id: EntityId(7),
            kinematics: Kinematics {
                position_metres: [FiniteF64::ZERO; 3],
                velocity_metres_per_second: [FiniteF64::ZERO; 3],
            },
            inertial_mass_kilograms: FiniteF64::new(4.0).unwrap(),
        }],
        &mut bytes,
        BulkLimits::default(),
    )
    .unwrap();
    bytes
}
fn input<'a>(ctx: &'a [u8], entities: &'a [u8]) -> ValidationInputs<'a> {
    ValidationInputs {
        timestep_seconds: FiniteF64::new(0.5).unwrap(),
        context: ctx,
        domain: b"domain",
        configuration: b"cfg",
        entities,
    }
}

#[test]
fn canonical_instance_round_trip_is_bounded_and_role_checked() {
    let context = context();
    let bytes = context.to_cbor().unwrap();
    assert_eq!(InstanceContext::from_cbor(&bytes).unwrap(), context);
    for n in 0..bytes.len() {
        assert!(
            InstanceContext::from_cbor(&bytes[..n]).is_err(),
            "prefix {n}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(InstanceContext::from_cbor(&trailing).is_err());
    assert!(InstanceContext::from_cbor(&vec![0; MAX_CONTEXT_BYTES + 1]).is_err());
    let mut bad = context.clone();
    bad.api_version = "orishu.simulation.instance/v2".parse().unwrap();
    assert!(bad.to_cbor().is_err());
    bad = context.clone();
    bad.execution_contract = ExecutionContractId::Field;
    assert!(bad.to_cbor().is_err());
    bad = context.clone();
    bad.bounds.projection_records = 0;
    assert!(bad.to_cbor().is_err());
    bad = context.clone();
    bad.bounds.sample_points = 1;
    assert!(bad.to_cbor().is_err());
    let mut canonical: ciborium::Value = ciborium::from_reader(bytes.as_slice()).unwrap();
    let ciborium::Value::Map(ref mut fields) = canonical else {
        panic!("map");
    };
    fields.push((
        ciborium::Value::Text("extra".into()),
        ciborium::Value::Integer(1.into()),
    ));
    let mut hostile = vec![];
    ciborium::into_writer(&canonical, &mut hostile).unwrap();
    assert!(InstanceContext::from_cbor(&hostile).is_err());
}

#[test]
fn validation_framing_borrows_bulk_and_reuses_storage() {
    let ctx = context();
    let encoded = ctx.to_cbor().unwrap();
    let entities = entities();
    let input = input(&encoded, &entities);
    let mut bytes = Vec::with_capacity(10_000);
    let address = bytes.as_ptr();
    input
        .encode(&mut bytes, &ctx, BulkLimits::default())
        .unwrap();
    assert_eq!(&bytes[..4], b"OSV1");
    assert_eq!(&bytes[4..12], &[0, 0, 0, 0, 0, 0, 0xe0, 0x3f]);
    assert_eq!(&bytes[12..16], &(encoded.len() as u32).to_le_bytes());
    assert_eq!(&bytes[16..28], &[6, 0, 0, 0, 3, 0, 0, 0, 76, 0, 0, 0]);
    let read = ValidationInputs::read(&bytes, &ctx, BulkLimits::default()).unwrap();
    assert_eq!(read.entities, entities);
    assert_eq!(
        read.entities.as_ptr(),
        bytes[bytes.len() - entities.len()..].as_ptr()
    );
    input
        .encode(&mut bytes, &ctx, BulkLimits::default())
        .unwrap();
    assert_eq!(address, bytes.as_ptr());
    assert_eq!(
        bytes.len(),
        input.encoded_bytes(BulkLimits::default()).unwrap()
    );
}

#[test]
fn validation_refuses_truncation_nonfinite_cross_context_and_budget_excess() {
    let ctx = context();
    let encoded = ctx.to_cbor().unwrap();
    let entities = entities();
    let input = input(&encoded, &entities);
    let mut bytes = vec![];
    input
        .encode(&mut bytes, &ctx, BulkLimits::default())
        .unwrap();
    for n in 0..bytes.len() {
        assert!(
            ValidationInputs::read(&bytes[..n], &ctx, BulkLimits::default()).is_err(),
            "prefix {n}"
        );
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(ValidationInputs::read(&bad, &ctx, BulkLimits::default()).is_err());
    for dt in [0.0, -0.0, -1.0, f64::INFINITY, f64::NAN] {
        let mut bad = bytes.clone();
        bad[4..12].copy_from_slice(&dt.to_le_bytes());
        assert!(ValidationInputs::read(&bad, &ctx, BulkLimits::default()).is_err());
    }
    let mut bad = bytes.clone();
    bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(ValidationInputs::read(&bad, &ctx, BulkLimits::default()).is_err());
    let tight = BulkLimits {
        bytes: bytes.len() - 1,
        ..BulkLimits::default()
    };
    assert_eq!(
        ValidationInputs::read(&bytes, &ctx, tight).unwrap_err(),
        BulkError::LimitExceeded
    );
    let tight = BulkLimits {
        records: 0,
        ..BulkLimits::default()
    };
    assert_eq!(
        ValidationInputs::read(&bytes, &ctx, tight).unwrap_err(),
        BulkError::LimitExceeded
    );
    let mut other = ctx.clone();
    other.instance = "another-use".parse().unwrap();
    assert!(ValidationInputs::read(&bytes, &other, BulkLimits::default()).is_err());
    let mut bad = bytes.clone();
    bad[28 + encoded.len()] = b'D';
    assert!(ValidationInputs::read(&bad, &ctx, BulkLimits::default()).is_err());
    let mut bad = bytes.clone();
    bad[28 + encoded.len() + 6] = b'C';
    assert!(ValidationInputs::read(&bad, &ctx, BulkLimits::default()).is_err());
    let mut output = b"previous candidate".to_vec();
    let before = output.clone();
    assert!(
        input
            .encode(&mut output, &other, BulkLimits::default())
            .is_err()
    );
    assert_eq!(output, before);
}

#[test]
fn canonical_coupling_table_preserves_optional_role_dimensions_and_rejects_undeclared_slots() {
    let mut ctx = context();
    ctx.execution_contract = ExecutionContractId::Field;
    ctx.contribution.extension_point = "orishu.compute.field-models/v1".parse().unwrap();
    let role = CouplingProperty {
        property: "charge".parse().unwrap(),
        dimension: orishu_plugin::Dimension::new([0, 0, 1, 1, 0, 0, 0]),
    };
    ctx.couplings = vec![CouplingDescriptor {
        slot: "electric".parse().unwrap(),
        component: ctx.scientific.clone(),
        source: Some(role),
        response: None,
    }];
    let canonical = ctx.to_cbor().unwrap();
    assert_eq!(InstanceContext::from_cbor(&canonical).unwrap(), ctx);
    let row = CoupledEntity {
        id: EntityId(1),
        slot: CouplingSlot(0),
        kinematics: Kinematics {
            position_metres: [FiniteF64::ZERO; 3],
            velocity_metres_per_second: [FiniteF64::ZERO; 3],
        },
        has_dynamics: false,
        source_si: Some(FiniteF64::ZERO),
        response_si: None,
    };
    let mut entities = vec![];
    let mut output = vec![];
    encode_batch(&[row], &mut entities, BulkLimits::default()).unwrap();
    input(&canonical, &entities)
        .encode(&mut output, &ctx, BulkLimits::default())
        .unwrap();
    for row in [
        CoupledEntity {
            slot: CouplingSlot(1),
            ..row
        },
        CoupledEntity {
            response_si: Some(FiniteF64::ZERO),
            ..row
        },
    ] {
        encode_batch(&[row], &mut entities, BulkLimits::default()).unwrap();
        assert!(
            input(&canonical, &entities)
                .encode(&mut output, &ctx, BulkLimits::default())
                .is_err()
        );
    }
    ctx.couplings.push(ctx.couplings[0].clone());
    assert!(ctx.to_cbor().is_err());
}
