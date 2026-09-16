//! Test-only selected context construction. Synthetic contribution pins below
//! are deliberately not admitted release manifests; no product resolver uses this.
use orishu_plugin::{
    ArtifactDigest, ContributionRef, Dimension, ExecutionContractId, ExecutionProfile, FiniteF64,
    KnownPoint, ScientificContractRef, StateFormat, execution::*,
};
use orishu_runtime::Buffer;
#[allow(dead_code)] // Admission tests use the release schemas; other binaries only use channels.
#[path = "../../../../plugins/reference/declarations.rs"]
pub mod declarations;
pub use declarations::channels as gravity_channels;

pub fn domain() -> Buffer {
    let value = DomainDescriptor {
        api_version: DOMAIN_SCHEMA.parse().unwrap(),
        lower_metres: [FiniteF64::new(-10.0).unwrap(); 3],
        upper_metres: [FiniteF64::new(10.0).unwrap(); 3],
        discretization: SpatialDiscretization::Continuous,
    };
    let bytes = value.to_cbor(DomainLimits::default()).unwrap();
    Buffer {
        schema: DOMAIN_SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}
pub fn euler_config(capacity: u32) -> Buffer {
    resolved(vec![(
        "capacity",
        ConfigurationValue::Quantity {
            value_si: FiniteF64::new(f64::from(capacity)).unwrap(),
            dimension: Dimension::DIMENSIONLESS,
        },
    )])
}
pub fn resolved(properties: Vec<(&str, ConfigurationValue)>) -> Buffer {
    let mut properties: Vec<_> = properties
        .into_iter()
        .map(|(id, value)| ConfigurationProperty {
            id: id.parse().unwrap(),
            value,
        })
        .collect();
    properties.sort_by(|a, b| a.id.cmp(&b.id));
    let config = ResolvedConfiguration {
        api_version: CONFIGURATION_SCHEMA.parse().unwrap(),
        properties,
    };
    let bytes = config.to_cbor(&orishu_plugin::Limits::default()).unwrap();
    Buffer {
        schema: CONFIGURATION_SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}
pub fn context(
    code: &[u8],
    contract: ExecutionContractId,
    config: &Buffer,
    capacity: u32,
) -> InstanceContext {
    let field = contract == ExecutionContractId::Field;
    let name = if field { "newtonian" } else { "euler" };
    let artifact = ArtifactDigest::sha256_of(code);
    let pin =
        ArtifactDigest::sha256_of(b"synthetic context test, not release admission").to_string();
    let scientific = ScientificContractRef {
        name: format!("org.orishu.reference.{name}").parse().unwrap(),
        version: 1.try_into().unwrap(),
        digest: pin.parse().unwrap(),
    };
    let domain = domain();
    InstanceContext {
        api_version: INSTANCE_SCHEMA.parse().unwrap(),
        instance: name.parse().unwrap(),
        kernel: artifact,
        contribution: ContributionRef {
            release: pin.parse().unwrap(),
            extension_point: if field {
                KnownPoint::FieldModels
            } else {
                KnownPoint::Integrators
            }
            .as_str()
            .parse()
            .unwrap(),
            local_id: name.parse().unwrap(),
        },
        scientific: scientific.clone(),
        execution_contract: contract,
        state_format: StateFormat {
            id: format!(
                "org.orishu.reference.{name}.{}",
                if field { "state" } else { "history" }
            )
            .parse()
            .unwrap(),
            version: 1.try_into().unwrap(),
        },
        profile: ExecutionProfile::ForceThenIntegrate,
        observables: if field {
            gravity_channels::bindings()
        } else {
            vec![]
        },
        compute_precision: ComputePrecision::Binary64,
        configuration: InputIdentity::of(
            config.schema.parse().unwrap(),
            config.value_count,
            &config.bytes,
        ),
        domain: InputIdentity::of(
            domain.schema.parse().unwrap(),
            domain.value_count,
            &domain.bytes,
        ),
        couplings: if field {
            vec![CouplingDescriptor {
                slot: "mass".parse().unwrap(),
                component: scientific,
                source: Some(CouplingProperty {
                    property: "mass".parse().unwrap(),
                    dimension: Dimension::MASS,
                }),
                response: Some(CouplingProperty {
                    property: "mass".parse().unwrap(),
                    dimension: Dimension::MASS,
                }),
            }]
        } else {
            vec![]
        },
        bounds: ExecutionBounds {
            projection_records: capacity,
            state_bytes: 128 * 1024 * 1024,
            sample_points: if field { 4096 } else { 0 },
            sample_channels: if field { 3 } else { 0 },
        },
    }
}
#[allow(dead_code)] // Separate integration binaries consume different fixture helpers.
pub fn validation(
    context: &InstanceContext,
    config: &Buffer,
    entities: &Buffer,
    dt: f64,
) -> Buffer {
    let ctx = context.to_cbor().unwrap();
    let domain = domain();
    let mut bytes = vec![];
    ValidationInputs {
        timestep_seconds: FiniteF64::new(1.0).unwrap(),
        context: &ctx,
        domain: &domain.bytes,
        configuration: &config.bytes,
        entities: &entities.bytes,
    }
    .encode(&mut bytes, context, BulkLimits::default())
    .unwrap();
    // Deliberately permit hostile timestep fixtures without weakening the encoder.
    bytes[4..12].copy_from_slice(&dt.to_le_bytes());
    Buffer {
        schema: VALIDATION_SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}
