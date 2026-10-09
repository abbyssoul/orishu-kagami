//! Standalone component provider intent owned by the experiment, not the UI.
use crate::{Limits, model::ExperimentState};
use orishu_plugin::{authoring_lock::SelectionLock, resolution::ProviderBinding};

/// Structural authoring refusal; installation/enablement are separate capability
/// questions, so offline restoration does not require a plugin inventory.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DependencyError {
    /// Invalid or over-budget closed graph.
    #[error("invalid dependency lock: {0}")]
    Lock(#[from] orishu_plugin::Error),
    /// Combining authored graphs requires agreement, not silent replacement.
    #[error("dependency composition refused: {0}")]
    Composition(#[from] orishu_plugin::authoring_lock::CompositionError),
    /// A standalone lock must describe exactly the currently authored exact
    /// component roots. Legacy logical references remain unresolved, not guessed.
    #[error("dependency lock roots differ from the experiment's exact components")]
    Roots,
    /// Captured execution evidence must use the same component provider intent.
    #[error("dependency lock disagrees with captured scientific selection")]
    ScientificSelection,
}
impl DependencyError {
    /// Stable domain reason for UI/MCP adapters.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Lock(_) => "invalid_dependency_lock",
            Self::Composition(_) => "dependency_composition_refused",
            Self::Roots => "dependency_lock_roots_mismatch",
            Self::ScientificSelection => "dependency_lock_scientific_mismatch",
        }
    }
}

pub(crate) fn validate(state: &ExperimentState, limits: &Limits) -> Result<(), DependencyError> {
    validate_against(state, limits, None)
}

/// Reset preparation supplies new verified evidence; ordinary validation always
/// reconciles against the actual captured setup. Root/lock checks are identical.
pub(crate) fn validate_against(
    state: &ExperimentState,
    limits: &Limits,
    replacement: Option<&orishu_plugin::selected::SelectionDescriptor>,
) -> Result<(), DependencyError> {
    let Some(lock) = &state.dependencies else {
        return Ok(());
    };
    // Locks can cross between authorities with different budgets. Verify the
    // receiving policy, including encoded bytes, not just the original handle.
    lock.validate(limits.dependencies)?;
    let mut seen = vec![false; lock.selection().roots.len()];
    for root in state
        .objects
        .values()
        .flat_map(|o| o.components.keys().filter_map(|c| c.contribution()))
    {
        let i = lock
            .selection()
            .roots
            .binary_search(root)
            .map_err(|_| DependencyError::Roots)?;
        seen[i] = true;
    }
    if seen.contains(&false) {
        return Err(DependencyError::Roots);
    }
    if let Some(selection) = replacement.or_else(|| {
        state
            .setup
            .scientific()
            .map(|s| s.declarations().descriptor())
    }) {
        reconcile(lock, selection)?;
    }
    Ok(())
}

pub(crate) fn reconcile(
    lock: &SelectionLock,
    scientific: &orishu_plugin::selected::SelectionDescriptor,
) -> Result<(), DependencyError> {
    if lock
        .selection()
        .contributions
        .iter()
        .any(|c| scientific.contributions.binary_search(c).is_err())
    {
        return Err(DependencyError::ScientificSelection);
    }
    // Checking only supplied edges would accept a forged lock that omits a
    // dependency the captured declarations prove is required. Every dependency
    // of every retained member must also appear in the standalone closure.
    let required = scientific
        .bindings
        .iter()
        .filter(|b| {
            lock.selection()
                .contributions
                .binary_search(&b.consumer)
                .is_ok()
        })
        .count();
    if required != lock.selection().bindings.len() {
        return Err(DependencyError::ScientificSelection);
    }
    for ProviderBinding {
        requirement,
        provider,
    } in &lock.selection().bindings
    {
        let matching = scientific
            .bindings
            .binary_search_by(|b| {
                b.consumer
                    .cmp(&requirement.consumer)
                    .then(b.requirement_slot.cmp(&requirement.slot))
            })
            .ok()
            .map(|i| &scientific.bindings[i]);
        if matching.is_none_or(|b| b.provider != *provider) {
            return Err(DependencyError::ScientificSelection);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_plugin::{
        ContributionRef,
        resolution::{RequirementKey, Selection},
        selected::{SelectedBinding, SelectionDescriptor},
    };
    fn reference(id: &str) -> ContributionRef {
        ContributionRef {
            release: format!("sha256:{}", "01".repeat(32)).parse().unwrap(),
            extension_point: "orishu.model.components/v1".parse().unwrap(),
            local_id: id.parse().unwrap(),
        }
    }
    #[test]
    fn captured_evidence_requires_all_and_only_component_dependency_edges() {
        let root = reference("a");
        let provider = reference("b");
        let binding = ProviderBinding {
            requirement: RequirementKey {
                consumer: root.clone(),
                slot: "dependency".parse().unwrap(),
            },
            provider: provider.clone(),
        };
        let lock = SelectionLock::new(
            Selection {
                roots: vec![root.clone()],
                contributions: vec![root.clone(), provider.clone()],
                bindings: vec![binding.clone()],
            },
            Default::default(),
        )
        .unwrap();
        let selected = SelectedBinding {
            consumer: root.clone(),
            requirement_slot: binding.requirement.slot.clone(),
            provider: provider.clone(),
            exact_contract: orishu_plugin::ScientificContractRef {
                name: "org.example.component".parse().unwrap(),
                version: 1.try_into().unwrap(),
                digest: root.release.to_string().parse().unwrap(),
            },
        };
        let mut evidence = SelectionDescriptor {
            api_version: orishu_plugin::selected::SELECTION_SCHEMA.parse().unwrap(),
            roots: vec![root.clone()],
            contributions: vec![root.clone(), provider],
            bindings: vec![selected],
            kernel_instances: vec![],
            release_evidence: vec![],
        };
        reconcile(&lock, &evidence).unwrap();
        let omitted = SelectionLock::new(
            Selection {
                roots: vec![root.clone()],
                contributions: vec![root],
                bindings: vec![],
            },
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            reconcile(&omitted, &evidence),
            Err(DependencyError::ScientificSelection)
        );
        evidence.bindings[0].provider = reference("c");
        assert_eq!(
            reconcile(&lock, &evidence),
            Err(DependencyError::ScientificSelection)
        );
        evidence.bindings.clear();
        assert_eq!(
            reconcile(&lock, &evidence),
            Err(DependencyError::ScientificSelection)
        );
    }
}
