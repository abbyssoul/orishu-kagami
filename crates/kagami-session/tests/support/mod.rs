//! Two adapter-shaped callers over one authority.
//!
//! The parity guarantee (ADR 0006) is that the UI and MCP reach the *same*
//! session with the same rules. These two helpers exist so a test can assert
//! that as a property rather than assume it: they differ only in how an
//! adapter would naturally behave — the UI submits against the revision it
//! just rendered, an MCP client mints its own correlation identities — and
//! neither has a privileged path.
#![allow(
    dead_code,
    reason = "every test binary compiles its own copy of this module and uses a different part"
)]

use kagami_catalog::{
    ComponentName, ComponentSchema, ComponentTypeId, Dimension, PluginId, PropertyKind,
    PropertyName, PropertySchema, SchemaRegistry, SchemaVersion,
};
use kagami_document::{AuthoredValue, DisplayName, ExperimentCommand, ObjectSpec};
use kagami_session::{
    Acceptance, ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    SessionRejection,
};

/// The component type a mass plugin contributes.
pub fn mass_component() -> ComponentTypeId {
    ComponentTypeId::new(
        PluginId::new("kagami.mass_sources").expect("valid identifier"),
        ComponentName::new("inertial_mass").expect("valid identifier"),
    )
}

/// An installation with one simulation plugin installed.
pub fn schemas() -> SchemaRegistry {
    SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(1)).with_property(
            PropertyName::new("mass").expect("valid identifier"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::MASS,
            }),
        ),
    )
}

/// The same installation after the plugin was replaced by a release that
/// declares the same property in a different dimension.
///
/// Not a version bump: values accepted by [`schemas`] are values this
/// declaration refuses, which is what separates "the plugin is missing" from
/// "the plugin is here and disagrees".
pub fn reshaped_schemas() -> SchemaRegistry {
    SchemaRegistry::new().with(
        ComponentSchema::new(mass_component(), SchemaVersion(2)).with_property(
            PropertyName::new("mass").expect("valid identifier"),
            PropertySchema::required(PropertyKind::Quantity {
                dimension: Dimension::LENGTH,
            }),
        ),
    )
}

/// The property values [`schemas`]'s mass component requires.
pub fn mass_properties() -> std::collections::BTreeMap<PropertyName, AuthoredValue> {
    let mut properties = std::collections::BTreeMap::new();
    properties.insert(
        PropertyName::new("mass").expect("valid identifier"),
        AuthoredValue::si("5.972e24"),
    );
    properties
}

/// A label, or a panic if the test wrote an invalid one.
pub fn name(value: &str) -> DisplayName {
    DisplayName::new(value).expect("valid label")
}

/// The command that adds a bare object.
pub fn create(label: &str) -> ExperimentCommand {
    ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(name(label))))
}

/// One adapter over a shared authority.
///
/// Owns nothing but its own name and correlation counter: the experiment, the
/// revision, and the history all live in the authority, which is the point.
pub struct Adapter {
    actor: ActorId,
    next: u64,
    /// `true` to submit guarded by the revision last read, the way a UI that
    /// renders a view and acts on it would.
    guard: bool,
}

impl Adapter {
    /// An adapter that guards every submission with the current revision.
    pub fn guarded(actor: &str) -> Self {
        Self {
            actor: ActorId::new(actor).expect("valid actor"),
            next: 0,
            guard: true,
        }
    }

    /// An adapter that applies to whatever is current.
    pub fn unguarded(actor: &str) -> Self {
        Self {
            actor: ActorId::new(actor).expect("valid actor"),
            next: 0,
            guard: false,
        }
    }

    /// Mint the next correlation identity for this adapter.
    pub fn command_id(&mut self) -> CommandId {
        let id = CommandId::new(format!("{}-{}", self.actor, self.next)).expect("valid identity");
        self.next += 1;
        id
    }

    /// Build an envelope this adapter would send.
    pub fn envelope(
        &mut self,
        authority: &DocumentAuthority,
        command: SessionCommand,
    ) -> ExperimentCommandEnvelope {
        let envelope =
            ExperimentCommandEnvelope::new(self.command_id(), self.actor.clone(), command);
        if self.guard {
            envelope.guarded_by(authority.revision())
        } else {
            envelope
        }
    }

    /// Submit a command, returning the authority's decision.
    pub fn submit(
        &mut self,
        authority: &mut DocumentAuthority,
        command: SessionCommand,
    ) -> Result<Acceptance, SessionRejection> {
        let envelope = self.envelope(authority, command);
        authority.submit(envelope)
    }

    /// Submit an edit that is expected to be accepted.
    pub fn edit(
        &mut self,
        authority: &mut DocumentAuthority,
        commands: Vec<ExperimentCommand>,
    ) -> Acceptance {
        self.submit(authority, SessionCommand::Edit(commands))
            .expect("edit should be accepted")
    }
}
