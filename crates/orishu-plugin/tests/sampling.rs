//! Public sampling codecs and candidate validation; no runtime or field physics.
use orishu_plugin::{
    ArtifactDigest, Declaration, Dimension, FiniteF64, Limits, ObservableSchema, Payload, Shape,
    execution::*,
};

fn n(v: f64) -> FiniteF64 {
    FiniteF64::new(v).unwrap()
}
fn channel(name: &str, shape: Shape) -> SampleChannel {
    let axes = match shape {
        Shape::Scalar => vec![],
        Shape::Vector { .. } => vec!["world axes".into()],
        Shape::Matrix { .. } => vec!["value axis".into(), "derivative axis".into()],
    };
    let schema = ObservableSchema {
        name: format!("org.test.{name}").parse().unwrap(),
        version: 1.try_into().unwrap(),
        requirements: vec![],
        meaning: name.into(),
        shape,
        dimension: Dimension::LENGTH,
        frame: "world Cartesian".into(),
        axes,
        conventions: "row-major SI".into(),
    };
    let contract = Payload::Observables(Declaration {
        scientific: schema.clone(),
        presentation: None,
    })
    .contract_ref(&Limits::default())
    .unwrap();
    SampleChannel { contract, schema }
}
fn context() -> InstanceContext {
    let digest = ArtifactDigest::sha256_of(b"fixture").to_string();
    let fixture = serde_json::json!({
        "apiVersion":INSTANCE_SCHEMA,"instance":"field","kernel":digest,
        "contribution":{"release":digest,"extensionPoint":"orishu.compute.field-models/v1","localId":"field"},
        "scientific":{"name":"org.test.field","version":1,"digest":digest},
        "executionContract":"orishu:simulation/field@1",
        "stateFormat":{"id":"org.test.state","version":1},
        "profile":"orishu.force-then-integrate/v1",
        "configuration":{"schema":"org.test.config/v1","byteLength":3,"valueCount":1,"digest":digest},
        "domain":{"schema":"org.test.domain/v1","byteLength":6,"valueCount":1,"digest":digest},
        "couplings":[],"observables":[],"computePrecision":"binary64",
        "bounds":{"projectionRecords":5,"stateBytes":1000,"samplePoints":100,"sampleChannels":4}
    });
    let mut context: InstanceContext = serde_json::from_str(&fixture.to_string()).unwrap();
    context.observables = [
        ("a", Shape::Scalar),
        ("b", Shape::Vector { length: 3 }),
        (
            "c",
            Shape::Matrix {
                rows: 2,
                columns: 3,
            },
        ),
    ]
    .into_iter()
    .map(|(slot, shape)| ObservableBinding {
        slot: slot.parse().unwrap(),
        channel: channel(slot, shape),
        quality_flags: 3,
    })
    .collect();
    context.validate().unwrap();
    context
}
fn metadata(ctx: &InstanceContext) -> SampleMetadata {
    SampleMetadata {
        api_version: SAMPLE_METADATA_SCHEMA.parse().unwrap(),
        request_id: 99,
        snapshot: SampleSnapshot {
            source: SnapshotSource::Authored {
                revision: ArtifactDigest::sha256_of(b"revision"),
            },
            state: InputIdentity::of("org.test.state/v1".parse().unwrap(), 1, b"state"),
        },
        field: ctx.instance.clone(),
        context: ArtifactDigest::sha256_of(&ctx.to_cbor().unwrap()),
        channels: [2, 0, 1]
            .map(|i| ctx.observables[i].channel.clone())
            .to_vec(),
    }
}
fn points() -> [SamplePoint; 2] {
    [
        SamplePoint {
            id: 99,
            position_metres: [n(3.0), n(-2.0), n(1.0)],
        },
        SamplePoint {
            id: 7,
            position_metres: [n(0.0); 3],
        },
    ]
}
fn query(meta: &SampleMetadata, points: &[SamplePoint]) -> Vec<u8> {
    let mut bytes = vec![];
    encode_sample_request(
        meta,
        points,
        &mut bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap();
    bytes
}
fn read(bytes: &[u8]) -> SampleRequest<'_> {
    SampleRequest::read(
        bytes,
        &mut SampleScratch::default(),
        SampleLimits::default(),
    )
    .unwrap()
}
fn response(request: &SampleRequest<'_>, context: &InstanceContext) -> Vec<u8> {
    let mut bytes = vec![];
    let mut output =
        SampleOutput::new(&mut bytes, request, context, SampleLimits::default()).unwrap();
    for p in 0..request.len() {
        for (c, channel) in request.metadata().channels.iter().enumerate() {
            output
                .valid(p, c, &vec![n((p + c) as f64); channel.components()], 3)
                .unwrap();
        }
    }
    output.finish().unwrap();
    bytes
}

