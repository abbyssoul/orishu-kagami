//! Independent composition-to-packet agreement. No authoring expression engine.
use super::*;

fn refused() -> Error {
    invalid(
        "scene",
        "scene composition differs from selected declarations or numerical inputs",
    )
}
fn scalar(component: &SceneComponent, property: &LocalContributionId) -> Result<FiniteF64, Error> {
    let p = component
        .properties
        .binary_search_by(|p| p.id.cmp(property))
        .map_err(|_| refused())?;
    match &component.properties[p].value {
        ConfigurationValue::Quantity { value_si, .. } => Ok(*value_si),
        _ => Err(refused()),
    }
}
fn component<'a>(
    object: &'a SceneObject,
    reference: &ContributionRef,
) -> Option<&'a SceneComponent> {
    object
        .components
        .binary_search_by(|c| c.contribution.cmp(reference))
        .ok()
        .map(|i| &object.components[i])
}
pub(super) fn verify(
    scene: &SceneDefinition,
    selected: &VerifiedSelection,
    execution: &ExecutionDefinition,
    objects: &BTreeMap<EntityId, ObjectState>,
    contexts: &BTreeMap<w::ComponentInstanceId, InstanceContext>,
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ProfileLimits,
) -> Result<(), Error> {
    if scene.objects.len() != objects.len() || scene.kernels.len() != contexts.len() {
        return Err(refused());
    }
    let dynamics = &contexts[&execution.dynamics.instance];
    let Payload::Integrators(integrator) = &selected.payloads()[&dynamics.contribution] else {
        unreachable!("verified integrator")
    };
    let dynamic_type = &selected
        .descriptor()
        .bindings
        .iter()
        .find(|b| {
            b.consumer == dynamics.contribution
                && b.requirement_slot == integrator.scientific.dynamics
        })
        .expect("verified binding")
        .provider;
    for object in &scene.objects {
        let numeric = objects.get(&object.id).ok_or_else(refused)?;
        if numeric.kinematics != object.kinematics {
            return Err(refused());
        }
        let mut mass = None;
        for component in &object.components {
            let Some(Payload::Components(declaration)) =
                selected.payloads().get(&component.contribution)
            else {
                return Err(refused());
            };
            let schema = &declaration.scientific;
            if component.properties.len() > limits.selection.declarations.max_schema_items {
                return Err(limited("scene.properties"));
            }
            for value in &component.properties {
                let property = schema
                    .properties
                    .iter()
                    .find(|p| p.id == value.id)
                    .ok_or_else(refused)?;
                if !property.schema.accepts(value.value.as_property_value()) {
                    return Err(refused());
                }
            }
            if schema.properties.iter().any(|p| {
                p.required
                    && component
                        .properties
                        .binary_search_by(|v| v.id.cmp(&p.id))
                        .is_err()
            }) {
                return Err(refused());
            }
            if schema.role == ComponentRole::Dynamics {
                if &component.contribution != dynamic_type {
                    return Err(refused());
                }
                mass = Some(scalar(
                    component,
                    schema
                        .bindings
                        .inertial_mass
                        .as_ref()
                        .expect("Dynamics role"),
                )?);
            }
        }
        if mass != numeric.inertial_mass_kilograms {
            return Err(refused());
        }
        // The initial contract integrates translations, not rigid-body rotation.
        if mass.is_some() && object.angular_velocity.iter().any(|v| v.get() != 0.0) {
            return Err(invalid(
                "scene.angularVelocity",
                "selected fixed profile does not integrate angular motion",
            ));
        }
    }
    for source in &scene.kernels {
        let context = contexts.get(&source.instance).ok_or_else(refused)?;
        let config = ResolvedConfiguration::from_cbor(
            blob(
                context.configuration.digest,
                context.configuration.byte_length,
                blobs,
            )?,
            &limits.selection.declarations,
        )?;
        if source
            .authored
            .iter()
            .any(|id| config.get(id.as_str()).is_none())
        {
            return Err(refused());
        }
        for quantity in &source.quantities {
            if !matches!(
                config.get(quantity.id.as_str()),
                Some(ConfigurationValue::Quantity { .. })
            ) {
                return Err(refused());
            }
        }
    }
    let mut work = 0usize;
    for field in &execution.fields {
        let context = &contexts[&field.kernel.instance];
        work = scene
            .objects
            .len()
            .checked_mul(context.couplings.len())
            .and_then(|n| work.checked_add(n))
            .filter(|n| *n <= limits.scene.projection_work)
            .ok_or_else(|| limited("scene.projection"))?;
    }
    for field in &execution.fields {
        let context = &contexts[&field.kernel.instance];
        let providers: Vec<_> = context
            .couplings
            .iter()
            .map(|slot| {
                &selected
                    .descriptor()
                    .bindings
                    .iter()
                    .find(|b| b.consumer == context.contribution && b.requirement_slot == slot.slot)
                    .expect("verified coupling")
                    .provider
            })
            .collect();
        let batch = packet::<CoupledEntity>(
            &field.coupled,
            blob(field.coupled.digest, field.coupled.byte_length, blobs)?,
            limits.couplings,
        )?;
        let mut actual = batch.iter();
        for object in &scene.objects {
            for (slot, provider) in providers.iter().enumerate() {
                let Some(component) = component(object, provider) else {
                    continue;
                };
                let roles = &context.couplings[slot];
                let expected = CoupledEntity {
                    id: object.id,
                    slot: CouplingSlot(slot as u32),
                    kinematics: object.kinematics,
                    has_dynamics: objects[&object.id].inertial_mass_kilograms.is_some(),
                    source_si: roles
                        .source
                        .as_ref()
                        .map(|p| scalar(component, &p.property))
                        .transpose()?,
                    response_si: roles
                        .response
                        .as_ref()
                        .map(|p| scalar(component, &p.property))
                        .transpose()?,
                };
                if actual.next() != Some(expected) {
                    return Err(refused());
                }
            }
        }
        if actual.next().is_some() {
            return Err(refused());
        }
    }
    Ok(())
}
