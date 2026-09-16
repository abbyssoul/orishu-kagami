//! Cold, kernel-independent numerical projection of one captured document revision.
//! Scene composition and authoring evidence accompany the numerical packets.
//! Emitter closure still needs its explicit schema/profile extension. No IO or initialization
//! occurs here, and unsupported component intent is refused rather than dropped.

use crate::{ExperimentSnapshot, Limits, ObjectId, PropertyPath, Rejection, Setup};
use kagami_catalog::{ComponentTypeId, PropertySchema, schema::property_name};
use orishu_plugin::{execution::*, selected::VerifiedDeclarations, workload::ProfileLimits, *};
use std::{collections::BTreeMap, sync::Arc};
mod scene;

/// Why a document cannot produce this first-pass numerical projection.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    /// Legacy domain/boundary intent requires explicit scientific migration.
    #[error("an explicitly captured scientific setup is required")]
    LegacySetup,
    /// The fixed profile requires exactly one selected integrator.
    #[error("the fixed execution profile requires a captured integrator")]
    MissingIntegrator,
    /// No silent removal of components, properties or unknown scientific roles.
    #[error("object {object:?} has intent outside the fixed numerical profile: {component}")]
    UnsupportedComponent {
        /// Exact object carrying unsupported intent.
        object: ObjectId,
        /// Unmodeled or mismatched component identity.
        component: ComponentTypeId,
    },
    /// Byte, object, field or coupling policy exceeded before output allocation.
    #[error("document execution projection exceeds its resource budget")]
    Limit,
    /// Authored property/variable could not be resolved under its exact schema.
    #[error(transparent)]
    Authoring(#[from] Rejection),
    /// Captured configuration/history no longer describes the source revision.
    #[error(transparent)]
    Capture(#[from] crate::scientific::ScientificError),
    /// Shared scientific metadata refusal.
    #[error(transparent)]
    Scientific(#[from] Error),
    /// Standard numerical packet refusal.
    #[error(transparent)]
    Bulk(#[from] BulkError),
}

/// Numerical inputs pinned to a specific authored revision. Opaque captured bytes
/// remain shared. The scene artifact contains component values and authoring
/// evidence; the source snapshot also remains available to the calling adapter.
#[derive(Debug)]
pub struct ExecutionProjection {
    source: ExperimentSnapshot,
    definition: ExecutionDefinition,
    blobs: BTreeMap<ArtifactDigest, Arc<[u8]>>,
}
impl ExecutionProjection {
    /// Exact source revision, including presentation outside workload identity.
    pub fn source(&self) -> &ExperimentSnapshot {
        &self.source
    }
    /// Fixed scientific boundary assembled without running any kernel.
    pub fn definition(&self) -> &ExecutionDefinition {
        &self.definition
    }
    /// Required scene/numerical inputs; excludes executable code and release evidence.
    pub fn blobs(&self) -> &BTreeMap<ArtifactDigest, Arc<[u8]>> {
        &self.blobs
    }
}

struct Component<'a> {
    schema: &'a ComponentSchema,
    properties: Vec<&'a LocalContributionId>,
}

fn provider<'a>(
    selected: &'a VerifiedDeclarations,
    consumer: &ContributionRef,
    slot: &LocalContributionId,
) -> &'a ContributionRef {
    &selected
        .descriptor()
        .bindings
        .iter()
        .find(|b| &b.consumer == consumer && &b.requirement_slot == slot)
        .expect("verified requirement binding")
        .provider
}
fn type_id(reference: &ContributionRef) -> ComponentTypeId {
    ComponentTypeId::exact(reference.clone()).expect("verified component identity")
}