#[test]
fn flat_ranges_preserve_exact_channels_points_and_shapes_and_reuse_storage() {
    let ctx = context();
    let meta = metadata(&ctx);
    let mut bytes = Vec::with_capacity(20_000);
    let mut scratch = SampleScratch::default();
    let address = bytes.as_ptr();
    for _ in 0..2 {
        encode_sample_request(
            &meta,
            &points(),
            &mut bytes,
            &mut scratch,
            SampleLimits::default(),
        )
        .unwrap();
        assert_eq!(address, bytes.as_ptr());
    }
    let request = read(&bytes);
    assert_eq!(request.metadata(), &meta);
    assert_eq!(request.points().collect::<Vec<_>>(), points());
    assert_eq!(request.bytes().as_ptr(), bytes.as_ptr());
    let layout = request.layout(SampleLimits::default()).unwrap();
    assert_eq!(layout.bytes, 248);
    assert_eq!(
        layout.channels[0],
        SampleChannelLayout {
            validity_offset: 40,
            quality_offset: 48,
            values_offset: 56,
            components: 6,
            point_stride: 48,
            points: 2,
        }
    );
    assert_eq!(layout.channels[1].validity_offset, 152);
    assert_eq!(layout.channels[2].validity_offset, 184);
    let mut out = Vec::with_capacity(1000);
    let address = out.as_ptr();
    for _ in 0..2 {
        let mut writer =
            SampleOutput::new(&mut out, &request, &ctx, SampleLimits::default()).unwrap();
        for p in 0..2 {
            writer.valid(p, 0, &[n(1.0); 6], 1).unwrap();
            writer.valid(p, 1, &[n(2.0)], 2).unwrap();
            writer.valid(p, 2, &[n(3.0); 3], 3).unwrap();
        }
        writer.finish().unwrap();
        assert_eq!(address, out.as_ptr());
    }
    let response = SampleResponse::read(&out, &request, &ctx, SampleLimits::default()).unwrap();
    match response.cell(1, 0).unwrap() {
        SampleCell::Valid {
            values,
            quality_flags,
        } => {
            assert_eq!(values.iter().collect::<Vec<_>>(), [n(1.0); 6]);
            assert_eq!(quality_flags, 1);
        }
        _ => panic!("valid matrix"),
    }
    assert!(response.cell(2, 0).is_none());
    assert!(response.cell(0, 3).is_none());
}

#[test]
fn requests_refuse_all_truncations_duplicates_noncanonical_numbers_and_budget_excess() {
    let ctx = context();
    let meta = metadata(&ctx);
    let original = query(&meta, &points());
    let mut scratch = SampleScratch::default();
    for n in 0..original.len() {
        assert!(
            SampleRequest::read(&original[..n], &mut scratch, SampleLimits::default()).is_err(),
            "prefix {n}"
        );
    }
    for offset in [4, 8] {
        let mut bad = original.clone();
        bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(SampleRequest::read(&bad, &mut scratch, SampleLimits::default()).is_err());
    }
    let mut bad = original.clone();
    bad.push(0);
    assert!(SampleRequest::read(&bad, &mut scratch, SampleLimits::default()).is_err());
    let start = original.len() - 64;
    for v in [-0.0, f64::INFINITY, f64::NAN] {
        let mut bad = original.clone();
        bad[start + 8..start + 16].copy_from_slice(&v.to_le_bytes());
        assert!(SampleRequest::read(&bad, &mut scratch, SampleLimits::default()).is_err());
    }
    let mut bad = original.clone();
    bad[start + 32..start + 40].copy_from_slice(&99u64.to_le_bytes());
    assert!(SampleRequest::read(&bad, &mut scratch, SampleLimits::default()).is_err());
    let mut destination = vec![1, 2, 3];
    let duplicates = [points()[0]; 2];
    assert_eq!(
        encode_sample_request(
            &meta,
            &duplicates,
            &mut destination,
            &mut scratch,
            SampleLimits::default()
        ),
        Err(SampleError::Request)
    );
    assert_eq!(destination, [1, 2, 3]);
    for limits in [
        SampleLimits {
            bytes: original.len() - 1,
            ..SampleLimits::default()
        },
        SampleLimits {
            points: 1,
            ..SampleLimits::default()
        },
        SampleLimits {
            channels: 2,
            ..SampleLimits::default()
        },
        SampleLimits {
            values: 19,
            ..SampleLimits::default()
        },
    ] {
        assert!(SampleRequest::read(&original, &mut scratch, limits).is_err());
    }
    let mut bad = meta.clone();
    bad.channels[0].schema.shape = Shape::Vector { length: 0 };
    assert!(bad.to_cbor().is_err());
    let mut bad = meta.clone();
    bad.channels[0].schema.dimension = Dimension::MASS;
    assert!(bad.to_cbor().is_err());
    let mut bad = meta;
    bad.channels[1] = bad.channels[0].clone();
    assert!(bad.to_cbor().is_err());
}

