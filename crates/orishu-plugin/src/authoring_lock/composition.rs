//! Explicit cold authoring operations, never implicit provider resolution.
use super::*;
use crate::resolution::{ProviderBinding, RequirementKey};
use std::collections::{BTreeMap, BTreeSet};

/// Two closed graphs disagree about an exact consumer's dependency slot. A
/// missing side means that graph omitted the edge, not permission to fill it in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingConflict {
    /// Exact consumer and slot requiring an explicit authoring decision.
    pub requirement: RequirementKey,
    /// Choice in the receiver, if present.
    pub existing: Option<ContributionRef>,
    /// Choice in the incoming graph, if present.
    pub incoming: Option<ContributionRef>,
}

/// Composition never replaces a pin or silently repairs an incomplete closure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompositionError {
    /// A graph or the resulting union exceeds the receiving policy.
    #[error("invalid or over-budget dependency composition: {0}")]
    Invalid(#[from] Error),
    /// Both graphs describe this consumer, but not identically.
    #[error("dependency choices conflict")]
    Conflict(Box<BindingConflict>),
}

impl SelectionLock {
    /// Copy only the transitive closure of explicit sorted, unique member roots.
    /// Suitable for template extraction and explicit removal/pruning edits. No
    /// missing root is inferred and no provider is selected from an inventory.
    /// Cold O((members + edges) log members) work, O(members + edges) memory.
    pub fn for_roots(&self, roots: &[ContributionRef], limits: LockLimits) -> Result<Self, Error> {
        self.validate(limits)?;
        if roots.len() > limits.items {
            return Err(limit("roots"));
        }
        if !sorted(roots) {
            return Err(invalid("roots", "roots must be sorted and unique"));
        }
        let members = &self.selection.contributions;
        let mut seen = vec![false; members.len()];
        let mut ready = Vec::new();
        for root in roots {
            let i = members
                .binary_search(root)
                .map_err(|_| invalid("roots", "root is outside the locked closure"))?;
            seen[i] = true;
            ready.push(i);
        }
        let bindings = &self.selection.bindings;
        let mut cursor = 0;
        while cursor < ready.len() {
            let member = &members[ready[cursor]];
            cursor += 1;
            let start = bindings.partition_point(|b| b.requirement.consumer < *member);
            for b in bindings[start..]
                .iter()
                .take_while(|b| b.requirement.consumer == *member)
            {
                let i = members
                    .binary_search(&b.provider)
                    .expect("validated endpoint");
                if !seen[i] {
                    seen[i] = true;
                    ready.push(i);
                }
            }
        }
        Self::new(
            Selection {
                roots: roots.to_vec(),
                contributions: members
                    .iter()
                    .zip(&seen)
                    .filter(|(_, yes)| **yes)
                    .map(|(c, _)| c.clone())
                    .collect(),
                bindings: bindings
                    .iter()
                    .filter(|b| {
                        seen[members
                            .binary_search(&b.requirement.consumer)
                            .expect("validated endpoint")]
                    })
                    .cloned()
                    .collect(),
            },
            limits,
        )
    }

    /// Union complete graphs without replacement. Common consumers must have
    /// identical outgoing bindings, including absence. Both inputs and the union
    /// obey the receiver's budgets. Cold O((members + edges) log(members + edges))
    /// work and O(members + edges) scratch; each input is independently bounded.
    pub fn merge(&self, incoming: &Self, limits: LockLimits) -> Result<Self, CompositionError> {
        self.validate(limits)?;
        incoming.validate(limits)?;
        let a = &self.selection;
        let b = &incoming.selection;
        for (own, other, reverse) in [(a, b, false), (b, a, true)] {
            for edge in &own.bindings {
                if other
                    .contributions
                    .binary_search(&edge.requirement.consumer)
                    .is_err()
                {
                    continue;
                }
                let other_provider = other
                    .bindings
                    .binary_search_by(|e| e.requirement.cmp(&edge.requirement))
                    .ok()
                    .map(|i| &other.bindings[i].provider);
                if other_provider != Some(&edge.provider) {
                    let (existing, incoming) = if reverse {
                        (other_provider.cloned(), Some(edge.provider.clone()))
                    } else {
                        (Some(edge.provider.clone()), other_provider.cloned())
                    };
                    return Err(CompositionError::Conflict(Box::new(BindingConflict {
                        requirement: edge.requirement.clone(),
                        existing,
                        incoming,
                    })));
                }
            }
        }
        fn union(
            items: impl Iterator<Item = ContributionRef>,
            max: usize,
        ) -> Result<Vec<ContributionRef>, Error> {
            let mut result = BTreeSet::new();
            for item in items {
                if result.len() == max && !result.contains(&item) {
                    return Err(limit("entries"));
                }
                result.insert(item);
            }
            Ok(result.into_iter().collect())
        }
        let roots = union(a.roots.iter().chain(&b.roots).cloned(), limits.items)?;
        let contributions = union(
            a.contributions.iter().chain(&b.contributions).cloned(),
            limits.items,
        )?;
        let mut edges = BTreeMap::new();
        for edge in a.bindings.iter().chain(&b.bindings) {
            if edges.len() == limits.items && !edges.contains_key(&edge.requirement) {
                return Err(limit("entries").into());
            }
            edges.insert(edge.requirement.clone(), edge.provider.clone());
        }
        Ok(Self::new(
            Selection {
                roots,
                contributions,
                bindings: edges
                    .into_iter()
                    .map(|(requirement, provider)| ProviderBinding {
                        requirement,
                        provider,
                    })
                    .collect(),
            },
            limits,
        )?)
    }
}
