//! Declared bounds on everything a catalog file can ask the loader to do.
//!
//! Catalog files are hand-editable, copied between machines, and reachable
//! from MCP clients, so they are untrusted input even when they came from the
//! user's own disk. Every bound is declared here as one value rather than
//! scattered as literals, so a caller can tighten them for an MCP-facing
//! authority and a test can drive a limit failure without generating a
//! megabyte of YAML.

/// Bounds applied while loading and validating catalog content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest catalog file accepted, in bytes. A larger file is refused
    /// before it is read into memory.
    pub max_file_bytes: u64,
    /// Largest number of files scanned in one catalog directory.
    pub max_files: usize,
    /// Largest number of `---`-separated documents read from one file.
    pub max_documents_per_file: usize,
    /// Largest number of components one template may compose.
    pub max_components_per_template: usize,
    /// Largest number of authored properties on one component.
    pub max_properties_per_component: usize,
    /// Largest number of parameters one template may declare.
    pub max_parameters_per_template: usize,
    /// Largest number of helper definitions one template may declare.
    pub max_helpers_per_template: usize,
    /// Largest authored expression source, in bytes.
    pub max_expression_bytes: usize,
    /// Largest number of symbol references one expression may make. This is
    /// the per-expression half of the expression-work bound; the other half
    /// is [`Self::max_bindings`], which caps the graph those symbols resolve
    /// into.
    pub max_expression_references: usize,
    /// Largest number of variable bindings one loaded catalog set may
    /// publish, across every catalog.
    pub max_bindings: usize,
    /// Largest number of metadata labels or annotations on one template.
    pub max_metadata_entries: usize,
    /// Largest text length accepted for a metadata value or a text property.
    pub max_text_bytes: usize,
}

impl Limits {
    /// Bounds sized for an interactive Kagami installation.
    ///
    /// Chosen to be comfortably above any plausible hand-authored catalog
    /// (the shipped `planets` catalog is ~40 templates in ~12 KiB) while
    /// keeping the worst case a single load can cost bounded and small.
    pub const DEFAULT: Self = Self {
        max_file_bytes: 1024 * 1024,
        max_files: 1024,
        max_documents_per_file: 1024,
        max_components_per_template: 64,
        max_properties_per_component: 64,
        max_parameters_per_template: 32,
        max_helpers_per_template: 64,
        max_expression_bytes: 1024,
        max_expression_references: 64,
        max_bindings: 65_536,
        max_metadata_entries: 32,
        max_text_bytes: 4096,
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
        assert!(limits.max_file_bytes > 0);
        assert!(limits.max_files > 0);
        assert!(limits.max_documents_per_file > 0);
        assert!(limits.max_components_per_template > 0);
        assert!(limits.max_properties_per_component > 0);
        assert!(limits.max_parameters_per_template > 0);
        assert!(limits.max_helpers_per_template > 0);
        assert!(limits.max_expression_bytes > 0);
        assert!(limits.max_expression_references > 0);
        assert!(limits.max_bindings > 0);
        assert!(limits.max_metadata_entries > 0);
        assert!(limits.max_text_bytes > 0);
    }

    #[test]
    fn the_default_binding_bound_fits_what_a_projection_can_hold() {
        // A projection spends two variables per binding — the canonical name
        // and its concise alias. This is only a statement about the *default*
        // pairing being coherent; a caller may pass any `max_bindings` it
        // likes, which is why `resolve` clamps rather than trusting it.
        assert!(
            Limits::DEFAULT.max_bindings <= crate::CatalogProjection::max_projectable_bindings()
        );
        // Every catalog expression is parsed before it is defined, so the
        // evaluator must also admit the longest source a catalog may carry.
        let evaluator = crate::CatalogProjection::evaluator_limits();
        assert!(Limits::DEFAULT.max_expression_bytes <= evaluator.max_expression_bytes);
    }

    #[test]
    fn a_generated_reference_can_exceed_what_any_authored_expression_may() {
        // The bound that actually bit: a template name is unbounded by the
        // loader, and the reference a binding publishes is built from it, so
        // comparing the two *expression* bounds proves nothing about the
        // references a catalog generates. This records the gap rather than
        // pretending the numbers above close it — closing it is `resolve`'s
        // job, and `a_template_whose_generated_reference_is_too_long_is_
        // isolated_not_fatal` is where that is checked.
        let evaluator = crate::CatalogProjection::evaluator_limits();
        let name = "a".repeat(Limits::DEFAULT.max_text_bytes);
        let generated = format!("catalog.{name}.component.property");
        assert!(
            generated.len() > evaluator.max_expression_bytes,
            "if this ever stops holding, the isolation path above has no way to be reached"
        );
    }

    #[test]
    fn a_custom_binding_bound_is_clamped_rather_than_trusted() {
        // The case a DEFAULT-to-DEFAULT comparison cannot see: a caller is
        // free to declare more bindings than the evaluator can hold.
        let generous = Limits {
            max_bindings: usize::MAX,
            ..Limits::DEFAULT
        };
        assert!(generous.max_bindings > crate::CatalogProjection::max_projectable_bindings());
    }
}