/// Resolve exact role-bound properties into the standard field and Dynamics
/// packets. This consumes retained verified vocabulary, not installed defaults,
/// display names or cached SI magnitudes. Selection/code availability and full
/// workload verification remain mandatory at the next boundary.
///
/// Cold work is bounded by objects, attached properties, selected coupling slots
/// and emitted records. Captured state is shared; output packet sizes and their
/// aggregate input-use budget are checked before allocating packet storage.
pub fn project(
    snapshot: &ExperimentSnapshot,
    policy: ProfileLimits,
    authoring: &Limits,
) -> Result<ExecutionProjection, ProjectionError> {
    let Setup::Scientific(setup) = snapshot.setup() else {
        return Err(ProjectionError::LegacySetup);
    };
    let selected = setup.declarations();
    selected.descriptor().validate(policy.selection)?;
    let integrator = setup
        .captures()
        .values()
        .find(|c| c.context.execution_contract == ExecutionContractId::Dynamics)
        .ok_or(ProjectionError::MissingIntegrator)?;
    let Payload::Integrators(model) = &selected.payloads()[&integrator.context.contribution] else {
        unreachable!("verified integrator")
    };
    let dynamics = type_id(provider(
        selected,
        &integrator.context.contribution,
        &model.scientific.dynamics,
    ));
    let fields: Vec<_> = setup
        .captures()
        .values()
        .filter(|c| c.context.execution_contract == ExecutionContractId::Field)
        .collect();
    if fields.len() > policy.fields
        || snapshot.object_count() > policy.objects
        || snapshot.object_count() > authoring.max_objects
    {
        return Err(ProjectionError::Limit);
    }
    let mut components = BTreeMap::new();
    for (reference, payload) in selected.payloads() {
        if let Payload::Components(c) = payload {
            let properties = [
                &c.scientific.bindings.inertial_mass,
                &c.scientific.bindings.source,
                &c.scientific.bindings.response,
            ]
            .into_iter()
            .flatten()
            .collect();
            components.insert(
                type_id(reference),
                Component {
                    schema: &c.scientific,
                    properties,
                },
            );
        }
    }
    let slots: Vec<Vec<_>> = fields
        .iter()
        .map(|field| {
            field
                .context
                .couplings
                .iter()
                .map(|slot| type_id(provider(selected, &field.context.contribution, &slot.slot)))
                .collect()
        })
        .collect();
    let mut coupled_counts = vec![0usize; fields.len()];
    for (id, object) in snapshot.objects() {
        for (kind, value) in &object.components {
            let unsupported = || ProjectionError::UnsupportedComponent {
                object: *id,
                component: kind.clone(),
            };
            let schema = components.get(kind).ok_or_else(unsupported)?;
            if (schema.schema.role == ComponentRole::Dynamics && kind != &dynamics)
                || value.schema_version.0 != schema.schema.version.get()
                || value.properties.keys().any(|p| {
                    !schema
                        .schema
                        .properties
                        .iter()
                        .any(|v| property_name(&v.id).as_ref() == Ok(p))
                })
                || schema.properties.iter().any(|p| {
                    !value
                        .properties
                        .contains_key(&property_name(p).expect("valid property"))
                })
            {
                return Err(unsupported());
            }
        }
        for (i, types) in slots.iter().enumerate() {
            coupled_counts[i] = coupled_counts[i]
                .checked_add(
                    types
                        .iter()
                        .filter(|t| object.components.contains_key(*t))
                        .count(),
                )
                .ok_or(ProjectionError::Limit)?;
        }
    }
    let bulk = BulkLimits {
        bytes: usize::try_from(policy.state_bytes).unwrap_or(usize::MAX),
        records: policy.objects,
    };
    let mut total = packet_bytes::<ObjectState>(snapshot.object_count(), bulk)? as u64;
    let mut records = 0usize;
    for (i, count) in coupled_counts.iter().enumerate() {
        records = records
            .checked_add(*count)
            .filter(|n| *n <= policy.couplings)
            .ok_or(ProjectionError::Limit)?;
        let size = packet_bytes::<CoupledEntity>(
            *count,
            BulkLimits {
                records: fields[i].context.bounds.projection_records as usize,
                ..bulk
            },
        )?;
        total = total
            .checked_add(size as u64)
            .ok_or(ProjectionError::Limit)?;
    }
    let domain = setup.domain().to_cbor(policy.domain)?;
    for capture in setup.captures().values() {
        for size in [
            domain.len(),
            capture.context.to_cbor()?.len(),
            capture.configuration.len(),
            capture.state.len(),
        ] {
            if size as u64 > policy.state_bytes {
                return Err(ProjectionError::Limit);
            }
            total = total
                .checked_add(size as u64)
                .ok_or(ProjectionError::Limit)?;
        }
    }
    if total > policy.input_bytes {
        return Err(ProjectionError::Limit);
    }
    let variables = crate::update::compile_document_variables(snapshot.state(), authoring)?;
    setup.validate_candidate(snapshot.state(), &variables, authoring)?;
    let scene_policy = SceneLimits {
        bytes: policy
            .scene
            .bytes
            .min(usize::try_from(policy.state_bytes).unwrap_or(usize::MAX))
            .min(usize::try_from(policy.input_bytes - total).unwrap_or(usize::MAX)),
        ..policy.scene
    };
    let scene_bytes = scene::build(snapshot, selected, &variables, authoring, scene_policy)?;
    total
        .checked_add(scene_bytes.len() as u64)
        .filter(|n| *n <= policy.input_bytes)
        .ok_or(ProjectionError::Limit)?;
    let scene_id = InputIdentity::of(
        SCENE_SCHEMA.parse().expect("static schema"),
        1,
        &scene_bytes,
    );
    let quantity = |id,
                    object: &crate::Object,
                    kind: &ComponentTypeId,
                    property: &LocalContributionId|
     -> Result<FiniteF64, ProjectionError> {
        let component = &object.components[kind];
        let key = property_name(property).expect("verified property");
        let schema = components[kind]
            .schema
            .properties
            .iter()
            .find(|p| &p.id == property)
            .expect("verified binding");
        let resolved = crate::validate::resolve_property(
            PropertyPath::new(id, kind.clone(), key.clone()),
            &PropertySchema::from_plugin(schema),
            &component.properties[&key].authored(),
            &variables,
            authoring,
        )?;
        Ok(FiniteF64::new(resolved.si_value().expect("quantity role"))
            .expect("validated finite quantity"))
    };
    let mass = components[&dynamics]
        .schema
        .bindings
        .inertial_mass
        .as_ref()
        .expect("Dynamics role");
    let mut objects = Vec::with_capacity(snapshot.object_count());
    for (id, object) in snapshot.objects() {
        let vector = |v: crate::Vector3| {
            v.to_array()
                .map(|n| FiniteF64::new(n).expect("validated kinematics"))
        };
        objects.push(ObjectState {
            id: EntityId(id.get()),
            kinematics: Kinematics {
                position_metres: vector(object.transform.translation),
                velocity_metres_per_second: vector(object.velocity.linear),
            },
            inertial_mass_kilograms: object
                .components
                .contains_key(&dynamics)
                .then(|| quantity(*id, object, &dynamics, mass))
                .transpose()?,
        });
    }
    let mut blobs = BTreeMap::new();
    let objects_id = packet(&objects, bulk, &mut blobs)?;
    let mut captured_fields = Vec::with_capacity(fields.len());
    for (i, field) in fields.iter().enumerate() {
        let mut coupled = Vec::with_capacity(coupled_counts[i]);
        for ((id, object), numeric) in snapshot.objects().iter().zip(&objects) {
            for (slot, kind) in slots[i].iter().enumerate() {
                if !object.components.contains_key(kind) {
                    continue;
                }
                let descriptor = &field.context.couplings[slot];
                coupled.push(CoupledEntity {
                    id: numeric.id,
                    slot: CouplingSlot(slot as u32),
                    kinematics: numeric.kinematics,
                    has_dynamics: numeric.inertial_mass_kilograms.is_some(),
                    source_si: descriptor
                        .source
                        .as_ref()
                        .map(|p| quantity(*id, object, kind, &p.property))
                        .transpose()?,
                    response_si: descriptor
                        .response
                        .as_ref()
                        .map(|p| quantity(*id, object, kind, &p.property))
                        .transpose()?,
                });
            }
        }
        captured_fields.push(CapturedField {
            kernel: capture(field, &mut blobs)?,
            coupled: packet(
                &coupled,
                BulkLimits {
                    records: policy.couplings,
                    ..bulk
                },
                &mut blobs,
            )?,
        });
    }
    let definition = ExecutionDefinition {
        api_version: EXECUTION_SCENE_SCHEMA.parse().expect("static schema"),
        profile: ExecutionProfile::ForceThenIntegrate,
        timestep_seconds: FiniteF64::new(setup.time_step().seconds())
            .expect("validated positive step"),
        objects: objects_id,
        scene: Some(scene_id.clone()),
        fields: captured_fields,
        dynamics: capture(integrator, &mut blobs)?,
    };
    blobs.insert(ArtifactDigest::sha256_of(&domain), domain.into());
    blobs.insert(scene_id.digest, scene_bytes.into());
    definition.validate(policy.fields)?;
    Ok(ExecutionProjection {
        source: snapshot.clone(),
        definition,
        blobs,
    })
}

