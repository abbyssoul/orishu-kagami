//! Explicit v3 projections beside the unchanged v2 projections.
use super::*;
use crate::v3::{WorkloadManifest as Manifest, WorkloadSpec as Spec};

fn check_discriminator(manifest: &Manifest) -> Result<(), CanonicalError> {
    if manifest.api_version().as_str() != crate::v3::API_VERSION
        || manifest.kind().as_str() != crate::manifest::WORKLOAD_KIND
    {
        return Err(CanonicalError::NotItsOwnCanonicalForm);
    }
    Ok(())
}
pub(crate) fn canonical_bytes(
    manifest: &Manifest,
    limits: &Limits,
) -> Result<Vec<u8>, CanonicalError> {
    check_discriminator(manifest)?;
    raw_bounds(manifest, limits)?;
    let limits = &Limits {
        max_nesting_depth: limits.max_nesting_depth.min(64),
        ..*limits
    };
    let tree = manifest.to_canonical()?;
    check_collections(&tree, "", limits)?;
    encode(&tree, limits)
}

// Raw compiler-built models must not allocate a canonical tree just to learn
// that a collection or an unvalidated ScalarValue::Text is over budget.
pub(crate) fn raw_bounds(m: &Manifest, l: &Limits) -> Result<(), CanonicalError> {
    let count = |found: usize, limit: usize| {
        if found > limit {
            Err(CanonicalError::TooManyValues { limit })
        } else {
            Ok(())
        }
    };
    let scalars = |values: &std::collections::BTreeMap<crate::ParameterName, ScalarValue>,
                   limit: usize| {
        count(values.len(), limit)?;
        for v in values.values() {
            if let ScalarValue::Text(s) = v
                && s.len() > l.max_text_bytes
            {
                return Err(CanonicalError::TooLarge {
                    found: s.len() as u64,
                    limit: l.max_text_bytes as u64,
                });
            }
        }
        Ok(())
    };
    let c = &m.spec.compute;
    count(m.metadata.labels.len(), l.max_labels)?;
    count(c.components.len(), l.max_components)?;
    count(c.channels.len(), l.max_channels)?;
    count(c.step_plan.invocations.len(), l.max_step_invocations)?;
    count(c.placement_constraints.len(), l.max_placement_constraints)?;
    count(
        c.components
            .len()
            .checked_add(m.spec.artifacts.len())
            .and_then(|n| n.checked_add(2))
            .unwrap_or(usize::MAX),
        l.max_artifacts,
    )?;
    scalars(&m.spec.requirements.hardware, l.max_parameter_entries)?;
    scalars(
        &m.spec.requirements.execution_profile,
        l.max_parameter_entries,
    )?;
    for component in &c.components {
        count(component.roles.len(), l.max_roles_per_component)?;
        count(
            component.state_ownership.len(),
            l.max_state_ownership_per_component,
        )?;
        count(component.limits.len(), l.max_limit_entries)?;
        scalars(&component.config, l.max_config_entries)?;
    }
    for channel in &c.channels {
        count(channel.shape.len(), l.max_channel_shape_rank)?;
    }
    for invocation in &c.step_plan.invocations {
        count(invocation.inputs.len(), l.max_inputs_per_invocation)?;
        count(invocation.outputs.len(), l.max_outputs_per_invocation)?;
        count(
            invocation.depends_on.len(),
            l.max_dependencies_per_invocation,
        )?;
    }
    for placement in &c.placement_constraints {
        count(placement.instances.len(), l.max_components)?;
        scalars(&placement.parameters, l.max_parameter_entries)?;
    }
    Ok(())
}
pub(crate) fn from_canonical_bytes(
    bytes: &[u8],
    limits: &Limits,
) -> Result<Manifest, CanonicalError> {
    let limits = &Limits {
        max_nesting_depth: limits.max_nesting_depth.min(64),
        ..*limits
    };
    let tree = decode(bytes, limits)?;
    check_collections(&tree, "", limits)?;
    let manifest = Manifest::from_canonical(&tree, &Path::root())?;
    if canonical_bytes(&manifest, limits)? != bytes {
        return Err(CanonicalError::NotItsOwnCanonicalForm);
    }
    Ok(manifest)
}
// Bound typed collection construction after the byte/value-bounded CBOR tree.
// Counts apply to the same public graph limits as structural admission.
fn check_collections(value: &CanonicalValue, key: &str, l: &Limits) -> Result<(), CanonicalError> {
    let count = match key {
        "components" | "instances" => Some(l.max_components),
        "channels" => Some(l.max_channels),
        "shape" => Some(l.max_channel_shape_rank),
        "invocations" => Some(l.max_step_invocations),
        "inputs" => Some(l.max_inputs_per_invocation),
        "outputs" => Some(l.max_outputs_per_invocation),
        "dependsOn" => Some(l.max_dependencies_per_invocation),
        "stateOwnership" => Some(l.max_state_ownership_per_component),
        "roles" => Some(l.max_roles_per_component),
        "config" => Some(l.max_config_entries),
        "limits" => Some(l.max_limit_entries),
        "parameters" | "hardware" | "executionProfile" => Some(l.max_parameter_entries),
        "artifacts" => Some(l.max_artifacts),
        "labels" => Some(l.max_labels),
        "placementConstraints" => Some(l.max_placement_constraints),
        _ => None,
    };
    match value {
        CanonicalValue::Array(items) => {
            if count.is_some_and(|n| items.len() > n) {
                return Err(CanonicalError::TooManyValues {
                    limit: count.unwrap(),
                });
            }
            for item in items {
                check_collections(item, "", l)?;
            }
        }
        CanonicalValue::Map(map) => {
            if count.is_some_and(|n| map.entries().len() > n) {
                return Err(CanonicalError::TooManyValues {
                    limit: count.unwrap(),
                });
            }
            for (k, v) in map.entries() {
                if let CanonicalValue::Text(k) = k {
                    check_collections(v, k, l)?;
                }
            }
        }
        CanonicalValue::Text(v) if v.len() > l.max_text_bytes => {
            return Err(CanonicalError::TooLarge {
                found: v.len() as u64,
                limit: l.max_text_bytes as u64,
            });
        }
        _ => {}
    }
    Ok(())
}
impl FromCanonical for Spec {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut f = Fields::of(value, path)?;
        let result = Self {
            compute: f.required("compute")?,
            selection: f.required("selection")?,
            execution: f.required("execution")?,
            artifacts: f.list("artifacts")?,
            requirements: f.required("requirements")?,
        };
        f.finish()?;
        Ok(result)
    }
}
impl FromCanonical for Manifest {
    fn from_canonical(value: &CanonicalValue, path: &Path) -> Result<Self, CanonicalError> {
        let mut f = Fields::of(value, path)?;
        let result = Self::new(
            f.required("apiVersion")?,
            f.required("kind")?,
            f.required("metadata")?,
            f.required("spec")?,
        );
        f.finish()?;
        Ok(result)
    }
}
impl ToCanonical for Spec {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("compute", Some(self.compute.to_canonical()?)),
            ("selection", Some(self.selection.to_canonical()?)),
            ("execution", Some(self.execution.to_canonical()?)),
            ("artifacts", array_or_omitted(&self.artifacts)?),
            ("requirements", Some(self.requirements.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}
impl ToCanonical for Manifest {
    fn to_canonical(&self) -> Result<CanonicalValue, CanonicalError> {
        CanonicalMap::fields([
            ("apiVersion", some_text(self.api_version().as_str())),
            ("kind", some_text(self.kind().as_str())),
            ("metadata", Some(self.metadata.to_canonical()?)),
            ("spec", Some(self.spec.to_canonical()?)),
        ])
        .map(CanonicalValue::Map)
    }
}
