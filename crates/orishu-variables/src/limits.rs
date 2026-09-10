//! Declared bounds on everything an expression can ask this engine to do.
//!
//! ADR 0005 and ADR 0007 make this crate a shared resource contract: the same
//! parser and evaluator serve Kagami authoring — where MCP is a first-class
//! caller — and, later, Orishu workload admission. Every expression reaching it
//! is therefore untrusted input, and every dimension it can consume is bounded
//! here as one named value rather than as a literal at the point of use, so a
//! network-facing caller can tighten them and a test can drive a limit failure
//! without building a gigantic system.
//!
//! Bounds are an owned value on the caller's side of the interface. There is no
//! global, thread-local, or otherwise ambient limit state: a
//! [`crate::VariablesSystem`] carries the limits it was built with, and the
//! parser is handed the limits for the source it is about to read.
//!
//! Several of these are load-bearing for *ordering*, not only for size:
//!
//! - [`Limits::max_expression_bytes`] is checked before the source is lexed or
//!   retained, so an oversized expression is never copied in order to be
//!   rejected.
//! - [`Limits::max_expression_nodes`] is charged as each node is admitted, so a
//!   bound is a bound on what a source can make the parser *allocate*, not only
//!   on what it may declare.
//! - [`Limits::max_variables`] is checked before a [`crate::VariablesSystem`]'s
//!   vector or index is touched, so a refused definition leaves the system
//!   exactly as it was.
//!
//! # Depth is counted twice, against one number
//!
//! [`Limits::max_parse_depth`] bounds both the parser's *recursion* and the
//! *depth of the tree it builds*, because neither implies the other:
//!
//! - `((((1))))` recurses once per parenthesis but produces a tree of depth 1.
//!   Only the recursion counter stops that source before the Rust call stack
//!   does.
//! - `a+b+c+…` recurses twice — the Pratt loop folds a left-associative chain
//!   without recursing — but produces a left-deep tree as deep as the chain is
//!   long. Only the tree counter stops that from turning
//!   [`crate::CompiledExpression::variables`], `is_const`, and evaluation, all
//!   of which walk the tree recursively, into unbounded stack use.
//!
//! One number for both is what makes "symbol collection cannot bypass the
//! bounds" true by construction rather than by a second check somewhere else.

use std::cell::Cell;

use thiserror::Error;

/// Which declared bound refused an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    /// [`Limits::max_expression_bytes`].
    ExpressionBytes,
    /// [`Limits::max_parse_depth`], as either parser recursion or tree depth.
    ParseDepth,
    /// [`Limits::max_expression_nodes`].
    ExpressionNodes,
    /// [`Limits::max_variables`].
    Variables,
    /// [`Limits::max_dependency_depth`].
    DependencyDepth,
    /// [`Limits::max_evaluation_work`].
    EvaluationWork,
}

impl LimitKind {
    /// What this bound counts, phrased to read as the subject of
    /// [`LimitError`]'s message.
    pub fn subject(self) -> &'static str {
        match self {
            Self::ExpressionBytes => "expression source is",
            Self::ParseDepth => "expression depth is",
            Self::ExpressionNodes => "expression node count is",
            Self::Variables => "variable count is",
            Self::DependencyDepth => "dependency depth is",
            Self::EvaluationWork => "evaluation work is",
        }
    }
}

/// A declared bound refused an operation, naming the dimension exceeded, the
/// value that would have been reached, and the bound that was configured.
///
/// A refusal that belongs to source text is reported as an
/// [`crate::ExprParsingError`] wrapping this, so the failure keeps its
/// [`crate::SourceSpan`]; the dimensions that do not belong to a span — the
/// variable count, the dependency depth, the evaluation budget — carry it
/// directly.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("{} {found}, over the limit of {allowed}", limit.subject())]
pub struct LimitError {
    /// The dimension that was exceeded.
    pub limit: LimitKind,
    /// The value that would have been reached had the operation continued.
    ///
    /// `u64::MAX` in the (unreachable in practice) case that the counter
    /// itself would have overflowed, which is reported rather than wrapped.
    pub found: u64,
    /// The bound that was configured.
    pub allowed: u64,
}

/// Build a refusal.
///
/// Outlined and cold on purpose. A [`LimitError`] is wider than a register
/// pair, so `Result<_, LimitError>` is returned through memory; keeping the
/// construction out of line leaves the checks below small enough to inline
/// into their callers, which is what stops a bound costing a call and a
/// memory round trip on the path that always succeeds.
#[cold]
#[inline(never)]
fn exceeded(limit: LimitKind, found: u64, allowed: usize) -> LimitError {
    LimitError {
        limit,
        found,
        allowed: allowed as u64,
    }
}

