//! One explicit graph for the fixed force-then-integrate profile. Admission
//! compares this complete projection, so unhandled graph knobs cannot be ignored.
use super::*;

pub(super) fn build(
    selected: &VerifiedSelection,
    e: &ExecutionDefinition,
    contexts: &BTreeMap<w::ComponentInstanceId, InstanceContext>,
) -> Result<w::ComputeSpec, Error> {
    let mut components = Vec::new();
    let mut channels = Vec::new();
    let mut invocations = Vec::new();
    let objects: w::StateChannelId = name("objects")?;
    let history: w::StateChannelId = name("history")?;
    let forces: w::StateChannelId = name("forces")?;
    for (i, field) in e.fields.iter().enumerate() {
        let state = name::<w::StateChannelId>(&format!("field-{i}"))?;
        let context = &contexts[&field.kernel.instance];
        components.push(component(selected, context, vec![state.clone()])?);
        channels.push(channel(
            state.clone(),
            field.kernel.state.schema.clone(),
            Some(context.instance.clone()),
            w::Reduction::Single,
        ));
        invocations.push(w::StepInvocation {
            invocation_id: name(context.instance.as_str())?,
            instance: context.instance.clone(),
            phase_id: name("advance")?,
            inputs: vec![objects.clone(), state.clone()],
            outputs: vec![state, forces.clone()],
            depends_on: vec![],
        });
    }
    if !e.fields.is_empty() {
        channels.push(channel(
            forces.clone(),
            name(Force::SCHEMA)?,
            None,
            w::Reduction::Sum,
        ));
    }
    let c = &contexts[&e.dynamics.instance];
    components.push(component(
        selected,
        c,
        vec![objects.clone(), history.clone()],
    )?);
    channels.push(channel(
        objects.clone(),
        name(ObjectState::SCHEMA)?,
        Some(c.instance.clone()),
        w::Reduction::Single,
    ));
    channels.push(channel(
        history.clone(),
        e.dynamics.state.schema.clone(),
        Some(c.instance.clone()),
        w::Reduction::Single,
    ));
    let mut inputs = vec![objects.clone(), history.clone()];
    if !e.fields.is_empty() {
        inputs.push(forces);
    }
    invocations.push(w::StepInvocation {
        invocation_id: name(c.instance.as_str())?,
        instance: c.instance.clone(),
        phase_id: name("integrate")?,
        inputs,
        outputs: vec![objects, history],
        depends_on: invocations
            .iter()
            .map(|i| i.invocation_id.clone())
            .collect(),
    });
    // Component/channel sets have one canonical representation. Invocation order
    // remains fields then Dynamics; dependency order is canonical field order.
    components.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
    Ok(w::ComputeSpec {
        workload_graph_profile: name(PROFILE)?,
        components,
        channels,
        step_plan: w::StepPlan {
            profile: name(PROFILE)?,
            invocations,
        },
        placement_constraints: vec![],
    })
}
fn channel(
    channel_id: w::StateChannelId,
    schema: w::SchemaId,
    owner: Option<w::ComponentInstanceId>,
    reduction: w::Reduction,
) -> w::StateChannel {
    w::StateChannel {
        channel_id,
        schema: w::SchemaCompat {
            schema_id: schema,
            version: 1,
        },
        shape: vec![],
        owner,
        reduction,
    }
}
fn component(
    selected: &VerifiedSelection,
    c: &InstanceContext,
    state_ownership: Vec<w::StateChannelId>,
) -> Result<w::ComponentInstance, Error> {
    let code = &selected.artifacts()[&c.kernel];
    let (role, lifecycle) = match c.execution_contract {
        ExecutionContractId::Field => ("field-model", "orishu:simulation/field@1"),
        ExecutionContractId::Dynamics => ("dynamics", "orishu:simulation/dynamics@1"),
    };
    Ok(w::ComponentInstance {
        instance_id: c.instance.clone(),
        artifact: artifact(
            w::ArtifactRole::COMPONENT,
            code.digest,
            code.size_bytes,
            &code.media_type,
        )?,
        plugin_id: name(
            selected.releases()[&c.contribution.release]
                .plugin_id
                .as_str(),
        )?,
        model_id: name(c.scientific.name.as_str())?,
        schema_id: name(&format!(
            "{}/v{}",
            c.state_format.id, c.state_format.version
        ))?,
        engine: name("wasm-component")?,
        lifecycle: name(lifecycle)?,
        roles: vec![name(role)?],
        state_ownership,
        config: BTreeMap::new(),
        limits: BTreeMap::from([
            (name("state-bytes")?, c.bounds.state_bytes),
            (
                name("projection-records")?,
                u64::from(c.bounds.projection_records),
            ),
            (name("sample-points")?, u64::from(c.bounds.sample_points)),
            (
                name("sample-channels")?,
                u64::from(c.bounds.sample_channels),
            ),
        ]),
    })
}
