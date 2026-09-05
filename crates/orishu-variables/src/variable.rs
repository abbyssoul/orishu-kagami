use crate::{expression::CompiledExpression, namespace::FQName};

/// Opaque handle to a variable registered in a [`crate::VariablesSystem`].
///
/// Cheap to copy and compare; does not carry the variable's name or
/// namespace, so a lookup through [`crate::VariablesSystem::lookup`] is needed
/// to go from a qualified name to a handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VariableId(pub(crate) u32);

/// Options supplied when defining a new variable.
#[derive(Clone, Debug, Default)]
pub struct VariableOptions {
    /// Initial comment, shown alongside the variable in UI listings.
    pub comment: String,
}

/// A named, namespaced variable bound to a compiled expression.
#[derive(Clone, Debug)]
pub struct Variable {
    pub name: FQName,
    pub comment: String,
    pub(crate) expression: CompiledExpression,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_options_default_comment_is_empty() {
        assert_eq!(VariableOptions::default().comment, "");
    }
}
