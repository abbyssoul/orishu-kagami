//! One-way compilation from the authoring authority into shared scene data.
use super::*;
use orishu_variables::VariablesSystem;

struct Budget {
    bytes: usize,
    text: usize,
}
impl Budget {
    fn charge(&mut self, bytes: usize) -> Result<(), ProjectionError> {
        self.bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or(ProjectionError::Limit)?;
        Ok(())
    }
    fn text(&mut self, s: &str) -> Result<(), ProjectionError> {
        if s.len() > self.text {
            return Err(ProjectionError::Limit);
        }
        self.charge(s.len())
    }
}

pub(super) fn build(
    snapshot: &ExperimentSnapshot,
    selected: &VerifiedDeclarations,
    variables: &VariablesSystem,
    authoring: &Limits,
    policy: SceneLimits,
) -> Result<Vec<u8>, ProjectionError> {
    if snapshot.object_count() > policy.objects
        || snapshot.variable_count() > policy.variables
        || selected.descriptor().kernel_instances.len() > policy.kernels
    {
        return Err(ProjectionError::Limit);
    }
    // Conservative cold-output reservation before copying any authored strings.
    // It is a logical byte budget, not a promise about allocator overhead/RSS.
    let mut budget = Budget {
        bytes: policy.bytes,
        text: policy.text_bytes,
    };
    let mut components = 0usize;
    let mut properties = 0usize;
    for object in snapshot.objects().values() {
        budget.charge(512)?;
        budget.text(object.name.as_str())?;
        components = components
            .checked_add(object.components.len())
            .filter(|n| *n <= policy.components)
            .ok_or(ProjectionError::Limit)?;
        if let Some(p) = &object.provenance {
            budget.text(&p.api_version)?;
        }
        for component in object.components.values() {
            budget.charge(512)?;
            properties = properties
                .checked_add(component.properties.len())
                .filter(|n| *n <= policy.properties)
                .ok_or(ProjectionError::Limit)?;
            for value in component.properties.values() {
                budget.charge(256)?;
                if let Some(s) = value.source() {
                    budget.text(s)?;
                }
                if let Some(u) = value.display_unit() {
                    budget.text(u)?;
                    if orishu_variables::lookup(u).is_err() {
                        return Err(Error {
                            code: ErrorCode::InvalidSelection,
                            path: "scene.unit".into(),
                            message: "unknown authored unit annotation".into(),
                        }
                        .into());
                    }
                }
                if let crate::PropertyValue::Text(s) = value {
                    budget.text(s)?;
                }
            }
        }
    }
    for variable in snapshot.variables().values() {
        budget.charge(64)?;
        budget.text(&variable.qualified_name())?;
        budget.text(&variable.expression)?;
        if let Some(d) = &variable.description {
            budget.text(d)?;
        }
    }
    let setup = snapshot
        .setup()
        .scientific()
        .expect("scientific projection");
    for capture in setup.captures().values() {
        budget.charge(256)?;
        properties = properties
            .checked_add(capture.authored.len())
            .filter(|n| *n <= policy.properties)
            .ok_or(ProjectionError::Limit)?;
        for p in &capture.authored {
            budget.charge(256)?;
            if let ConfigurationInput::Expression { source } = &p.input {
                properties = properties
                    .checked_add(1)
                    .filter(|n| *n <= policy.properties)
                    .ok_or(ProjectionError::Limit)?;
                budget.charge(256)?;
                budget.text(source)?;
            }
        }
    }
    let finite = |v| FiniteF64::new(v).expect("validated authored finite value");
    let vector = |v: crate::Vector3| v.to_array().map(finite);
    let mut objects = Vec::with_capacity(snapshot.object_count());
    for (id, object) in snapshot.objects() {
        let mut components = Vec::with_capacity(object.components.len());
        for (kind, component) in &object.components {
            let reference = kind
                .contribution()
                .expect("projection checked exact component");
            let Payload::Components(declaration) = &selected.payloads()[reference] else {
                unreachable!("verified component")
            };
            let mut properties = Vec::with_capacity(component.properties.len());
            for declared in &declaration.scientific.properties {
                let key = property_name(&declared.id).expect("valid property ID");
                let Some(value) = component.properties.get(&key) else {
                    if declared.required {
                        return Err(Error {
                            code: ErrorCode::InvalidSelection,
                            path: "scene.property".into(),
                            message: "required authored property absent".into(),
                        }
                        .into());
                    }
                    continue;
                };
                let resolved = crate::validate::resolve_property(
                    PropertyPath::new(*id, kind.clone(), key),
                    &PropertySchema::from_plugin(declared),
                    &value.authored(),
                    variables,
                    authoring,
                )?;
                let resolved = match resolved {
                    crate::PropertyValue::Quantity {
                        si_value,
                        dimension,
                        ..
                    } => ConfigurationValue::Quantity {
                        value_si: finite(si_value),
                        dimension,
                    },
                    crate::PropertyValue::Boolean(value) => ConfigurationValue::Boolean { value },
                    crate::PropertyValue::Text(value) => ConfigurationValue::Text { value },
                    crate::PropertyValue::Unresolved { .. } => {
                        unreachable!("exact declaration resolves property")
                    }
                };
                properties.push(SceneProperty {
                    id: declared.id.clone(),
                    value: resolved,
                    source: value.source().map(|expression| QuantitySource {
                        expression: expression.into(),
                        unit: value.display_unit().map(str::to_owned),
                    }),
                });
            }
            properties.sort_by(|a, b| a.id.cmp(&b.id));
            components.push(SceneComponent {
                contribution: reference.clone(),
                properties,
            });
        }
        components.sort_by(|a, b| a.contribution.cmp(&b.contribution));
        let shape = object.shape.map(|shape| match shape {
            crate::ObjectShape::Sphere { radius } => SceneShape::Sphere {
                radius_metres: finite(radius),
            },
            crate::ObjectShape::Box { half_extent } => SceneShape::Box {
                half_extent_metres: vector(half_extent),
            },
        });
        objects.push(SceneObject {
            id: EntityId(id.get()),
            name: object.name.as_str().into(),
            kinematics: Kinematics {
                position_metres: vector(object.transform.translation),
                velocity_metres_per_second: vector(object.velocity.linear),
            },
            orientation: object.transform.rotation.to_array().map(finite),
            angular_velocity: vector(object.velocity.angular),
            shape,
            components,
            template: object.provenance.as_ref().map(|p| TemplateEvidence {
                api_version: p.api_version.clone(),
                fingerprint: format!("sha256:{}", p.fingerprint)
                    .parse()
                    .expect("canonical template SHA-256"),
            }),
        });
    }
    let variables = snapshot
        .variables()
        .iter()
        .map(|(id, v)| VariableEvidence {
            id: id.get(),
            name: v.qualified_name(),
            expression: v.expression.clone(),
            description: v.description.clone(),
        })
        .collect();
    let kernels = setup
        .captures()
        .iter()
        .map(|(instance, c)| {
            let mut quantities: Vec<_> = c
                .authored
                .iter()
                .filter_map(|p| match &p.input {
                    ConfigurationInput::Expression { source } => Some(PropertySource {
                        id: p.id.clone(),
                        source: QuantitySource {
                            expression: source.clone(),
                            unit: None,
                        },
                    }),
                    _ => None,
                })
                .collect();
            quantities.sort_by(|a, b| a.id.cmp(&b.id));
            KernelSource {
                instance: instance.clone(),
                authored: {
                    let mut ids: Vec<_> = c.authored.iter().map(|p| p.id.clone()).collect();
                    ids.sort();
                    ids
                },
                quantities,
            }
        })
        .collect();
    Ok(SceneDefinition {
        api_version: SCENE_SCHEMA.parse().expect("static schema"),
        objects,
        variables,
        kernels,
    }
    .to_cbor(policy)?)
}