#[test]
fn response_identity_binds_positions_snapshot_channels_request_context_and_precision() {
    let ctx = context();
    let meta = metadata(&ctx);
    let bytes = query(&meta, &points());
    let request = read(&bytes);
    let out = response(&request, &ctx);
    for n in 0..out.len() {
        assert!(SampleResponse::read(&out[..n], &request, &ctx, SampleLimits::default()).is_err());
    }
    for i in 0..6 {
        let mut altered = meta.clone();
        let mut points = points();
        match i {
            0 => altered.request_id += 1,
            1 => {
                altered.snapshot.source = SnapshotSource::Authored {
                    revision: ArtifactDigest::sha256_of(b"another revision"),
                }
            }
            2 => altered.snapshot.state.digest = ArtifactDigest::sha256_of(b"other state"),
            3 => altered.channels.swap(0, 2),
            4 => points[0].id += 1,
            _ => points[0].position_metres[0] = n(4.0),
        }
        let bytes = query(&altered, &points);
        let other = read(&bytes);
        assert!(SampleResponse::read(&out, &other, &ctx, SampleLimits::default()).is_err());
    }
    let mut other = ctx.clone();
    other.compute_precision = ComputePrecision::Binary32;
    assert!(SampleResponse::read(&out, &request, &other, SampleLimits::default()).is_err());
    let mut other = ctx.clone();
    other.bounds.sample_points = 1;
    let mut other_meta = metadata(&other);
    other_meta.channels = meta.channels;
    let other_bytes = query(&other_meta, &points());
    let other_request = read(&other_bytes);
    assert!(
        SampleOutput::new(&mut vec![], &other_request, &other, SampleLimits::default()).is_err()
    );
    let mut corrupt = out.clone();
    corrupt[36..40].copy_from_slice(&32u32.to_le_bytes());
    assert!(SampleResponse::read(&corrupt, &request, &ctx, SampleLimits::default()).is_err());
    let layout = request.layout(SampleLimits::default()).unwrap();
    for v in [-0.0, f64::NAN, f64::INFINITY] {
        let mut corrupt = out.clone();
        let offset = layout.channels[0].values_offset;
        corrupt[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
        assert!(SampleResponse::read(&corrupt, &request, &ctx, SampleLimits::default()).is_err());
    }
    for flags in [0u32, 4, u32::MAX] {
        let mut corrupt = out.clone();
        let offset = layout.channels[0].quality_offset;
        corrupt[offset..offset + 4].copy_from_slice(&flags.to_le_bytes());
        assert!(SampleResponse::read(&corrupt, &request, &ctx, SampleLimits::default()).is_err());
    }
}

#[test]
fn unavailable_is_distinct_from_invalid_and_invalid_never_exposes_numeric_slots() {
    let ctx = context();
    let mut meta = metadata(&ctx);
    meta.channels = vec![
        ctx.observables[0].channel.clone(),
        channel("unavailable", Shape::Scalar),
    ];
    let bytes = query(&meta, &points());
    let request = read(&bytes);
    let mut out = vec![];
    let mut writer = SampleOutput::new(&mut out, &request, &ctx, SampleLimits::default()).unwrap();
    writer
        .invalid(0, 0, SampleInvalidity::OutsideDomain)
        .unwrap();
    writer.invalid(1, 0, SampleInvalidity::Singular).unwrap();
    for p in 0..2 {
        writer
            .invalid(p, 1, SampleInvalidity::ChannelUnavailable)
            .unwrap();
    }
    writer.finish().unwrap();
    let layout = request.layout(SampleLimits::default()).unwrap();
    for c in &layout.channels {
        for p in 0..2 {
            let start = c.values_offset + p * 8;
            out[start..start + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        }
    }
    let result = SampleResponse::read(&out, &request, &ctx, SampleLimits::default()).unwrap();
    assert!(matches!(
        result.cell(0, 0),
        Some(SampleCell::Invalid(SampleInvalidity::OutsideDomain))
    ));
    assert!(matches!(
        result.cell(1, 0),
        Some(SampleCell::Invalid(SampleInvalidity::Singular))
    ));
    assert!(matches!(
        result.cell(0, 1),
        Some(SampleCell::Invalid(SampleInvalidity::ChannelUnavailable))
    ));
    for (channel, code) in [(0, 4u32), (1, 1), (0, 5), (0, 0)] {
        let mut bad = out.clone();
        let offset = layout.channels[channel].validity_offset;
        bad[offset..offset + 4].copy_from_slice(&code.to_le_bytes());
        assert!(SampleResponse::read(&bad, &request, &ctx, SampleLimits::default()).is_err());
    }
    let mut bad = out;
    let offset = layout.channels[0].quality_offset;
    bad[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    assert!(SampleResponse::read(&bad, &request, &ctx, SampleLimits::default()).is_err());
}

#[test]
fn partial_duplicate_invalid_quality_or_unavailable_writes_poison_completion() {
    let ctx = context();
    let mut meta = metadata(&ctx);
    meta.channels.truncate(1);
    let bytes = query(&meta, &points()[..1]);
    let request = read(&bytes);
    for mode in 0..6 {
        let mut out = vec![];
        let mut writer =
            SampleOutput::new(&mut out, &request, &ctx, SampleLimits::default()).unwrap();
        match mode {
            0 => {}
            1 => {
                writer.valid(0, 0, &[n(1.0); 6], 1).unwrap();
                assert!(writer.valid(0, 0, &[n(1.0); 6], 1).is_err());
            }
            2 => {
                assert!(writer.valid(0, 0, &[n(1.0); 6], 4).is_err());
            }
            3 => {
                assert!(writer.valid(0, 0, &[n(1.0); 5], 1).is_err());
            }
            4 => {
                assert!(
                    writer
                        .invalid(0, 0, SampleInvalidity::ChannelUnavailable)
                        .is_err()
                );
            }
            _ => {
                assert!(writer.valid(1, 0, &[n(1.0); 6], 1).is_err());
            }
        }
        assert!(writer.finish().is_err());
    }
    meta.channels[0] = channel("unavailable", Shape::Scalar);
    let bytes = query(&meta, &points()[..1]);
    let request = read(&bytes);
    let mut out = vec![];
    let mut writer = SampleOutput::new(&mut out, &request, &ctx, SampleLimits::default()).unwrap();
    assert!(writer.valid(0, 0, &[n(0.0)], 1).is_err());
    assert!(
        writer
            .invalid(0, 0, SampleInvalidity::ChannelUnavailable)
            .is_err()
    );
    assert!(writer.finish().is_err());
}

#[test]
fn empty_batches_and_committed_metadata_have_explicit_identity() {
    let ctx = context();
    let mut meta = metadata(&ctx);
    let digest = ArtifactDigest::sha256_of(b"workload").to_string();
    meta.snapshot.source = SnapshotSource::Committed {
        workload: digest.parse().unwrap(),
        run: ArtifactDigest::sha256_of(b"run descriptor"),
        epoch: 2,
        boundary: 10,
        time_seconds: n(0.5),
    };
    let bytes = query(&meta, &[]);
    let request = read(&bytes);
    assert!(request.is_empty());
    assert_eq!(request.points().len(), 0);
    let mut output = vec![];
    SampleOutput::new(&mut output, &request, &ctx, SampleLimits::default())
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(output.len(), 40);
    let result = SampleResponse::read(&output, &request, &ctx, SampleLimits::default()).unwrap();
    assert!(result.cell(0, 0).is_none());
    if let SnapshotSource::Committed { time_seconds, .. } = &mut meta.snapshot.source {
        *time_seconds = n(-1.0);
    }
    assert!(meta.to_cbor().is_err());
}
