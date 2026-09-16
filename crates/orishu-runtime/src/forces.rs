//! Deterministic fixed-profile force reduction, before a plugin integrates motion.
//! No model equation or integration formula lives here. The run supervisor must
//! bind every input to its selected kernel and committed boundary before calling.

use orishu_plugin::{
    FiniteF64,
    execution::{Batch, CoupledEntity, DynamicEntity, EntityId, Force},
};
use orishu_workload::ComponentInstanceId;

/// One selected field's complete inputs and finished force output. Read the packet
/// with the shared bounded reader before constructing this view.
pub struct FieldForces<'a> {
    /// Exact selected field instance, not plugin display name or completion order.
    pub instance: &'a ComponentInstanceId,
    /// Number of entries in the admitted canonical coupling-slot descriptor table.
    pub coupling_slots: u32,
    /// Coupled projection supplied to that field from the committed boundary.
    pub coupled: Batch<'a, CoupledEntity>,
    /// Field's complete force candidate for all responding dynamic entities.
    pub forces: Batch<'a, Force>,
}

/// Aggregate reduction limits, distinct from each individual packet's byte limit.
#[derive(Clone, Copy, Debug)]
pub struct ReductionLimits {
    /// Maximum selected field instances.
    pub fields: usize,
    /// Maximum dynamic entities in the common input projection.
    pub entities: usize,
    /// Maximum aggregate coupled plus force records across all fields.
    pub field_records: usize,
}
impl Default for ReductionLimits {
    fn default() -> Self {
        Self {
            fields: 256,
            entities: 1_000_000,
            field_records: 4_000_000,
        }
    }
}

/// Bounded failure with no partial reduced-force result.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum ReductionError {
    /// Counts or allocation request exceed caller limits.
    #[error("force reduction limit exceeded")]
    LimitExceeded,
    /// Missing, duplicate, unexpected or noncanonical selected field identity.
    #[error("force reduction field coverage mismatch")]
    FieldCoverage,
    /// Coupled dynamic membership/kinematics differs from the common snapshot.
    #[error("force reduction entity projection mismatch")]
    EntityProjection,
    /// A projection names a coupling slot not declared by this selected model.
    #[error("force reduction undeclared coupling slot")]
    CouplingSlot,
    /// Missing, duplicate or unexpected force; zero response still requires a row.
    #[error("force reduction response coverage mismatch")]
    ResponseCoverage,
    /// A stable-order intermediate sum is non-finite.
    #[error("force reduction produced a non-finite sum")]
    NonFiniteSum,
}

/// Reusable reduction workspace. Capacity grows only when the admitted workload's
/// field/entity extents exceed prior capacity, never per entity or field sample.
/// Errors can change scratch, but never return a partially reduced batch.
#[derive(Default)]
pub struct ForceReducer {
    order: Vec<usize>,
    reduced: Vec<Force>,
}
impl ForceReducer {
    /// Validate exact selected-field and responding-entity coverage, then reduce
    /// by ascending field-instance identity regardless of completion order.
    /// `selected_fields` must be the complete canonical admitted field set.
    /// Complexity: O(F log F + C log E + E), where C is coupled records; storage
    /// O(F + E). No fields means zero net forces for all dynamic entities, never
    /// permission to omit a selected field's force result.
    pub fn reduce(
        &mut self,
        entities: &Batch<'_, DynamicEntity>,
        selected_fields: &[ComponentInstanceId],
        fields: &[FieldForces<'_>],
        limits: ReductionLimits,
    ) -> Result<&[Force], ReductionError> {
        if selected_fields.len() > limits.fields
            || fields.len() > limits.fields
            || entities.len() > limits.entities
        {
            return Err(ReductionError::LimitExceeded);
        }
        if fields.len() != selected_fields.len()
            || !selected_fields.windows(2).all(|ids| ids[0] < ids[1])
        {
            return Err(ReductionError::FieldCoverage);
        }
        let mut records = 0usize;
        for field in fields {
            records = records
                .checked_add(field.coupled.len())
                .and_then(|n| n.checked_add(field.forces.len()))
                .filter(|n| *n <= limits.field_records)
                .ok_or(ReductionError::LimitExceeded)?;
        }
        self.order
            .try_reserve(fields.len().saturating_sub(self.order.len()))
            .map_err(|_| ReductionError::LimitExceeded)?;
        self.order.clear();
        self.order.extend(0..fields.len());
        self.order
            .sort_unstable_by(|a, b| fields[*a].instance.cmp(fields[*b].instance));
        for (at, index) in self.order.iter().enumerate() {
            if fields[*index].instance != &selected_fields[at] {
                return Err(ReductionError::FieldCoverage);
            }
        }
        self.reduced
            .try_reserve(entities.len().saturating_sub(self.reduced.len()))
            .map_err(|_| ReductionError::LimitExceeded)?;
        self.reduced.clear();
        self.reduced.extend(entities.iter().map(|entity| Force {
            id: entity.id,
            newtons: [FiniteF64::ZERO; 3],
        }));
        for index in &self.order {
            let field = &fields[*index];
            let mut forces = field.forces.iter();
            let mut responded = None;
            for coupled in field.coupled.iter() {
                if coupled.slot.0 >= field.coupling_slots {
                    return Err(ReductionError::CouplingSlot);
                }
                let dynamic = find_entity(entities, coupled.id);
                if coupled.has_dynamics != dynamic.is_some() {
                    return Err(ReductionError::EntityProjection);
                }
                if let Some((_, entity)) = dynamic
                    && entity.kinematics != coupled.kinematics
                {
                    return Err(ReductionError::EntityProjection);
                }
                if coupled.responds_dynamically() && responded != Some(coupled.id) {
                    let force = forces
                        .next()
                        .filter(|f| f.id == coupled.id)
                        .ok_or(ReductionError::ResponseCoverage)?;
                    let (at, _) = dynamic.ok_or(ReductionError::EntityProjection)?;
                    responded = Some(coupled.id);
                    for axis in 0..3 {
                        self.reduced[at].newtons[axis] = FiniteF64::new(
                            self.reduced[at].newtons[axis].get() + force.newtons[axis].get(),
                        )
                        .map_err(|_| ReductionError::NonFiniteSum)?;
                    }
                }
            }
            if forces.next().is_some() {
                return Err(ReductionError::ResponseCoverage);
            }
        }
        Ok(&self.reduced)
    }
}

fn find_entity(
    entities: &Batch<'_, DynamicEntity>,
    id: EntityId,
) -> Option<(usize, DynamicEntity)> {
    let (mut low, mut high) = (0, entities.len());
    while low < high {
        let mid = low + (high - low) / 2;
        let entity = entities.get(mid)?;
        match entity.id.cmp(&id) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Some((mid, entity)),
        }
    }
    None
}
