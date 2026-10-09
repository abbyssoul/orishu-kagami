//! Prepare the window's two removal intents with their exact surviving graph.
//! This does not accept edits or alter the authority's root-equality invariant.
use std::sync::Arc;

use kagami_catalog::ComponentTypeId;
use kagami_document::{ExperimentCommand, ObjectId, dependencies::DependencyError};

use crate::document::Document;

pub(super) enum Removal {
    Object(ObjectId),
    Component(ObjectId, ComponentTypeId),
}

impl Removal {
    /// Cold, bounded scan of authored components followed by shared graph
    /// restriction. No inventory IO, provider resolution or guest execution.
    /// Existing roots/edges never change identity; a dependency shared by a
    /// surviving root remains selected. Invalid targets still reach the normal
    /// authority and are refused there without adopting this proposed lock.
    pub(super) fn commands(
        self,
        document: &Document,
    ) -> Result<Vec<ExperimentCommand>, DependencyError> {
        let mut replacement = None;
        if let Some(lock) = document.snapshot().dependencies() {
            lock.validate(document.limits().dependencies)?;
            let roots = &lock.selection().roots;
            let mut retained = vec![false; roots.len()];
            for (id, object) in document.snapshot().objects() {
                for component in object.components.keys() {
                    let removed = match &self {
                        Self::Object(target) => id == target,
                        Self::Component(target, kind) => id == target && component == kind,
                    };
                    if !removed && let Some(root) = component.contribution() {
                        let index = roots
                            .binary_search(root)
                            .map_err(|_| DependencyError::Roots)?;
                        retained[index] = true;
                    }
                }
            }
            if retained.contains(&false) {
                let surviving = roots
                    .iter()
                    .zip(retained)
                    .filter(|(_, keep)| *keep)
                    .map(|(root, _)| root.clone())
                    .collect::<Vec<_>>();
                replacement = Some(Arc::new(
                    lock.for_roots(&surviving, document.limits().dependencies)?,
                ));
            }
        }
        let command = match self {
            Self::Object(object) => ExperimentCommand::RemoveObject(object),
            Self::Component(object, component) => {
                ExperimentCommand::DetachComponent { object, component }
            }
        };
        let mut commands = vec![command];
        if let Some(lock) = replacement {
            commands.push(ExperimentCommand::AdoptDependencies(lock));
        }
        Ok(commands)
    }
}
