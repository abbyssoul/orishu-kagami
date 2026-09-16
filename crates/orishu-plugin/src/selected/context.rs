//! Agreement between admitted selection and a concrete scientific kernel use.
use super::*;
use crate::execution::{CouplingDescriptor, CouplingProperty, InstanceContext};

/// Explicit non-declaration inputs to a configured kernel use. These are host/
/// author choices, not inferred scientific defaults. Input bytes still need
/// validation against their identities and the selected configuration schema.
#[derive(Clone, Debug)]
pub struct ContextInputs {
    /// Exact resolved configuration descriptor.
    pub configuration: execution::InputIdentity,
    /// Exact shared geometric domain descriptor.
    pub domain: execution::InputIdentity,
    /// Requested computational precision; the kernel may refuse it.
    pub compute_precision: execution::ComputePrecision,
    /// Explicit host ceilings, checked against the declaration where applicable.
    pub bounds: execution::ExecutionBounds,
    /// Allowed sample quality flags by observable requirement slot. Must cover
    /// exactly the selected model's channels (empty for Dynamics). This is a
    /// requested allowance, not a claim that a kernel can produce each quality.
    pub quality_flags: BTreeMap<LocalContributionId, u32>,
}

/// Project exact selected declarations into portable instance metadata.
///
/// No provider is selected here: the instance and every dependency must already
/// be in the verified closure. In particular, coupling roles and channel shapes
/// come from selected vocabulary, not conventional property names. The returned
/// context is independently checked through [`verify_context`]. This performs no
/// IO, guest execution, configuration evaluation or document/run mutation.
pub fn build_context(
    selected: &VerifiedSelection,
    instance: &ComponentInstanceId,
    inputs: ContextInputs,
    limits: &Limits,
) -> Result<InstanceContext, Error> {
    use execution::{ObservableBinding, SampleChannel};
    let use_ = selected
        .descriptor()
        .kernel_instances
        .iter()
        .find(|k| &k.instance_id == instance)
        .ok_or_else(|| invalid("instance", "instance absent from selected kernel uses"))?;
    let payload = &selected.payloads()[&use_.contribution];
    let provider = |slot: &LocalContributionId| {
        let binding = selected
            .descriptor()
            .bindings
            .iter()
            .find(|b| b.consumer == use_.contribution && &b.requirement_slot == slot)
            .expect("verified selection binds every requirement");
        &selected.payloads()[&binding.provider]
    };
    let mut couplings = Vec::new();
    let mut observables = Vec::new();
    let (state_format, profile) = match payload {
        Payload::FieldModels(declaration) => {
            let model = &declaration.scientific;
            if inputs.quality_flags.len() != model.observables.len() {
                return Err(invalid(
                    "observables",
                    "quality policy must cover every selected channel",
                ));
            }
            for slot in &model.couplings {
                let p = provider(slot);
                let Payload::Components(component) = p else {
                    unreachable!("verified coupling provider")
                };
                let property = |id: &Option<LocalContributionId>| {
                    id.as_ref().map(|id| {
                        let property = component
                            .scientific
                            .properties
                            .iter()
                            .find(|p| &p.id == id)
                            .expect("validated role property");
                        let PropertyType::Quantity { dimension, .. } = property.schema else {
                            unreachable!("validated scalar role")
                        };
                        CouplingProperty {
                            property: id.clone(),
                            dimension,
                        }
                    })
                };
                couplings.push(CouplingDescriptor {
                    slot: slot.clone(),
                    component: p.contract_ref(limits)?,
                    source: property(&component.scientific.bindings.source),
                    response: property(&component.scientific.bindings.response),
                });
            }
            for slot in &model.observables {
                let p = provider(slot);
                let Payload::Observables(channel) = p else {
                    unreachable!("verified observable provider")
                };
                observables.push(ObservableBinding {
                    slot: slot.clone(),
                    channel: SampleChannel {
                        contract: p.contract_ref(limits)?,
                        schema: channel.scientific.clone(),
                    },
                    quality_flags: *inputs.quality_flags.get(slot).ok_or_else(|| {
                        invalid("observables", "missing selected channel quality policy")
                    })?,
                });
            }
            (model.state_format.clone(), model.profile)
        }
        Payload::Integrators(declaration) => {
            if !inputs.quality_flags.is_empty() {
                return Err(invalid("observables", "Dynamics has no field channels"));
            }
            (
                declaration.scientific.history.format.clone(),
                declaration.scientific.profile,
            )
        }
        _ => {
            return Err(invalid(
                "instance",
                "selected declaration is not executable",
            ));
        }
    };
    couplings.sort_by(|a, b| a.slot.cmp(&b.slot));
    observables.sort_by(|a, b| a.slot.cmp(&b.slot));
    let context = InstanceContext {
        api_version: execution::INSTANCE_SCHEMA.parse().expect("static schema"),
        instance: instance.clone(),
        kernel: payload.kernel().expect("executable declaration"),
        contribution: use_.contribution.clone(),
        scientific: payload.contract_ref(limits)?,
        execution_contract: use_.execution_contract,
        state_format,
        profile,
        configuration: inputs.configuration,
        domain: inputs.domain,
        couplings,
        observables,
        compute_precision: inputs.compute_precision,
        bounds: inputs.bounds,
    };
    verify_context(selected, &context, limits)?;
    Ok(context)
}

