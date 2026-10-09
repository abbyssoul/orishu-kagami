use orishu_plugin::{ArtifactDigest, FiniteF64, execution::*};

fn number(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}
fn source(boundary: u64) -> SnapshotSource {
    SnapshotSource::Committed {
        workload: ArtifactDigest::sha256_of(b"workload")
            .to_string()
            .parse()
            .unwrap(),
        run: ArtifactDigest::sha256_of(b"run descriptor"),
        epoch: 1,
        boundary,
        time_seconds: number(boundary as f64 * 0.5),
    }
}
fn object(id: u64, dynamic: bool) -> ObjectState {
    ObjectState {
        id: EntityId(id),
        kinematics: Kinematics {
            position_metres: [number(1.0), number(2.0), number(3.0)],
            velocity_metres_per_second: [number(4.0), number(5.0), number(6.0)],
        },
        inertial_mass_kilograms: dynamic.then(|| number(2.0)),
    }
}
fn force(id: u64) -> Force {
    Force {
        id: EntityId(id),
        newtons: [number(7.0), number(8.0), number(9.0)],
    }
}
fn packet<R: BulkRecord>(records: &[R]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_batch(records, &mut out, BulkLimits::default()).unwrap();
    out
}
fn frame(boundary: u64) -> (Vec<u8>, ArtifactDigest) {
    let mut out = Vec::new();
    let objects = packet(&[object(1, false), object(2, true)]);
    let forces = packet(&[force(2)]);
    let id = encode_object_observation(
        &source(boundary),
        &objects,
        (boundary != 0).then_some(forces.as_slice()),
        &mut out,
        ObjectObservationLimits::default(),
    )
    .unwrap();
    (out, id)
}
fn read(bytes: &[u8]) -> Result<ObjectObservation<'_>, ObjectObservationError> {
    ObjectObservation::read(
        bytes,
        ArtifactDigest::sha256_of(bytes),
        ObjectObservationLimits::default(),
    )
}

#[test]
fn full_projection_roundtrips_static_dynamic_and_predecessor_forces() {
    for boundary in [0, 1, 17] {
        let (bytes, id) = frame(boundary);
        if boundary == 1 {
            assert_eq!(bytes.len(), 489);
            assert_eq!(
                id.to_string(),
                "sha256:a64d5fbbf5e6e91916114d107ab5c3c5153b673003e9f082dee050fb97df7317"
            );
        }
        let observation =
            ObjectObservation::read(&bytes, id, ObjectObservationLimits::default()).unwrap();
        observation.check_source(&source(boundary)).unwrap();
        assert_eq!(observation.id(), id);
        assert_eq!(observation.bytes().as_ptr(), bytes.as_ptr());
        assert_eq!(
            observation.objects().iter().collect::<Vec<_>>(),
            [object(1, false), object(2, true)]
        );
        assert_eq!(
            observation.force_evaluation_boundary(),
            boundary.checked_sub(1)
        );
        if boundary == 0 {
            assert!(observation.forces().is_none());
        } else {
            assert_eq!(
                observation.forces().unwrap().iter().collect::<Vec<_>>(),
                [force(2)]
            );
        }
        assert_eq!(frame(boundary), (bytes, id));
    }
}

#[test]
fn source_correlation_checks_every_identity_and_time_field() {
    let (bytes, _) = frame(1);
    let observation = read(&bytes).unwrap();
    for case in 0..6 {
        let mut wrong = source(1);
        let SnapshotSource::Committed {
            workload,
            run,
            epoch,
            boundary,
            time_seconds,
        } = &mut wrong
        else {
            unreachable!()
        };
        match case {
            0 => {
                *workload = ArtifactDigest::sha256_of(b"other workload")
                    .to_string()
                    .parse()
                    .unwrap()
            }
            1 => *run = ArtifactDigest::sha256_of(b"other run"),
            2 => *epoch += 1,
            3 => *boundary += 1,
            4 => *time_seconds = number(2.0),
            _ => {
                wrong = SnapshotSource::Authored {
                    revision: ArtifactDigest::sha256_of(b"revision"),
                }
            }
        }
        assert_eq!(
            observation.check_source(&wrong),
            Err(ObjectObservationError::Source)
        );
    }
}

