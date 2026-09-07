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

mod expression;
mod namespace;
pub mod quantity;
mod rewrite;
mod system;
mod variable;

pub use expression::{
    CompiledExpression, ExprEvalError, ExprParsingError, ExprParsingErrorKind, SourceSpan,
};
pub use namespace::{FQName, IntoName, InvalidName, Name, Namespace};
pub use quantity::{
    Dimension, Quantity, QuantityError, UNITS, Unit, UnitError, canonical_for, lookup,
};
pub use rewrite::rewrite_symbols;
pub use system::{VariablesError, VariablesSystem};
pub use variable::{VariableId, VariableOptions};