/// Refuse when `found` is already over `allowed`.
#[inline]
pub(crate) fn ensure(limit: LimitKind, found: usize, allowed: usize) -> Result<(), LimitError> {
    if found > allowed {
        return Err(exceeded(limit, found as u64, allowed));
    }
    Ok(())
}

/// Account for one more of whatever `limit` counts, returning the new count.
///
/// Refuses *before* the caller commits to the thing being counted, which is
/// what lets a parser reject a node it has not built and a system reject a
/// variable it has not pushed. The increment is checked, so a caller that
/// configured a bound at the top of the range still gets a refusal rather than
/// a wrapped counter.
#[inline]
pub(crate) fn admit_one(
    limit: LimitKind,
    current: usize,
    allowed: usize,
) -> Result<usize, LimitError> {
    let Some(next) = current.checked_add(1) else {
        return Err(exceeded(limit, u64::MAX, allowed));
    };
    if next > allowed {
        return Err(exceeded(limit, next as u64, allowed));
    }
    Ok(next)
}

/// The evaluation budget of one top-level evaluation.
///
/// A [`Cell`] rather than a `&mut` counter because
/// [`crate::VariablesSystem::eval`] resolves dependencies from inside a closure
/// that already holds the evaluation scratch buffers mutably, and the same
/// budget must be charged from both sides of that borrow. It is a plain local
/// value created per call, never shared state: two concurrent evaluations get
/// two budgets.
pub(crate) struct Budget {
    spent: Cell<u64>,
    allowed: u64,
}

impl Budget {
    /// A budget of `allowed` units of work.
    #[inline]
    pub(crate) fn new(allowed: u64) -> Self {
        Self {
            spent: Cell::new(0),
            allowed,
        }
    }

    /// Charge `units` of work, refusing when the total would exceed the bound.
    ///
    /// Charged once per syntax-tree node, so this is the hottest of the
    /// bounds; inlined so that what a node actually pays is a load, an add, a
    /// compare, and a store.
    #[inline]
    pub(crate) fn spend(&self, units: u64) -> Result<(), LimitError> {
        let Some(spent) = self.spent.get().checked_add(units) else {
            return Err(self.exhausted(u64::MAX));
        };
        if spent > self.allowed {
            return Err(self.exhausted(spent));
        }
        self.spent.set(spent);
        Ok(())
    }

    /// The refusal, outlined for the same reason as [`exceeded`].
    #[cold]
    #[inline(never)]
    fn exhausted(&self, found: u64) -> LimitError {
        LimitError {
            limit: LimitKind::EvaluationWork,
            found,
            allowed: self.allowed,
        }
    }
}

/// Bounds applied while parsing, rewriting, defining, and evaluating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest expression source accepted, in bytes.
    ///
    /// Checked before the source is lexed or retained, and again against the
    /// output of [`crate::rewrite_symbols_bounded`], whose result is another
    /// expression source.
    pub max_expression_bytes: usize,
    /// Deepest expression the parser will read or build.
    ///
    /// Bounds the parser's recursion *and* the depth of the resulting tree; see
    /// the module documentation for why one number governs both.
    pub max_parse_depth: usize,
    /// Largest number of syntax-tree nodes one expression may contain.
    pub max_expression_nodes: usize,
    /// Largest number of variables one [`crate::VariablesSystem`] may hold.
    pub max_variables: usize,
    /// Deepest chain of variable references one resolution will follow.
    pub max_dependency_depth: usize,
    /// Largest total work one top-level evaluation may cost.
    ///
    /// One unit is one syntax-tree node walked. A variable reached during a
    /// resolution is walked twice — once to collect the dependencies it
    /// references, once to compute its value — and, because resolution memoizes
    /// per call, exactly twice however many paths reach it. This is the bound
    /// that stops a *wide* graph, which neither the variable count nor the
    /// dependency depth constrains: 3 000 variables of 300 nodes each is a
    /// shallow graph and nearly two million units of work.
    pub max_evaluation_work: u64,
}

