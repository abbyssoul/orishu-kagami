//! Declared bounds on everything a command batch can ask the model to do.
//!
//! MCP is a first-class caller of this model (ADR 0006), so a command batch is
//! untrusted input even when it originated from the user's own window. Every
//! bound is one declared value rather than a literal scattered through
//! validation, so an MCP-facing authority can tighten them and a test can
//! drive a limit failure without constructing a gigantic experiment.
//!
//! Structural ceilings that a caller must never be able to widen — the display
//! name length, the plugin identifier depth — live with their types instead.
//! One bound in two places is one bound that can disagree with itself.

/// Bounds applied while validating a candidate experiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest number of objects one experiment may hold.
    pub max_objects: usize,
    /// Largest number of components one object may carry.
    pub max_components_per_object: usize,
    /// Largest number of authored properties on one component.
    pub max_properties_per_component: usize,
    /// Largest number of commands accepted in one atomic batch.
    pub max_commands_per_batch: usize,
    /// Largest authored expression source, in bytes.
    pub max_expression_bytes: usize,
    /// Largest number of symbol references one expression may make. The
    /// per-expression half of the expression-work bound; the graph those
    /// symbols resolve into is bounded by the variables subsystem.
    pub max_expression_references: usize,
    /// Largest text length accepted for a text-valued property.
    pub max_text_bytes: usize,
    /// Largest number of variable definitions one experiment may hold.
    pub max_variables: usize,
    /// Largest description accepted for a variable definition, in bytes.
    pub max_description_bytes: usize,
    /// Largest number of enabled simulation plugins.
    pub max_enabled_plugins: usize,
    /// Largest number of retained undo entries.
    pub max_undo_depth: usize,
}

impl Limits {
    /// Bounds sized for an interactive Kagami installation.
    ///
    /// Comfortably above any experiment a person authors by hand while
    /// keeping the worst case one batch can cost bounded and small. The
    /// per-component and per-expression bounds match
    /// [`kagami_catalog::Limits`] on purpose: an object materialised from a
    /// template must not be refused by the authority that asked for it.
    ///
    /// `max_undo_depth` is 128, the depth Field CAD ran with in practice; an
    /// entry is a pointer plus a label, so the cost is the number of
    /// *distinct* experiments reachable through the stack rather than the
    /// depth itself.
    pub const DEFAULT: Self = Self {
        max_objects: 65_536,
        max_components_per_object: 64,
        max_properties_per_component: 64,
        max_commands_per_batch: 4_096,
        max_expression_bytes: 1_024,
        max_expression_references: 64,
        max_text_bytes: 4_096,
        max_variables: 4_096,
        max_description_bytes: 4_096,
        max_enabled_plugins: 64,
        max_undo_depth: 128,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_the_declared_constant() {
        assert_eq!(Limits::default(), Limits::DEFAULT);
    }

    #[test]
    fn default_limits_are_all_non_zero() {
        let limits = Limits::DEFAULT;
        assert!(limits.max_objects > 0);
        assert!(limits.max_components_per_object > 0);
        assert!(limits.max_properties_per_component > 0);
        assert!(limits.max_commands_per_batch > 0);
        assert!(limits.max_expression_bytes > 0);
        assert!(limits.max_expression_references > 0);
        assert!(limits.max_text_bytes > 0);
        assert!(limits.max_variables > 0);
        assert!(limits.max_description_bytes > 0);
        assert!(limits.max_enabled_plugins > 0);
        assert!(limits.max_undo_depth > 0);
    }

    #[test]
    fn expression_and_component_bounds_admit_anything_the_catalog_can_materialise() {
        // A template the catalog accepts must be instantiable here. If the
        // catalog ever widens one of these, this fails rather than surfacing
        // as an object that cannot be created from a valid template.
        let catalog = kagami_catalog::Limits::DEFAULT;
        let document = Limits::DEFAULT;
        assert!(document.max_components_per_object >= catalog.max_components_per_template);
        assert!(document.max_properties_per_component >= catalog.max_properties_per_component);
        assert!(document.max_expression_bytes >= catalog.max_expression_bytes);
        assert!(document.max_expression_references >= catalog.max_expression_references);
        assert!(document.max_text_bytes >= catalog.max_text_bytes);
    }
}
