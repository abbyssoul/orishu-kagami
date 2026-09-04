use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, PoisonError};

use thiserror::Error;

use crate::expression::{CompiledExpression, ExprEvalError, ExprParsingError, eval_ast};
use crate::namespace::{FQName, IntoName, InvalidName, Name, Namespace};
use crate::variable::{Variable, VariableId, VariableOptions};

/// Everything that can go wrong when defining, redefining, or evaluating
/// variables through a [`VariablesSystem`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VariablesError {
    /// The expression source text does not parse.
    #[error(transparent)]
    Parsing(#[from] ExprParsingError),
    /// The expression parsed, but evaluating it failed.
    #[error(transparent)]
    Eval(#[from] ExprEvalError),
    /// The candidate variable name is not a valid [`Name`].
    #[error(transparent)]
    InvalidName(#[from] InvalidName),
    /// A variable with this name already exists in this namespace.
    #[error("variable `{name}` already defined in namespace `{namespace}`")]
    DuplicateName { namespace: Namespace, name: Name },
    /// The `VariableId` does not refer to a variable in this system.
    #[error("unknown variable handle")]
    UnknownHandle,
}

/// Subsystem that owns a set of namespaced variables and computes their
/// values, resolving references between them on demand (idCVarSystem-style,
/// but with no notion of dimensions or documents).
#[derive(Default)]
pub struct VariablesSystem {
    variables: Vec<Variable>,
    /// Keyed by each variable's canonical dotted name (`FQName`'s `Display`
    /// form), not `FQName` itself, so [`Self::resolve_symbol`] — the hot
    /// path walked once per symbol reference on every eval — can look up
    /// the raw, already-dotted symbol text directly with no allocation.
    /// `FQName::parse`'s split-at-last-dot and `Display`'s join are exact
    /// inverses (a `Name` can never contain `.`, so the last dot in the
    /// combined string is always the separator), so this has identical key
    /// identity to a `HashMap<FQName, _>`.
    index: HashMap<String, VariableId>,
    /// Scratch cycle-detection buffer for [`Self::value`]/[`Self::eval`],
    /// checked out for the duration of one top-level call and returned
    /// (cleared) afterward, so its capacity is amortized across calls
    /// instead of reallocating a fresh `Vec` every time. A `Mutex` (not
    /// `RefCell`) to keep this type `Sync`-safe, matching `SampleCache`'s
    /// interior-mutability convention elsewhere in the workspace.
    visiting_pool: Mutex<Vec<VariableId>>,
}

impl VariablesSystem {
    /// Define a new variable in `namespace` named `name`, bound to
    /// `expression`. Fails if `name` is not a valid [`Name`], if the name is
    /// already taken in that namespace, or if `expr_source` does not parse.
    pub fn define<T>(
        &mut self,
        namespace: &Namespace,
        name: T,
        expression: CompiledExpression,
        options: VariableOptions,
    ) -> Result<VariableId, VariablesError>
    where
        T: IntoName,
    {
        let name = name.into_name()?;
        let key = namespace.qualified(&name);
        let key_text = key.to_string();
        if self.index.contains_key(&key_text) {
            return Err(VariablesError::DuplicateName {
                namespace: namespace.clone(),
                name,
            });
        }

        let id = VariableId(self.variables.len() as u32);
        self.variables.push(Variable {
            name: key,
            comment: options.comment,
            expression,
        });
        self.index.insert(key_text, id);
        Ok(id)
    }

    /// Replace `id`'s expression, leaving its comment untouched. Fails if
    /// `id` is unknown or `expr_source` does not parse.
    pub fn set(
        &mut self,
        id: VariableId,
        expression: CompiledExpression,
    ) -> Result<(), VariablesError> {
        let variable = self
            .variables
            .get_mut(id.0 as usize)
            .ok_or(VariablesError::UnknownHandle)?;
        variable.expression = expression;
        Ok(())
    }

    /// Compute `id`'s current value, recursively resolving any variables it
    /// depends on. Recomputed on every call (no caching).
    pub fn value(&self, id: VariableId) -> Result<f64, VariablesError> {
        if self.variables.get(id.0 as usize).is_none() {
            return Err(VariablesError::UnknownHandle);
        }
        let mut visiting = self.take_visiting();
        let result = self.value_with_visiting(id, &mut visiting);
        self.return_visiting(visiting);
        result.map_err(VariablesError::from)
    }

    /// Checks out the shared cycle-detection scratch buffer, leaving it
    /// empty behind. The lock is held only for this swap, never across the
    /// recursive evaluation that follows.
    fn take_visiting(&self) -> Vec<VariableId> {
        let mut buffer = self
            .visiting_pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *buffer)
    }

    /// Clears and returns a buffer previously obtained from
    /// [`Self::take_visiting`], so its capacity is reused by the next call.
    fn return_visiting(&self, mut buffer: Vec<VariableId>) {
        buffer.clear();
        *self
            .visiting_pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = buffer;
    }

    fn value_with_visiting(
        &self,
        id: VariableId,
        visiting: &mut Vec<VariableId>,
    ) -> Result<f64, ExprEvalError> {
        if visiting.contains(&id) {
            return Err(ExprEvalError::Cycle);
        }
        visiting.push(id);
        let ast = &self.variables[id.0 as usize].expression.ast;
        let result = eval_ast(ast, &mut |raw: &str| {
            let target = self.resolve_symbol(raw)?;
            self.value_with_visiting(target, visiting)
        });
        visiting.pop();
        result
    }

    /// Parse and evaluate an ad-hoc expression against this system's
    /// currently defined variables, without registering it as a variable.
    pub fn eval(&self, expr_source: &str) -> Result<f64, VariablesError> {
        let compiled = CompiledExpression::parse(expr_source)?;
        let mut visiting = self.take_visiting();
        let value = eval_ast(&compiled.ast, &mut |raw: &str| {
            let target = self.resolve_symbol(raw)?;
            self.value_with_visiting(target, &mut visiting)
        });
        self.return_visiting(visiting);
        value.map_err(VariablesError::from)
    }

    /// Resolve a raw dotted symbol (as captured verbatim by [`Expr::Symbol`]
    /// at parse time) to its handle, with no allocation on the success
    /// path — `raw` is already in the index's canonical form, so this is a
    /// direct lookup rather than a round trip through [`FQName::parse`].
    pub fn resolve_symbol(&self, raw: &str) -> Result<VariableId, ExprEvalError> {
        self.index
            .get(raw)
            .copied()
            .ok_or_else(|| ExprEvalError::UnknownVariable(raw.to_owned()))
    }

    /// Resolve an already-split namespace and name to its handle. Builds a
    /// transient [`FQName`] to query the index; prefer [`Self::resolve_symbol`]
    /// when the raw dotted text is already at hand, as it queries the index
    /// directly with no allocation.
    pub fn lookup_in(&self, namespace: &Namespace, name: &Name) -> Option<VariableId> {
        self.lookup(&namespace.qualified(name))
    }

    /// Resolve a fully qualified variable name to its handle.
    pub fn lookup(&self, name: &FQName) -> Option<VariableId> {
        self.index.get(&name.to_string()).copied()
    }

    /// Read `id`'s fully qualified name, or `None` if `id` is unknown.
    pub fn qualified_name(&self, id: VariableId) -> Option<&FQName> {
        self.variables
            .get(id.0 as usize)
            .map(|variable| &variable.name)
    }

    /// Read `id`'s name (without its namespace), or `None` if `id` is unknown.
    pub fn name(&self, id: VariableId) -> Option<&Name> {
        self.variables
            .get(id.0 as usize)
            .map(|variable| variable.name.name())
    }

    /// Read `id`'s comment, or `None` if `id` is unknown.
    pub fn comment(&self, id: VariableId) -> Option<&str> {
        self.variables
            .get(id.0 as usize)
            .map(|variable| variable.comment.as_str())
    }

    /// Replace `id`'s comment. Fails if `id` is unknown.
    pub fn set_comment(
        &mut self,
        id: VariableId,
        comment: impl Into<String>,
    ) -> Result<(), VariablesError> {
        let variable = self
            .variables
            .get_mut(id.0 as usize)
            .ok_or(VariablesError::UnknownHandle)?;
        variable.comment = comment.into();
        Ok(())
    }

    /// Every namespace with at least one variable defined in it, sorted and
    /// deduplicated.
    pub fn namespaces(&self) -> impl Iterator<Item = Namespace> {
        self.variables
            .iter()
            .map(|variable| variable.name.namespace().clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    /// Every variable defined in this system, across all namespaces.
    pub fn variables(&self) -> impl Iterator<Item = &Variable> {
        self.variables.iter()
    }

    /// Handles of every variable defined directly in `namespace`.
    pub fn variables_in(&self, namespace: &Namespace) -> impl Iterator<Item = VariableId> {
        self.variables
            .iter()
            .enumerate()
            .filter(move |(_, variable)| variable.name.namespace() == namespace)
            .map(|(index, _)| VariableId(index as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns(value: &str) -> Namespace {
        Namespace::new(value)
    }

    fn fqn(value: &str) -> FQName {
        FQName::parse(value).unwrap()
    }

    #[test]
    fn define_variable_in_namespace_succeeds_and_returns_handle() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("8.99e9").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.value(id).unwrap(), 8.99e9);
    }

    #[test]
    fn value_of_constant_expression_variable() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns(""),
                "a",
                CompiledExpression::parse("12.375").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.value(id).unwrap(), 12.375);
    }

    #[test]
    fn value_of_variable_referencing_another_variable() {
        let mut vars = VariablesSystem::default();
        let a = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("3.0").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        let _ = a;
        let b = vars
            .define(
                &ns("globals"),
                "b",
                CompiledExpression::parse("globals.a * 2").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.value(b).unwrap(), 6.0);
    }

    #[test]
    fn redefine_updates_the_value_without_changing_the_handle() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        vars.set(id, CompiledExpression::parse("2").unwrap())
            .unwrap();
        assert_eq!(vars.value(id).unwrap(), 2.0);
    }

    #[test]
    fn redefine_leaves_existing_comment_untouched() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions {
                    comment: "speed of light".to_owned(),
                },
            )
            .unwrap();
        vars.set(id, CompiledExpression::parse("2").unwrap())
            .unwrap();
        assert_eq!(vars.comment(id), Some("speed of light"));
    }

    #[test]
    fn value_reflects_dependency_redefined_after_first_read() {
        let mut vars = VariablesSystem::default();
        let a = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        let b = vars
            .define(
                &ns("globals"),
                "b",
                CompiledExpression::parse("globals.a + 1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.value(b).unwrap(), 2.0);
        vars.set(a, CompiledExpression::parse("10").unwrap())
            .unwrap();
        assert_eq!(vars.value(b).unwrap(), 11.0);
    }

    #[test]
    fn duplicate_name_in_same_namespace_is_rejected() {
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("globals"),
            "a",
            CompiledExpression::parse("1").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        let err = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("2").unwrap(),
                VariableOptions::default(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            VariablesError::DuplicateName {
                namespace: ns("globals"),
                name: Name::new("a").unwrap(),
            }
        );
    }

    #[test]
    fn same_name_allowed_in_two_different_namespaces() {
        let mut vars = VariablesSystem::default();
        let a = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        let b = vars
            .define(
                &ns("locals"),
                "a",
                CompiledExpression::parse("2").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(vars.value(a).unwrap(), 1.0);
        assert_eq!(vars.value(b).unwrap(), 2.0);
    }

    #[test]
    fn two_definitions_land_in_the_same_shared_namespace() {
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("globals.electricity"),
            "K",
            CompiledExpression::parse("8.99e9").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        vars.define(
            &ns("globals.electricity"),
            "epsilon0",
            CompiledExpression::parse("8.854e-12").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        assert_eq!(vars.variables_in(&ns("globals.electricity")).count(), 2);
    }

    #[test]
    fn plugin_private_namespace_does_not_collide_with_shared_namespace() {
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("globals.electricity"),
            "K",
            CompiledExpression::parse("8.99e9").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        vars.define(
            &ns("globals.plugins.maxwell"),
            "step_limit",
            CompiledExpression::parse("1000").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        let namespaces = vars.namespaces().collect::<Vec<_>>();
        assert!(namespaces.contains(&ns("globals.electricity")));
        assert!(namespaces.contains(&ns("globals.plugins.maxwell")));
    }

    #[test]
    fn self_referencing_variable_value_returns_cycle_error() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("globals.a + 1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(
            vars.value(id).unwrap_err(),
            VariablesError::Eval(ExprEvalError::Cycle)
        );
    }

    #[test]
    fn mutually_referencing_variables_value_returns_cycle_error() {
        let mut vars = VariablesSystem::default();
        let a = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("globals.b").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        vars.define(
            &ns("globals"),
            "b",
            CompiledExpression::parse("globals.a").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        assert_eq!(
            vars.value(a).unwrap_err(),
            VariablesError::Eval(ExprEvalError::Cycle)
        );
    }

    #[test]
    fn value_of_expression_referencing_unknown_variable_returns_error() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("globals.missing + 1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(
            vars.value(id).unwrap_err(),
            VariablesError::Eval(ExprEvalError::UnknownVariable("globals.missing".to_owned()))
        );
    }

    #[test]
    fn find_resolves_qualified_name_to_the_defined_handle() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.lookup(&fqn("globals.electricity.K")), Some(id));
    }

    #[test]
    fn find_returns_none_for_unknown_namespace_or_name() {
        let vars = VariablesSystem::default();
        assert_eq!(vars.lookup(&fqn("globals.electricity.K")), None);
    }

    #[test]
    fn lookup_in_resolves_a_variable_by_split_namespace_and_name() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(
            vars.lookup_in(&ns("globals.electricity"), &Name::new("K").unwrap()),
            Some(id)
        );
    }

    #[test]
    fn lookup_in_returns_none_for_unknown_namespace_or_name() {
        let vars = VariablesSystem::default();
        assert_eq!(
            vars.lookup_in(&ns("globals.electricity"), &Name::new("K").unwrap()),
            None
        );
    }

    #[test]
    fn namespaces_lists_every_namespace_with_at_least_one_variable() {
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("a"),
            "x",
            CompiledExpression::parse("1").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        vars.define(
            &ns("b"),
            "y",
            CompiledExpression::parse("2").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        assert_eq!(
            vars.namespaces().collect::<Vec<_>>(),
            vec![ns("a"), ns("b")]
        );
    }

    #[test]
    fn variables_in_lists_only_that_namespaces_variables() {
        let mut vars = VariablesSystem::default();
        let a = vars
            .define(
                &ns("a"),
                "x",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        vars.define(
            &ns("b"),
            "y",
            CompiledExpression::parse("2").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        assert_eq!(vars.variables_in(&ns("a")).collect::<Vec<_>>(), vec![a]);
    }

    #[test]
    fn eval_ad_hoc_constant_expression() {
        let vars = VariablesSystem::default();
        assert_eq!(vars.eval("2 + 2 * 2").unwrap(), 6.0);
    }

    #[test]
    fn eval_ad_hoc_expression_referencing_defined_variables() {
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("globals.electricity"),
            "K",
            CompiledExpression::parse("8.99e9").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        assert_eq!(
            vars.eval("globals.electricity.K * 2").unwrap(),
            8.99e9 * 2.0
        );
    }

    #[test]
    fn eval_ad_hoc_expression_with_unknown_variable_errors() {
        let vars = VariablesSystem::default();
        assert_eq!(
            vars.eval("globals.missing").unwrap_err(),
            VariablesError::Eval(ExprEvalError::UnknownVariable("globals.missing".to_owned()))
        );
    }

    #[test]
    fn eval_does_not_register_a_variable() {
        let vars = VariablesSystem::default();
        vars.eval("1 + 1").unwrap();
        assert!(vars.namespaces().next().is_none());
    }

    #[test]
    fn eval_division_by_zero_returns_error() {
        let vars = VariablesSystem::default();
        assert_eq!(
            vars.eval("1 / 0").unwrap_err(),
            VariablesError::Eval(ExprEvalError::DivisionByZero)
        );
    }

    #[test]
    fn new_variable_has_the_comment_from_define_options() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions {
                    comment: "a comment".to_owned(),
                },
            )
            .unwrap();
        assert_eq!(vars.comment(id), Some("a comment"));
    }

    #[test]
    fn set_comment_replaces_previous_comment() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals"),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        vars.set_comment(id, "updated").unwrap();
        assert_eq!(vars.comment(id), Some("updated"));
    }

    #[test]
    fn comment_of_unknown_handle_returns_none() {
        let vars = VariablesSystem::default();
        assert_eq!(vars.comment(VariableId(9999)), None);
    }

    #[test]
    fn name_returns_the_defined_variables_own_name() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.name(id), Some(&Name::new("K").unwrap()));
    }

    #[test]
    fn define_rejects_an_invalid_name() {
        let mut vars = VariablesSystem::default();
        let err = vars
            .define(
                &ns("globals"),
                "1bad",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            VariablesError::InvalidName(InvalidName::StartsWithDigit("1bad".to_owned()))
        );
    }

    #[test]
    fn define_accepts_an_owned_name_via_try_into() {
        let mut vars = VariablesSystem::default();
        let name = Name::new("a").unwrap();
        let id = vars
            .define(
                &ns("globals"),
                name,
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.value(id).unwrap(), 1.0);
    }

    #[test]
    fn qualified_name_combines_namespace_and_name() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(
            vars.qualified_name(id),
            Some(&FQName::parse("globals.electricity.K").unwrap())
        );
    }

    #[test]
    fn qualified_name_in_root_namespace_has_no_leading_dot() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns(""),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.qualified_name(id).unwrap().to_string(), "a");
    }

    #[test]
    fn qualified_name_of_unknown_handle_returns_none() {
        let vars = VariablesSystem::default();
        assert_eq!(vars.qualified_name(VariableId(9999)), None);
    }

    #[test]
    fn resolve_symbol_rejects_a_syntactically_invalid_symbol() {
        let vars = VariablesSystem::default();
        assert_eq!(
            vars.resolve_symbol("globals.1bad"),
            Err(ExprEvalError::UnknownVariable("globals.1bad".to_owned()))
        );
    }

    #[test]
    fn resolve_symbol_finds_a_qualified_variable() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns("globals.electricity"),
                "K",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.resolve_symbol("globals.electricity.K"), Ok(id));
    }

    #[test]
    fn resolve_symbol_finds_a_bare_variable_in_the_root_namespace() {
        let mut vars = VariablesSystem::default();
        let id = vars
            .define(
                &ns(""),
                "a",
                CompiledExpression::parse("1").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(vars.resolve_symbol("a"), Ok(id));
    }
}