fn packet<R: BulkRecord>(
    values: &[R],
    limits: BulkLimits,
    blobs: &mut BTreeMap<ArtifactDigest, Arc<[u8]>>,
) -> Result<InputIdentity, ProjectionError> {
    let mut bytes = Vec::new();
    encode_batch(values, &mut bytes, limits)?;
    let id = InputIdentity::of(
        R::SCHEMA.parse().expect("static schema"),
        values.len() as u64,
        &bytes,
    );
    blobs.entry(id.digest).or_insert_with(|| bytes.into());
    Ok(id)
}
fn capture(
    capture: &crate::scientific::KernelCapture,
    blobs: &mut BTreeMap<ArtifactDigest, Arc<[u8]>>,
) -> Result<CapturedKernel, ProjectionError> {
    let c = &capture.context;
    let bytes = c.to_cbor()?;
    let context = InputIdentity::of(INSTANCE_SCHEMA.parse().expect("static schema"), 1, &bytes);
    let state = InputIdentity::of(
        format!("{}/v{}", c.state_format.id, c.state_format.version)
            .parse()
            .expect("validated state schema"),
        capture.state_values,
        &capture.state,
    );
    blobs.entry(context.digest).or_insert_with(|| bytes.into());
    blobs
        .entry(state.digest)
        .or_insert_with(|| capture.state.clone());
    blobs
        .entry(c.configuration.digest)
        .or_insert_with(|| capture.configuration.clone());
    Ok(CapturedKernel {
        instance: c.instance.clone(),
        context,
        state,
    })
}