/// Verify all declaration-derived context fields against the exact selected use.
/// Input bytes/configuration/domain admissibility and portable state must still
/// be verified separately. Negotiated precision/sampling quality are kernel input,
/// not permission to invent observable vocabulary or source/response dimensions.
pub fn verify_context(
    selected: &VerifiedSelection,
    context: &InstanceContext,
    limits: &Limits,
) -> Result<(), Error> {
    context.to_cbor()?;
    let instance = selected
        .descriptor()
        .kernel_instances
        .iter()
        .find(|k| k.instance_id == context.instance)
        .ok_or_else(|| invalid("instance", "instance absent from selected kernel uses"))?;
    if instance.contribution != context.contribution
        || instance.execution_contract != context.execution_contract
    {
        return Err(invalid(
            "instance",
            "selected instance role or contribution mismatch",
        ));
    }
    let payload = &selected.payloads()[&context.contribution];
    if payload.kernel() != Some(context.kernel)
        || payload.contract_ref(limits)? != context.scientific
    {
        return Err(invalid(
            "instance",
            "kernel artifact or scientific identity mismatch",
        ));
    }
    let provider = |slot: &LocalContributionId| {
        selected
            .descriptor()
            .bindings
            .iter()
            .find(|b| b.consumer == context.contribution && &b.requirement_slot == slot)
            .map(|b| &b.provider)
            .expect("verified selection has every requirement")
    };
    match payload {
        Payload::FieldModels(declaration) => {
            let model = &declaration.scientific;
            if context.state_format != model.state_format
                || context.profile != model.profile
                || context.bounds.state_bytes > model.max_state_bytes
            {
                return Err(invalid(
                    "instance",
                    "field format/profile/state bound mismatch",
                ));
            }
            let mut expected = Vec::with_capacity(model.couplings.len());
            for slot in &model.couplings {
                let p = &selected.payloads()[provider(slot)];
                let Payload::Components(component) = p else {
                    unreachable!("verified coupling role")
                };
                let s = &component.scientific;
                let property = |name: &Option<LocalContributionId>| {
                    name.as_ref().map(|name| {
                        let value = s
                            .properties
                            .iter()
                            .find(|p| &p.id == name)
                            .expect("validated role binding");
                        let PropertyType::Quantity { dimension, .. } = value.schema else {
                            unreachable!("validated scalar role")
                        };
                        CouplingProperty {
                            property: name.clone(),
                            dimension,
                        }
                    })
                };
                expected.push(CouplingDescriptor {
                    slot: slot.clone(),
                    component: p.contract_ref(limits)?,
                    source: property(&s.bindings.source),
                    response: property(&s.bindings.response),
                });
            }
            expected.sort_by(|a, b| a.slot.cmp(&b.slot));
            if context.couplings != expected {
                return Err(invalid(
                    "couplings",
                    "context changes selected source/response semantics",
                ));
            }
            if context.observables.len() != model.observables.len() {
                return Err(invalid("observables", "context channel coverage mismatch"));
            }
            for binding in &context.observables {
                if !model.observables.contains(&binding.slot) {
                    return Err(invalid("observables", "context adds an undeclared channel"));
                }
                let p = &selected.payloads()[provider(&binding.slot)];
                let Payload::Observables(channel) = p else {
                    unreachable!("verified observable role")
                };
                if binding.channel.schema != channel.scientific
                    || binding.channel.contract != p.contract_ref(limits)?
                {
                    return Err(invalid(
                        "observables",
                        "context changes selected channel semantics",
                    ));
                }
            }
        }
        Payload::Integrators(declaration) => {
            let model = &declaration.scientific;
            if context.state_format != model.history.format || context.profile != model.profile {
                return Err(invalid("instance", "integrator format/profile mismatch"));
            }
        }
        _ => {
            return Err(invalid(
                "instance",
                "selected declaration is not executable",
            ));
        }
    }
    Ok(())
}