#[test]
fn initial_absence_is_not_empty_computed_force_set() {
    let objects = packet::<ObjectState>(&[]);
    let forces = packet::<Force>(&[]);
    let mut out = Vec::new();
    for boundary in [0, 1] {
        encode_object_observation(
            &source(boundary),
            &objects,
            (boundary != 0).then_some(forces.as_slice()),
            &mut out,
            ObjectObservationLimits::default(),
        )
        .unwrap();
        assert!(read(&out).unwrap().objects().is_empty());
        assert_eq!(read(&out).unwrap().forces().is_some(), boundary != 0);
        let before = out.clone();
        assert_eq!(
            encode_object_observation(
                &source(boundary),
                &objects,
                (boundary == 0).then_some(forces.as_slice()),
                &mut out,
                ObjectObservationLimits::default()
            ),
            Err(ObjectObservationError::Coverage)
        );
        assert_eq!(out, before);
    }
}

#[test]
fn force_membership_is_exact_not_a_subset_or_static_object_overlay() {
    let objects = packet(&[object(1, false), object(2, true)]);
    for forces in [
        vec![],
        vec![force(1)],
        vec![force(3)],
        vec![force(1), force(2)],
        vec![force(2), force(3)],
    ] {
        let mut out = b"prior output".to_vec();
        assert_eq!(
            encode_object_observation(
                &source(1),
                &objects,
                Some(&packet(&forces)),
                &mut out,
                ObjectObservationLimits::default()
            ),
            Err(ObjectObservationError::Coverage)
        );
        assert_eq!(out, b"prior output");
    }
}

#[test]
fn limits_precede_allocation_and_output_reuses_capacity_atomically() {
    let (bytes, _) = frame(1);
    for limits in [
        ObjectObservationLimits {
            bytes: bytes.len() - 1,
            objects: 2,
        },
        ObjectObservationLimits {
            bytes: bytes.len(),
            objects: 1,
        },
    ] {
        assert!(
            ObjectObservation::read(&bytes, ArtifactDigest::sha256_of(&bytes), limits).is_err()
        );
        let mut output = b"prior output".to_vec();
        assert!(
            encode_object_observation(
                &source(1),
                &packet(&[object(1, false), object(2, true)]),
                Some(&packet(&[force(2)])),
                &mut output,
                limits
            )
            .is_err()
        );
        assert_eq!(output, b"prior output");
    }
    let limits = ObjectObservationLimits {
        bytes: bytes.len(),
        objects: 2,
    };
    assert!(ObjectObservation::read(&bytes, ArtifactDigest::sha256_of(&bytes), limits).is_ok());
    let mut output = Vec::with_capacity(bytes.len());
    let pointer = output.as_ptr();
    for _ in 0..2 {
        encode_object_observation(
            &source(1),
            &packet(&[object(1, false), object(2, true)]),
            Some(&packet(&[force(2)])),
            &mut output,
            limits,
        )
        .unwrap();
        assert_eq!(output.as_ptr(), pointer);
        assert_eq!(output, bytes);
    }
}

#[test]
fn rejects_truncation_trailing_wrong_magic_lengths_and_integrity() {
    let (bytes, id) = frame(1);
    for end in 0..bytes.len() {
        assert!(read(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(read(&trailing).is_err());
    for offset in [0, 4, 8, 12] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&bad).is_err());
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        ObjectObservation::read(&corrupt, id, ObjectObservationLimits::default()),
        Err(ObjectObservationError::Integrity)
    ));
    assert!(matches!(
        ObjectObservation::read(
            &bytes,
            ArtifactDigest::sha256_of(b"different"),
            ObjectObservationLimits::default()
        ),
        Err(ObjectObservationError::Integrity)
    ));
}

