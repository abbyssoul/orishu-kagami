//! Shared initial/history projection from exact declared Dynamics vocabulary.
use super::*;

pub(super) struct DynamicsProjection<'a> {
    state: &'a ExperimentState,
    component: kagami_catalog::ComponentTypeId,
    property: kagami_catalog::PropertyName,
    schema: kagami_catalog::PropertySchema,
    version: u32,
}
impl<'a> DynamicsProjection<'a> {
    pub(super) fn new(
        state: &'a ExperimentState,
        selected: &VerifiedDeclarations,
        integrator: &ContributionRef,
    ) -> Result<Self, ScientificError> {
        let Some(Payload::Integrators(model)) = selected.payloads().get(integrator) else {
            return Err(ScientificError::Mismatch);
        };
        let binding = selected
            .descriptor()
            .bindings
            .iter()
            .find(|b| &b.consumer == integrator && b.requirement_slot == model.scientific.dynamics)
            .ok_or(ScientificError::Mismatch)?;
        let Some(Payload::Components(component)) = selected.payloads().get(&binding.provider)
        else {
            return Err(ScientificError::Mismatch);
        };
        let mass = component
            .scientific
            .bindings
            .inertial_mass
            .as_ref()
            .ok_or(ScientificError::Mismatch)?;
        let declaration = component
            .scientific
            .properties
            .iter()
            .find(|p| &p.id == mass)
            .ok_or(ScientificError::Mismatch)?;
        Ok(Self {
            state,
            component: kagami_catalog::ComponentTypeId::exact(binding.provider.clone())
                .map_err(|_| ScientificError::Mismatch)?,
            property: kagami_catalog::schema::property_name(mass)
                .map_err(|_| ScientificError::Mismatch)?,
            schema: kagami_catalog::PropertySchema::from_plugin(declaration),
            version: component.scientific.version.get(),
        })
    }
    pub(super) fn len(&self) -> usize {
        self.state
            .objects
            .values()
            .filter(|o| o.components.contains_key(&self.component))
            .count()
    }
    pub(super) fn visit(
        &self,
        variables: &VariablesSystem,
        limits: &crate::Limits,
        mut consume: impl FnMut(DynamicEntity) -> Result<(), ScientificError>,
    ) -> Result<(), ScientificError> {
        for (id, object) in self.state.objects.iter() {
            let Some(component) = object.components.get(&self.component) else {
                continue;
            };
            if component.schema_version.0 != self.version {
                return Err(ScientificError::History);
            }
            let value = component
                .properties
                .get(&self.property)
                .ok_or(ScientificError::History)?;
            // Reprice authored source under retained exact vocabulary. Offline
            // hydration need not have priced this component in its registry.
            let value = crate::validate::resolve_property(
                crate::PropertyPath::new(*id, self.component.clone(), self.property.clone()),
                &self.schema,
                &value.authored(),
                variables,
                limits,
            )
            .map_err(|_| ScientificError::History)?;
            let mass = FiniteF64::new(value.si_value().ok_or(ScientificError::History)?)
                .map_err(|_| ScientificError::History)?;
            if mass.get() <= 0.0 {
                return Err(ScientificError::History);
            }
            let vector = |v: crate::Vector3| {
                v.to_array()
                    .map(|n| FiniteF64::new(n).expect("validated authored kinematics"))
            };
            consume(DynamicEntity {
                id: EntityId(id.get()),
                kinematics: Kinematics {
                    position_metres: vector(object.transform.translation),
                    velocity_metres_per_second: vector(object.velocity.linear),
                },
                inertial_mass_kilograms: mass,
            })?;
        }
        Ok(())
    }
}

/// Encode the selected integrator's exact initial dynamic objects before any
/// history exists. Shares its projection with final captured-history validation.
/// Static/kinematic objects are absent, not zero-mass Dynamic records. Bounds are
/// checked before allocating records/packet; no guest or IO runs in this crate.
pub fn initial_dynamics(
    snapshot: &crate::ExperimentSnapshot,
    selected: &VerifiedDeclarations,
    instance: &ComponentInstanceId,
    variables: &VariablesSystem,
    authoring: &crate::Limits,
    bulk: BulkLimits,
) -> Result<Vec<u8>, ScientificError> {
    if snapshot.object_count() > authoring.max_objects {
        return Err(ScientificError::Limit);
    }
    let kernel = selected
        .descriptor()
        .kernel_instances
        .iter()
        .find(|k| {
            &k.instance_id == instance && k.execution_contract == ExecutionContractId::Dynamics
        })
        .ok_or(ScientificError::Mismatch)?;
    let projection = DynamicsProjection::new(snapshot.state(), selected, &kernel.contribution)?;
    let count = projection.len();
    packet_bytes::<DynamicEntity>(count, bulk).map_err(|_| ScientificError::Limit)?;
    let mut records = Vec::with_capacity(count);
    projection.visit(variables, authoring, |v| {
        records.push(v);
        Ok(())
    })?;
    let mut bytes = Vec::new();
    encode_batch(&records, &mut bytes, bulk).map_err(|_| ScientificError::History)?;
    Ok(bytes)
}
