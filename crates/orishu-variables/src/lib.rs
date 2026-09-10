//! Variables and expressions sub-system, in the idCVarSystem tradition.
//!
//! This crate knows nothing about CAD, solvers, or documents, and is
//! deliberately kept generic. It lets callers define namespaced variables
//! bound to parsed math expressions, and expressions can reference other
//! variables by their fully qualified `namespace.name`.
//!
//! # One grammar, dimensioned values
//!
//! Evaluation produces a [`Quantity`] — a canonical-SI magnitude plus the
//! [`Dimension`] the arithmetic derived — rather than a bare number, so
//! `2.7 g / cm^3` is a density and `1 kg + 1 m` is an error. Units are
//! ordinary symbols resolved from a shared table, which is what lets one
//! parser carry them instead of forking into a dimensionless grammar and a
//! unit-aware one. A pure number is simply a dimensionless quantity.
//!
//! A consumer that knows the dimension it *expected* compares it against the
//! one that was derived; this crate does not know what a property or a
//! workload field is.
//!
//! # Bounded, and bounded by the caller
//!
//! Every expression reaching this crate is untrusted: it may have come from an
//! MCP client, a hand-edited catalog file, or a workload manifest submitted by
//! a peer. Source length, parse depth, node count, variable count, dependency
//! depth, and total evaluation work are therefore all bounded by an explicit
//! [`Limits`] value, and a refusal is a structured [`LimitError`] naming the
//! dimension, the value reached, and the bound configured.
//!
//! The bounds are the caller's, never the process's. Convenient entry points —
//! [`CompiledExpression::parse`], [`VariablesSystem::default`],
//! [`rewrite_symbols`] — apply the documented [`Limits::DEFAULT`]; the
//! bounded siblings [`CompiledExpression::parse_bounded`],
//! [`VariablesSystem::with_limits`], and [`rewrite_symbols_bounded`] take
//! whatever the caller declares.
//!
//! ```
//! use orishu_variables::{CompiledExpression, LimitKind, Limits};
//!
//! // The convenient path is bounded too: this would be a stack overflow in an
//! // unbounded recursive-descent parser.
//! let nested = format!("{}1{}", "(".repeat(2_000), ")".repeat(2_000));
//! let refused = CompiledExpression::parse(&nested).unwrap_err();
//! assert_eq!(
//!     refused.limit_error().map(|error| error.limit),
//!     Some(LimitKind::ParseDepth)
//! );
//!
//! // A caller that needs different bounds says so.
//! let strict = Limits {
//!     max_expression_bytes: 8,
//!     ..Limits::DEFAULT
//! };
//! assert!(CompiledExpression::parse_bounded("2.7 g", &strict).is_ok());
//! assert!(CompiledExpression::parse_bounded("2.7 g / cm^3", &strict).is_err());
//! ```

mod expression;
mod limits;
mod namespace;
pub mod quantity;
mod rewrite;
mod system;
mod variable;

pub use expression::{
    CompiledExpression, ExprEvalError, ExprParsingError, ExprParsingErrorKind, SourceSpan,
};
pub use limits::{LimitError, LimitKind, Limits};
pub use namespace::{FQName, IntoName, InvalidName, Name, Namespace};
pub use quantity::{
    Dimension, Quantity, QuantityError, UNITS, Unit, UnitError, canonical_for, lookup,
};
pub use rewrite::{rewrite_symbols, rewrite_symbols_bounded};
pub use system::{VariablesError, VariablesSystem};
pub use variable::{VariableId, VariableOptions};
