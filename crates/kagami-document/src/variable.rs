//! Named values an experiment defines for its own expressions to use.
//!
//! `mass_of_sun = 1.989e30 kg`, then a property authored as
//! `mass_of_sun / 2`. The point is not the arithmetic — that is
//! `orishu-variables`' — but that the *definition* is persisted experiment
//! intent (ADR 0005): what a save writes is the expression the author wrote,
//! and every magnitude anyone reads is derived from it.
//!
//! # An identity is not a name
//!
//! A definition has a [`VariableId`] minted by the model and an editable
//! [`Name`] the author chooses. Renaming is therefore presentation work over
//! an unchanged graph: nothing that referred to the definition by identity
//! rebinds, and the expressions that referred to it *by name* are rewritten as
//! part of the same validated edit (see [`mod@crate::update`]). Storing the name
//! as the handle would make a rename either a delete-and-recreate or a
//! silent breakage.
//!
//! # Namespaces
//!
//! A definition lives in a [`Namespace`], and its qualified name is what an
//! expression writes. The root namespace is the ordinary place for an
//! experiment's own variables, so an author writes `mass_of_sun` rather than
//! decorating every reference. Namespaces exist so plugin-exported constants
//! and catalog-qualified bindings can share one resolution rule with document
//! variables rather than needing a second one.

use orishu_variables::{Name, Namespace};

/// Stable, document-local identity of one variable definition.
///
/// Opaque and without a `Default`, for the reason [`crate::ObjectId`] is: an
/// identity in hand named a real definition at some revision, and a caller
/// that could conjure one could hand a command an identity nothing allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(u64);

impl VariableId {
    /// Rebuild an identity from its counter value.
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// The underlying counter value.
    ///
    /// For display and persistence only: an identity is meaningful within one
    /// experiment's history, and arithmetic on it is never meaningful.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for VariableId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "variable-{}", self.0)
    }
}

/// A variable definition, as the experiment persists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    /// Where the definition lives. Expressions reach it by qualified name.
    pub namespace: Namespace,
    /// The editable name. Not the identity.
    pub name: Name,
    /// Expression source, retained verbatim. This is the intent.
    pub expression: String,
    /// What the author says it is for. Never part of resolution.
    pub description: Option<String>,
}

impl Variable {
    /// The dotted name an expression writes to reach this definition.
    pub fn qualified_name(&self) -> String {
        self.namespace.qualified(&self.name).to_string()
    }
}

/// Everything needed to define a variable, in one value.
///
/// A builder rather than a wide constructor, for the reason
/// [`crate::ObjectSpec`] is one: a definition cannot be assembled
/// half-specified, and adding an authored field later breaks no call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableSpec {
    /// Where the definition lives. Defaults to the root namespace, which is
    /// where an experiment's own variables belong.
    pub namespace: Namespace,
    /// The editable name.
    pub name: Name,
    /// Expression source, exactly as authored.
    pub expression: String,
    /// What the author says it is for.
    pub description: Option<String>,
}

impl VariableSpec {
    /// A root-namespace definition bound to `expression`.
    pub fn new(name: Name, expression: impl Into<String>) -> Self {
        Self {
            namespace: Namespace::new(""),
            name,
            expression: expression.into(),
            description: None,
        }
    }

    /// Put the definition in `namespace`.
    #[must_use]
    pub fn in_namespace(mut self, namespace: Namespace) -> Self {
        self.namespace = namespace;
        self
    }

    /// Describe what it is for.
    #[must_use]
    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// The dotted name an expression would write to reach this definition.
    pub fn qualified_name(&self) -> String {
        self.namespace.qualified(&self.name).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> Name {
        Name::new(value).expect("valid identifier")
    }

    #[test]
    fn a_root_definition_is_reached_by_its_bare_name() {
        let spec = VariableSpec::new(name("mass_of_sun"), "1.989e30 kg");
        assert_eq!(spec.qualified_name(), "mass_of_sun");
        assert_eq!(spec.description, None);
    }

    #[test]
    fn a_namespaced_definition_is_reached_by_its_dotted_name() {
        let spec = VariableSpec::new(name("K"), "8.99e9")
            .in_namespace(Namespace::new("electricity"))
            .described("Coulomb constant");
        assert_eq!(spec.qualified_name(), "electricity.K");
        assert_eq!(spec.description.as_deref(), Some("Coulomb constant"));
    }

    #[test]
    fn identities_display_readably_and_are_not_names() {
        let id = VariableId::from_raw(3);
        assert_eq!(id.to_string(), "variable-3");
        assert_eq!(id.get(), 3);
    }
}
