use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, PoisonError};

use thiserror::Error;

use crate::expression::{CompiledExpression, ExprEvalError, ExprParsingError, eval_ast};
use crate::namespace::{FQName, IntoName, InvalidName, Name, Namespace};
use crate::quantity::{self, Quantity};
use crate::variable::{Variable, VariableId, VariableOptions};

/// Everything that can go wrong when defining, redefining, or evaluating
/// variables through a [`VariablesSystem`].
#[derive(Debug, Error, PartialEq)]
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
    /// A root-namespace name would shadow a unit symbol.
    ///
    /// Units resolve as ordinary symbols, so a root variable called `m` would
    /// silently change what every expression using metres means. Only the
    /// root namespace can collide — a namespaced variable is reached by its
    /// qualified name — so `electricity.K` is accepted and a bare `K` is not.
    #[error("`{name}` is a unit symbol, so it cannot also be a root variable name")]
    ShadowsUnit {
        /// The name that was refused.
        name: Name,
    },
}

/// Subsystem that owns a set of namespaced variables and computes their
/// values, resolving references between them on demand (idCVarSystem-style,
/// but with no notion of documents).
///
/// A symbol resolves to a variable if one is defined, and otherwise to a unit
/// from the shared table. Definition refuses a name the unit table already
/// claims, so that fallback can never be shadowed out from under an
/// expression that relied on it.
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
    /// Scratch buffers for [`Self::value`]/[`Self::eval`]'s dependency walk,
    /// checked out for the duration of one top-level call and returned
    /// (cleared) afterward, so their capacity is amortized across calls
    /// instead of reallocating from scratch every time. A `Mutex` (not
    /// `RefCell`) to keep this type `Sync`-safe, matching `SampleCache`'s
    /// interior-mutability convention elsewhere in the workspace.
    scratch_pool: Mutex<EvalScratch>,
}

/// One frame of the explicit dependency-resolution stack: which variable is
/// being resolved, and how far through its dependency list we have got.
#[derive(Clone, Copy)]
struct Frame {
    id: VariableId,
    /// Where this frame's dependencies start in [`EvalScratch::deps`].
    deps_start: usize,
    /// Index of the next dependency to visit, into that same arena.
    next: usize,
}

/// The working set of one top-level evaluation, walked iteratively so that
/// resolution depth is bounded by the heap rather than by the thread's
/// stack — a long dependency chain used to cost one Rust frame per link.
#[derive(Default)]
struct EvalScratch {
    stack: Vec<Frame>,
    /// LIFO arena of resolved dependency handles: each frame owns the slice
    /// from its `deps_start` to the arena's end while it is on top, and
    /// truncates back to `deps_start` when it pops.
    deps: Vec<VariableId>,
    /// `None` = on the stack right now, so a reference back to it is a
    /// cycle; `Some(value)` = fully evaluated during this call. Doubling as
    /// the memo means a shared dependency is evaluated once per call rather
    /// than once per path that reaches it.
    state: HashMap<VariableId, Option<Quantity>>,
}

impl EvalScratch {
    fn clear(&mut self) {
        self.stack.clear();
        self.deps.clear();
        self.state.clear();
    }
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
        // Only a *root* name can be confused with a unit: reaching a
        // namespaced variable requires writing its qualified name, and `K`
        // in an expression can never mean `globals.electricity.K`. So
        // Coulomb's constant may be called `K` where it belongs, and only a
        // bare `K` — which would silently retune every expression using
        // kelvin — is refused.
        if namespace.is_root() && quantity::lookup(name.as_str()).is_ok() {
            return Err(VariablesError::ShadowsUnit { name });
        }
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

    /// Compute `id`'s current value, resolving any variables it depends on,
    /// however deep the chain. Recomputed on every call (no caching across
    /// calls; within one call each variable is evaluated once).
    pub fn value(&self, id: VariableId) -> Result<Quantity, VariablesError> {
        if self.variables.get(id.0 as usize).is_none() {
            return Err(VariablesError::UnknownHandle);
        }
        let mut scratch = self.take_scratch();
        let result = self.resolve(id, &mut scratch);
        self.return_scratch(scratch);
        result.map_err(VariablesError::from)
    }

