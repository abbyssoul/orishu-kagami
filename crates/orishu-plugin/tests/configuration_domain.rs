use orishu_plugin::{
    Dimension, ErrorCode, FiniteF64, Limits, Property, PropertyType, execution::*,
};
use orishu_variables::{CompiledExpression, Namespace, VariableOptions, VariablesSystem};
fn n(v: f64) -> FiniteF64 {
    FiniteF64::new(v).unwrap()
}
fn schema() -> Vec<Property> {
    vec![
        Property {
            id: "radius".parse().unwrap(),
            required: true,
            schema: PropertyType::Quantity {
                dimension: Dimension::LENGTH,
                default_expression: Some("1000 mm".into()),
                minimum_si: Some(n(0.0)),
                maximum_si: Some(n(2.0)),
            },
        },
        Property {
            id: "enabled".parse().unwrap(),
            required: true,
            schema: PropertyType::Boolean {
                default: Some(true),
            },
        },
        Property {
            id: "label".parse().unwrap(),
            required: false,
            schema: PropertyType::Text {
                max_bytes: 16,
                default: Some("test".into()),
            },
        },
        Property {
            id: "optional".parse().unwrap(),
            required: false,
            schema: PropertyType::Boolean { default: None },
        },
    ]
}
fn expr(id: &str, source: &str) -> AuthoredConfigurationProperty {
    AuthoredConfigurationProperty {
        id: id.parse().unwrap(),
        input: ConfigurationInput::Expression {
            source: source.into(),
        },
    }
}