#[test]
fn recomputed_digest_does_not_bypass_numeric_membership_or_metadata_validation() {
    let (bytes, _) = frame(1);
    let object_start = 16 + u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let force_start = object_start + u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    for case in 0..5 {
        let mut bad = bytes.clone();
        match case {
            // Non-finite position, then noncanonical negative zero.
            0 => bad[object_start + HEADER_BYTES + 8..object_start + HEADER_BYTES + 16]
                .copy_from_slice(&f64::INFINITY.to_le_bytes()),
            1 => bad[object_start + HEADER_BYTES + 8..object_start + HEADER_BYTES + 16]
                .copy_from_slice(&(-0.0f64).to_le_bytes()),
            // Duplicate object ID, invalid force membership, unknown version.
            2 => bad[object_start + HEADER_BYTES + ObjectState::BYTES
                ..object_start + HEADER_BYTES + ObjectState::BYTES + 8]
                .copy_from_slice(&1u64.to_le_bytes()),
            3 => bad[force_start + HEADER_BYTES..force_start + HEADER_BYTES + 8]
                .copy_from_slice(&3u64.to_le_bytes()),
            _ => {
                let position = bad
                    .windows(OBJECT_OBSERVATION_SCHEMA.len())
                    .position(|v| v == OBJECT_OBSERVATION_SCHEMA.as_bytes())
                    .unwrap();
                bad[position + OBJECT_OBSERVATION_SCHEMA.len() - 1] = b'2';
            }
        }
        assert!(read(&bad).is_err(), "case {case}");
    }
}

#[test]
fn invalid_source_cannot_be_encoded_and_output_is_unchanged() {
    for case in 0..3 {
        let mut invalid = source(0);
        let SnapshotSource::Committed {
            epoch,
            time_seconds,
            ..
        } = &mut invalid
        else {
            unreachable!()
        };
        match case {
            0 => *epoch = 0,
            1 => *time_seconds = number(-1.0),
            _ => {
                invalid = SnapshotSource::Authored {
                    revision: ArtifactDigest::sha256_of(b"revision"),
                }
            }
        }
        let mut output = b"prior output".to_vec();
        assert_eq!(
            encode_object_observation(
                &invalid,
                &packet::<ObjectState>(&[]),
                None,
                &mut output,
                ObjectObservationLimits::default()
            ),
            Err(ObjectObservationError::Source)
        );
        assert_eq!(output, b"prior output");
    }
}

#[test]
fn hostile_metadata_and_claimed_record_counts_are_refused() {
    let (bytes, _) = frame(1);
    let metadata_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let metadata = &bytes[16..16 + metadata_len];
    let replace_metadata = |metadata: &[u8]| {
        let mut modified = bytes[..16].to_vec();
        modified[4..8].copy_from_slice(&(metadata.len() as u32).to_le_bytes());
        modified.extend_from_slice(metadata);
        modified.extend_from_slice(&bytes[16 + metadata_len..]);
        modified
    };
    for case in 0..5 {
        let mut bad = metadata.to_vec();
        match case {
            0 => {
                let at = bad.windows(7).position(|v| v == b"\x65epoch\x01").unwrap();
                bad[at + 6] = 0;
            }
            1 => {
                let at = bad.windows(11).position(|v| v == b"timeSeconds").unwrap();
                // Canonical f64 is big-endian; flip the sign bit after its head.
                bad[at + 12] |= 0x80;
            }
            2 => {
                assert_eq!(bad[0], 0xa2);
                bad[0] = 0xa3;
                bad.extend_from_slice(b"\x6bunknown-key\xf5");
            }
            3 => {
                bad[0] = 0xa3;
                bad.extend_from_slice(b"\x66source\xf5");
            }
            _ => bad.resize(2049, 0),
        }
        assert!(
            read(&replace_metadata(&bad)).is_err(),
            "metadata case {case}"
        );
    }
    let mut bad_count = bytes.clone();
    let at = 16 + metadata_len + 8;
    bad_count[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        read(&bad_count),
        Err(ObjectObservationError::Bulk(BulkError::LimitExceeded))
    ));
}