    /// Checks out the shared scratch buffers, leaving empty ones behind. The
    /// lock is held only for this swap, never across the evaluation that
    /// follows.
    fn take_scratch(&self) -> EvalScratch {
        let mut scratch = self
            .scratch_pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *scratch)
    }

    /// Clears and returns buffers previously obtained from
    /// [`Self::take_scratch`], so their capacity is reused by the next call.
    /// Clearing is what keeps [`Self::value`]'s "recomputed on every call"
    /// contract honest: the memo never outlives the call that filled it, so
    /// a later `set` can never be masked by a stale value.
    fn return_scratch(&self, mut scratch: EvalScratch) {
        scratch.clear();
        *self
            .scratch_pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = scratch;
    }

    /// Evaluates `root`, resolving its transitive dependencies with an
    /// explicit stack rather than by recursing once per link, so a chain
    /// only as deep as the heap allows still resolves. Every variable
    /// reached is evaluated exactly once and memoized in `scratch.state`.
    fn resolve(
        &self,
        root: VariableId,
        scratch: &mut EvalScratch,
    ) -> Result<Quantity, ExprEvalError> {
        if let Some(&Some(value)) = scratch.state.get(&root) {
            return Ok(value);
        }
        // Borrows of the ASTs being walked, reused across every frame push
        // of this call instead of allocated per node.
        let mut symbols: Vec<&str> = Vec::new();
        self.push_frame(root, scratch, &mut symbols)?;

        while let Some(&Frame {
            id,
            deps_start,
            next,
        }) = scratch.stack.last()
        {
            let top = scratch.stack.len() - 1;
            // While this frame is on top, its dependencies are exactly the
            // tail of the arena from `deps_start` onward.
            if next < scratch.deps.len() {
                let dependency = scratch.deps[next];
                scratch.stack[top].next += 1;
                match scratch.state.get(&dependency) {
                    // Already evaluated on another path this call.
                    Some(Some(_)) => {}
                    // Still on the stack, so this edge closes a loop.
                    Some(None) => return Err(ExprEvalError::Cycle),
                    None => self.push_frame(dependency, scratch, &mut symbols)?,
                }
                continue;
            }

            let ast = &self.variables[id.0 as usize].expression.ast;
            let state = &scratch.state;
            let value = eval_ast(ast, &mut |raw: &str| {
                let Some(target) = self.symbol_variable(raw) else {
                    return unit_quantity(raw);
                };
                // Every symbol of this expression that named a variable was
                // pushed as a dependency and evaluated above, so this lookup
                // always hits; treating a miss as a cycle keeps an unforeseen
                // gap an error, not a panic.
                state
                    .get(&target)
                    .copied()
                    .flatten()
                    .ok_or(ExprEvalError::Cycle)
            })?;
            scratch.state.insert(id, Some(value));
            scratch.deps.truncate(deps_start);
            scratch.stack.pop();
        }

        scratch
            .state
            .get(&root)
            .copied()
            .flatten()
            .ok_or(ExprEvalError::Cycle)
    }

    /// Marks `id` as in progress and pushes a frame for it, appending its
    /// resolved dependencies to the arena. Fails if any symbol it references
    /// is undefined.
    fn push_frame<'a>(
        &'a self,
        id: VariableId,
        scratch: &mut EvalScratch,
        symbols: &mut Vec<&'a str>,
    ) -> Result<(), ExprEvalError> {
        let deps_start = scratch.deps.len();
        symbols.clear();
        self.variables[id.0 as usize]
            .expression
            .ast
            .collect_symbol_refs(symbols);
        for raw in symbols.iter() {
            // A symbol no variable defines may still be a unit, which has no
            // dependencies of its own. Resolving it here would report an
            // unknown variable for `kg`.
            match self.symbol_variable(raw) {
                Some(dependency) => scratch.deps.push(dependency),
                None => unit_quantity(raw).map(|_| ())?,
            }
        }
        scratch.state.insert(id, None);
        scratch.stack.push(Frame {
            id,
            deps_start,
            next: deps_start,
        });
        Ok(())
    }

    /// Parse and evaluate an ad-hoc expression against this system's
    /// currently defined variables, without registering it as a variable.
    pub fn eval(&self, expr_source: &str) -> Result<Quantity, VariablesError> {
        let compiled = CompiledExpression::parse(expr_source)?;
        let mut scratch = self.take_scratch();
        let value = eval_ast(
            &compiled.ast,
            &mut |raw: &str| match self.symbol_variable(raw) {
                Some(target) => self.resolve(target, &mut scratch),
                None => unit_quantity(raw),
            },
        );
        self.return_scratch(scratch);
        value.map_err(VariablesError::from)
    }

    /// Resolve a raw dotted symbol (as captured verbatim by a symbol node
    /// at parse time) to its handle, with no allocation on the success
    /// path — `raw` is already in the index's canonical form, so this is a
    /// direct lookup rather than a round trip through [`FQName::parse`].
    pub fn resolve_symbol(&self, raw: &str) -> Result<VariableId, ExprEvalError> {
        self.index
            .get(raw)
            .copied()
            .ok_or_else(|| ExprEvalError::UnknownVariable(raw.to_owned()))
    }

    /// The variable a raw dotted symbol names, if one is defined.
    ///
    /// Distinct from [`Self::resolve_symbol`] because a symbol that names no
    /// variable is not necessarily an error: it may be a unit.
    pub fn symbol_variable(&self, raw: &str) -> Option<VariableId> {
        self.index.get(raw).copied()
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

/// The quantity a bare unit symbol denotes: its SI factor, in its dimension.
///
/// `kg` is 1 kilogram, `km` is 1000 metres. That is what makes `2.7 g` an
/// ordinary product and keeps units out of the grammar.
fn unit_quantity(symbol: &str) -> Result<Quantity, ExprEvalError> {
    let unit =
        quantity::lookup(symbol).map_err(|_| ExprEvalError::UnknownVariable(symbol.to_owned()))?;
    Quantity::new(unit.si_factor(), unit.dimension()).map_err(ExprEvalError::from)
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
        assert_eq!(vars.value(id).unwrap().magnitude(), 8.99e9);
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
        assert_eq!(vars.value(id).unwrap().magnitude(), 12.375);
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
        assert_eq!(vars.value(b).unwrap().magnitude(), 6.0);
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
        assert_eq!(vars.value(id).unwrap().magnitude(), 2.0);
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
        assert_eq!(vars.value(b).unwrap().magnitude(), 2.0);
        vars.set(a, CompiledExpression::parse("10").unwrap())
            .unwrap();
        assert_eq!(vars.value(b).unwrap().magnitude(), 11.0);
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
        assert_eq!(vars.value(a).unwrap().magnitude(), 1.0);
        assert_eq!(vars.value(b).unwrap().magnitude(), 2.0);
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

    /// `v0 = 0`, `v1 = chain.v0 + 1`, ..., returning the tail's handle.
    fn build_chain(vars: &mut VariablesSystem, depth: usize) -> VariableId {
        let mut previous = vars
            .define(
                &ns("chain"),
                "v0",
                CompiledExpression::parse("0").unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        for i in 1..depth {
            previous = vars
                .define(
                    &ns("chain"),
                    format!("v{i}"),
                    CompiledExpression::parse(&format!("chain.v{} + 1", i - 1)).unwrap(),
                    VariableOptions::default(),
                )
                .unwrap();
        }
        previous
    }

    #[test]
    fn deep_dependency_chain_does_not_overflow_the_stack() {
        let mut vars = VariablesSystem::default();
        let tail = build_chain(&mut vars, 100_000);
        assert_eq!(vars.value(tail).unwrap().magnitude(), 99_999.0);
    }

    #[test]
    fn deep_dependency_chain_is_resolvable_through_eval() {
        let mut vars = VariablesSystem::default();
        build_chain(&mut vars, 100_000);
        assert_eq!(
            vars.eval("chain.v99999 + 1").unwrap().magnitude(),
            100_000.0
        );
    }

    #[test]
    fn cycle_deep_in_a_long_chain_is_still_detected() {
        let mut vars = VariablesSystem::default();
        let tail = build_chain(&mut vars, 10_000);
        let head = vars
            .lookup_in(&ns("chain"), &Name::new("v0").unwrap())
            .unwrap();
        vars.set(head, CompiledExpression::parse("chain.v9999").unwrap())
            .unwrap();
        assert_eq!(
            vars.value(tail).unwrap_err(),
            VariablesError::Eval(ExprEvalError::Cycle)
        );
    }

    #[test]
    fn shared_dependencies_are_evaluated_once_per_call() {
        // Each level references the one below it twice, so without
        // memoization this is 2^40 evaluations and never returns.
        let mut vars = VariablesSystem::default();
        vars.define(
            &ns("chain"),
            "v0",
            CompiledExpression::parse("1").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
        let mut tail = None;
        for i in 1..40 {
            let source = format!("(chain.v{prev} + chain.v{prev}) / 2", prev = i - 1);
            tail = Some(
                vars.define(
                    &ns("chain"),
                    format!("v{i}"),
                    CompiledExpression::parse(&source).unwrap(),
                    VariableOptions::default(),
                )
                .unwrap(),
            );
        }
        assert_eq!(vars.value(tail.unwrap()).unwrap().magnitude(), 1.0);
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
        assert_eq!(vars.eval("2 + 2 * 2").unwrap().magnitude(), 6.0);
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
            vars.eval("globals.electricity.K * 2").unwrap().magnitude(),
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
        assert_eq!(vars.value(id).unwrap().magnitude(), 1.0);
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
