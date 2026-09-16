use orishu_plugin::{execution::*, *};

#[test]
fn empty_scene_canonical_golden() {
    let scene = SceneDefinition {
        api_version: SCENE_SCHEMA.parse().unwrap(),
        objects: vec![],
        variables: vec![],
        kernels: vec![],
    };
    let bytes = scene.to_cbor(Default::default()).unwrap();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        concat!(
            "a4676b65726e656c7380676f626a6563747380697661726961626c657380",
            "6a61706956657273696f6e781a6f72697368752e73696d756c6174696f6e2e7363656e652f7631"
        )
    );
    assert_eq!(
        SceneDefinition::from_cbor(&bytes, Default::default()).unwrap(),
        scene
    );
}

fn fixture() -> SceneDefinition {
    let zero = FiniteF64::ZERO;
    let one = FiniteF64::new(1.0).unwrap();
    SceneDefinition {
        api_version: SCENE_SCHEMA.parse().unwrap(),
        objects: vec![SceneObject {
            id: EntityId(7),
            name: "particle".into(),
            kinematics: Kinematics {
                position_metres: [zero; 3],
                velocity_metres_per_second: [one; 3],
            },
            orientation: [zero, zero, zero, one],
            angular_velocity: [zero; 3],
            shape: Some(SceneShape::Sphere { radius_metres: one }),
            components: vec![SceneComponent {
                contribution: ContributionRef {
                    release: format!("{}", ArtifactDigest::sha256_of(b"release"))
                        .parse()
                        .unwrap(),
                    extension_point: KnownPoint::Components.as_str().parse().unwrap(),
                    local_id: "mass".parse().unwrap(),
                },
                properties: vec![SceneProperty {
                    id: "mass".parse().unwrap(),
                    value: ConfigurationValue::Quantity {
                        value_si: one,
                        dimension: Dimension::MASS,
                    },
                    source: Some(QuantitySource {
                        expression: "1000".into(),
                        unit: Some("g".into()),
                    }),
                }],
            }],
            template: Some(TemplateEvidence {
                api_version: "kagami.dev/v2".into(),
                fingerprint: ArtifactDigest::sha256_of(b"historical source"),
            }),
        }],
        variables: vec![VariableEvidence {
            id: 1,
            name: "mass".into(),
            expression: "1000 g".into(),
            description: Some("retained, never evaluated by a worker".into()),
        }],
        kernels: vec![KernelSource {
            instance: "field".parse().unwrap(),
            authored: vec!["capacity".parse().unwrap()],
            quantities: vec![PropertySource {
                id: "capacity".parse().unwrap(),
                source: QuantitySource {
                    expression: "64".into(),
                    unit: None,
                },
            }],
        }],
    }
}

#[test]
fn canonical_scene_preserves_composition_units_geometry_and_nonresolving_evidence() {
    let scene = fixture();
    let limits = SceneLimits::default();
    let bytes = scene.to_cbor(limits).unwrap();
    assert_eq!(SceneDefinition::from_cbor(&bytes, limits).unwrap(), scene);
    for end in 0..bytes.len() {
        assert!(SceneDefinition::from_cbor(&bytes[..end], limits).is_err());
    }
    assert!(
        SceneDefinition::from_cbor(
            &bytes,
            SceneLimits {
                bytes: bytes.len() - 1,
                ..limits
            }
        )
        .is_err()
    );
    let mut different = scene.clone();
    different.objects[0].components[0].properties[0]
        .source
        .as_mut()
        .unwrap()
        .expression = "1e3".into();
    assert_ne!(
        bytes,
        different.to_cbor(limits).unwrap(),
        "source evidence participates in identity"
    );
    assert_eq!(
        different.objects[0].components[0].properties[0].value,
        scene.objects[0].components[0].properties[0].value
    );
}

#[test]
fn raw_and_decoded_scene_limits_and_structural_refusals_agree() {
    let scene = fixture();
    let bytes = scene.to_cbor(Default::default()).unwrap();
    for limits in [
        SceneLimits {
            objects: 0,
            ..Default::default()
        },
        SceneLimits {
            components: 0,
            ..Default::default()
        },
        SceneLimits {
            properties: 1,
            ..Default::default()
        },
        SceneLimits {
            variables: 0,
            ..Default::default()
        },
        SceneLimits {
            kernels: 0,
            ..Default::default()
        },
        SceneLimits {
            text_bytes: 3,
            ..Default::default()
        },
        SceneLimits {
            values: 1,
            ..Default::default()
        },
    ] {
        assert!(scene.to_cbor(limits).is_err());
        assert!(SceneDefinition::from_cbor(&bytes, limits).is_err());
    }
    for case in 0..8 {
        let mut s = scene.clone();
        match case {
            0 => s.objects.push(s.objects[0].clone()),
            1 => {
                let c = s.objects[0].components[0].clone();
                s.objects[0].components.push(c);
            }
            2 => {
                let p = s.objects[0].components[0].properties[0].clone();
                s.objects[0].components[0].properties.push(p);
            }
            3 => s.objects[0].orientation = [FiniteF64::ZERO; 4],
            4 => {
                s.objects[0].shape = Some(SceneShape::Sphere {
                    radius_metres: FiniteF64::ZERO,
                })
            }
            5 => {
                s.objects[0].components[0].properties[0].value =
                    ConfigurationValue::Boolean { value: true }
            }
            6 => s.kernels.push(s.kernels[0].clone()),
            _ => s.api_version = "orishu.simulation.scene/v99".parse().unwrap(),
        }
        assert!(s.to_cbor(Default::default()).is_err(), "case {case}");
    }
    // Several individually allowed strings must still fit the aggregate before
    // any canonical tree is copied; max text alone is not an allocation budget.
    let mut s = scene;
    s.variables[0].expression = "x".repeat(10_000);
    s.objects[0].name = "y".repeat(10_000);
    assert!(
        s.to_cbor(SceneLimits {
            bytes: 16_000,
            ..Default::default()
        })
        .is_err()
    );
}