#[test]
fn declared_configuration_captures_units_defaults_and_variables_without_changing_source() {
    let mut vars = VariablesSystem::default();
    let id = vars
        .define(
            &Namespace::new("experiment"),
            "radius",
            CompiledExpression::parse("5 cm").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
    let inputs = vec![
        expr("radius", "experiment.radius * 2"),
        AuthoredConfigurationProperty {
            id: "enabled".parse().unwrap(),
            input: ConfigurationInput::Boolean { value: false },
        },
    ];
    let before = inputs.clone();
    let config = resolve_configuration(&schema(), &inputs, &vars, &Limits::default()).unwrap();
    assert_eq!(inputs, before);
    assert_eq!(
        config.get("radius"),
        Some(&ConfigurationValue::Quantity {
            value_si: n(0.1),
            dimension: Dimension::LENGTH
        })
    );
    assert_eq!(
        config.get("enabled"),
        Some(&ConfigurationValue::Boolean { value: false })
    );
    assert_eq!(
        config.get("label"),
        Some(&ConfigurationValue::Text {
            value: "test".into()
        })
    );
    assert_eq!(config.get("optional"), None);
    let bytes = config.to_cbor(&Limits::default()).unwrap();
    vars.set(id, CompiledExpression::parse("9 cm").unwrap())
        .unwrap();
    let captured = ResolvedConfiguration::from_cbor(&bytes, &Limits::default()).unwrap();
    assert_eq!(captured, config);
    let mut new_schema = schema();
    if let PropertyType::Quantity {
        default_expression, ..
    } = &mut new_schema[0].schema
    {
        *default_expression = Some("2 m".into());
    }
    captured
        .validate_against(&new_schema, &Limits::default())
        .unwrap();
    assert_eq!(captured.get("radius"), config.get("radius"));
    assert_eq!(
        resolve_configuration(&schema(), &[], &vars, &Limits::default())
            .unwrap()
            .get("radius"),
        Some(&ConfigurationValue::Quantity {
            value_si: n(1.0),
            dimension: Dimension::LENGTH
        })
    );
    let mut reversed = inputs.clone();
    reversed.reverse();
    assert_eq!(
        resolve_configuration(&schema(), &inputs, &vars, &Limits::default()).unwrap(),
        resolve_configuration(&schema(), &reversed, &vars, &Limits::default()).unwrap()
    );
}

#[test]
fn configuration_rejects_missing_extra_duplicate_dimension_type_and_constraint_errors() {
    let vars = VariablesSystem::default();
    for inputs in [
        vec![expr("radius", "2 kg")],
        vec![expr("radius", "3 m")],
        vec![expr("radius", "-1 m")],
        vec![expr("undeclared", "unknown.symbol")],
        vec![expr("radius", "1 m"), expr("radius", "1 m")],
        vec![expr("enabled", "1")],
        vec![expr("radius", "missing.symbol")],
        vec![AuthoredConfigurationProperty {
            id: "label".parse().unwrap(),
            input: ConfigurationInput::Text {
                value: "x".repeat(17),
            },
        }],
    ] {
        assert!(resolve_configuration(&schema(), &inputs, &vars, &Limits::default()).is_err());
    }
    let mut missing = schema();
    if let PropertyType::Quantity {
        default_expression, ..
    } = &mut missing[0].schema
    {
        *default_expression = None;
    }
    assert!(resolve_configuration(&missing, &[], &vars, &Limits::default()).is_err());
    let mut config = resolve_configuration(&schema(), &[], &vars, &Limits::default()).unwrap();
    config.properties.retain(|p| p.id.as_str() != "radius");
    assert!(
        config
            .validate_against(&schema(), &Limits::default())
            .is_err()
    );
    // A syntactically valid default is not permission for worker-side insertion.
    assert_eq!(config.get("radius"), None);
    let vars = VariablesSystem::with_limits(orishu_variables::Limits {
        max_evaluation_work: 0,
        ..orishu_variables::Limits::DEFAULT
    });
    assert_eq!(
        resolve_configuration(&schema(), &[], &vars, &Limits::default())
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let tight = Limits {
        max_schema_items: 1,
        ..Limits::default()
    };
    assert_eq!(
        resolve_configuration(&schema(), &[], &VariablesSystem::default(), &tight)
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn configuration_canonical_reader_rejects_hostile_collections_scalars_and_alternate_encodings() {
    let config = resolve_configuration(
        &schema(),
        &[],
        &VariablesSystem::default(),
        &Limits::default(),
    )
    .unwrap();
    let bytes = config.to_cbor(&Limits::default()).unwrap();
    for n in 0..bytes.len() {
        assert!(ResolvedConfiguration::from_cbor(&bytes[..n], &Limits::default()).is_err());
    }
    let tight = Limits {
        max_payload_bytes: bytes.len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        ResolvedConfiguration::from_cbor(&bytes, &tight)
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let mut bad = config.clone();
    bad.properties.reverse();
    assert!(bad.to_cbor(&Limits::default()).is_err());
    bad = config.clone();
    bad.properties.push(bad.properties[0].clone());
    assert!(bad.to_cbor(&Limits::default()).is_err());
    bad = config.clone();
    bad.api_version = "orishu.simulation.configuration/v2".parse().unwrap();
    assert!(bad.to_cbor(&Limits::default()).is_err());
    let mut tree: ciborium::Value = ciborium::from_reader(bytes.as_slice()).unwrap();
    let ciborium::Value::Map(ref mut map) = tree else {
        panic!("map");
    };
    map.push((
        ciborium::Value::Text("extra".into()),
        ciborium::Value::Bool(true),
    ));
    let mut hostile = vec![];
    ciborium::into_writer(&tree, &mut hostile).unwrap();
    assert!(ResolvedConfiguration::from_cbor(&hostile, &Limits::default()).is_err());
    let raw = serde_json::json!({"apiVersion":CONFIGURATION_SCHEMA,"properties":[{"id":"radius","value":{"kind":"quantity","valueSI":null,"dimension":[1,0,0,0,0,0,0]}}]});
    assert!(serde_json::from_value::<ResolvedConfiguration>(raw).is_err());
}
fn domain() -> DomainDescriptor {
    DomainDescriptor {
        api_version: DOMAIN_SCHEMA.parse().unwrap(),
        lower_metres: [n(-1.0), n(-2.0), n(-3.0)],
        upper_metres: [n(1.0), n(2.0), n(3.0)],
        discretization: SpatialDiscretization::Continuous,
    }
}
#[test]
fn domain_keeps_origin_rank_and_explicit_spatial_scheme_without_allocating_a_grid() {
    let domain = domain();
    let bytes = domain.to_cbor(DomainLimits { cells: 0 }).unwrap();
    assert_eq!(
        DomainDescriptor::from_cbor(&bytes, DomainLimits { cells: 0 }).unwrap(),
        domain
    );
    let mut grid = domain.clone();
    grid.discretization = SpatialDiscretization::CartesianCells { cells: [2, 3, 4] };
    let bytes = grid.to_cbor(DomainLimits { cells: 24 }).unwrap();
    assert_eq!(
        DomainDescriptor::from_cbor(&bytes, DomainLimits { cells: 24 }).unwrap(),
        grid
    );
    assert_eq!(
        DomainDescriptor::from_cbor(&bytes, DomainLimits { cells: 23 })
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let mut translated = domain.clone();
    translated.lower_metres[0] = n(9.0);
    translated.upper_metres[0] = n(11.0);
    assert_ne!(
        domain.to_cbor(DomainLimits::default()).unwrap(),
        translated.to_cbor(DomainLimits::default()).unwrap()
    );
}

#[test]
fn domain_rejects_degenerate_overflowing_unrepresentable_and_hostile_descriptors() {
    let original = domain();
    for cells in [[0, 1, 1], [u32::MAX, u32::MAX, u32::MAX]] {
        let mut bad = original.clone();
        bad.discretization = SpatialDiscretization::CartesianCells { cells };
        assert!(bad.to_cbor(DomainLimits { cells: u64::MAX }).is_err());
    }
    let mut bad = original.clone();
    bad.upper_metres[0] = bad.lower_metres[0];
    assert!(bad.to_cbor(DomainLimits::default()).is_err());
    bad = original.clone();
    bad.lower_metres[0] = n(-f64::MAX);
    bad.upper_metres[0] = n(f64::MAX);
    assert!(bad.to_cbor(DomainLimits::default()).is_err());
    bad = original.clone();
    bad.lower_metres[0] = n(1.0);
    bad.upper_metres[0] = n(f64::from_bits(1.0f64.to_bits() + 1));
    bad.discretization = SpatialDiscretization::CartesianCells { cells: [4, 1, 1] };
    assert!(bad.to_cbor(DomainLimits::default()).is_err());
    let bytes = original.to_cbor(DomainLimits::default()).unwrap();
    for n in 0..bytes.len() {
        assert!(DomainDescriptor::from_cbor(&bytes[..n], DomainLimits::default()).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(DomainDescriptor::from_cbor(&trailing, DomainLimits::default()).is_err());
    assert!(
        DomainDescriptor::from_cbor(&vec![0; MAX_DOMAIN_BYTES + 1], DomainLimits::default())
            .is_err()
    );
    let mut bad = original.clone();
    bad.api_version = "orishu.simulation.domain/v2".parse().unwrap();
    assert!(bad.to_cbor(DomainLimits::default()).is_err());
}