impl Limits {
    /// Bounds sized for interactive authoring, applied by every entry point
    /// that does not take limits explicitly.
    ///
    /// Comfortably above any expression or variable set a person writes by
    /// hand, while keeping the worst case one untrusted expression can cost
    /// small enough to evaluate eagerly. A caller that needs different numbers
    /// — a batch importer, a benchmark, a stricter MCP-facing authority —
    /// passes its own through [`crate::VariablesSystem::with_limits`] or
    /// [`crate::CompiledExpression::parse_bounded`].
    ///
    /// Two of these are set by what a *consumer* may legitimately ask for
    /// rather than by taste:
    ///
    /// - `max_expression_bytes` is four times `kagami-catalog`'s and
    ///   `kagami-document`'s own 1 KiB expression bound, so this engine is the
    ///   backstop for those layers rather than the thing that refuses first.
    /// - `max_variables` is generous because a catalog projection defines two
    ///   variables — a canonical name and a concise alias — for each of the
    ///   65 536 bindings `kagami-catalog` permits. A bound below that would
    ///   turn a legal catalog into a refusal.
    /// - `max_dependency_depth` is generous for the same reason, halved: a
    ///   reference written in a catalog expression resolves through the alias
    ///   *and* the canonical binding it names, so a chain of templates costs
    ///   two levels per link. This admits a chain roughly two thousand
    ///   templates long. It is not the bound that makes a runaway graph
    ///   affordable — `max_evaluation_work` is — because resolution walks the
    ///   heap rather than the call stack, so depth costs a vector entry.
    pub const DEFAULT: Self = Self {
        max_expression_bytes: 4_096,
        max_parse_depth: 128,
        max_expression_nodes: 4_096,
        max_variables: 262_144,
        max_dependency_depth: 4_096,
        max_evaluation_work: 1_000_000,
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
    fn every_default_limit_is_non_zero() {
        // A zero bound would refuse every expression, including a valid one,
        // and the failure would look like a malformed source rather than a
        // misconfigured caller.
        let limits = Limits::DEFAULT;
        assert!(limits.max_expression_bytes > 0);
        assert!(limits.max_parse_depth > 0);
        assert!(limits.max_expression_nodes > 0);
        assert!(limits.max_variables > 0);
        assert!(limits.max_dependency_depth > 0);
        assert!(limits.max_evaluation_work > 0);
    }

    #[test]
    fn every_default_bound_is_reachable() {
        // A node needs at least one byte of source and a tree at least as many
        // nodes as it is deep, so a bound above the one below it could never
        // be the one that fires. A `const` block, so the defaults are checked
        // when the crate compiles rather than when the suite runs.
        const {
            assert!(Limits::DEFAULT.max_expression_nodes <= Limits::DEFAULT.max_expression_bytes);
            assert!(Limits::DEFAULT.max_parse_depth <= Limits::DEFAULT.max_expression_nodes);
        }
    }

    #[test]
    fn admit_one_refuses_the_increment_that_would_pass_the_bound() {
        assert_eq!(admit_one(LimitKind::Variables, 1, 2), Ok(2));
        assert_eq!(
            admit_one(LimitKind::Variables, 2, 2),
            Err(LimitError {
                limit: LimitKind::Variables,
                found: 3,
                allowed: 2,
            })
        );
    }

    #[test]
    fn admit_one_reports_rather_than_wraps_a_saturated_counter() {
        assert_eq!(
            admit_one(LimitKind::ExpressionNodes, usize::MAX, usize::MAX),
            Err(LimitError {
                limit: LimitKind::ExpressionNodes,
                found: u64::MAX,
                allowed: usize::MAX as u64,
            })
        );
    }

    #[test]
    fn a_budget_spends_up_to_its_bound_and_no_further() {
        let budget = Budget::new(3);
        assert_eq!(budget.spend(2), Ok(()));
        assert_eq!(budget.spend(1), Ok(()));
        assert_eq!(
            budget.spend(1),
            Err(LimitError {
                limit: LimitKind::EvaluationWork,
                found: 4,
                allowed: 3,
            })
        );
    }

    #[test]
    fn a_refused_charge_does_not_consume_the_budget() {
        // Otherwise a caller that recovered from one refusal would find the
        // budget silently emptied by the charge that failed.
        let budget = Budget::new(3);
        assert!(budget.spend(4).is_err());
        assert_eq!(budget.spend(3), Ok(()));
    }

    #[test]
    fn limit_errors_name_their_dimension_and_bound() {
        let error = LimitError {
            limit: LimitKind::ExpressionBytes,
            found: 5_000,
            allowed: 4_096,
        };
        assert_eq!(
            error.to_string(),
            "expression source is 5000, over the limit of 4096"
        );
    }
}
