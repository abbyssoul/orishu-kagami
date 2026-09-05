//! Variables and expressions sub-system, in the idCVarSystem tradition.
//!
//! This crate knows nothing about CAD, solvers, dimensions, units, or
//! documents, and is deliberately kept generic. It lets callers define
//! namespaced variables bound to parsed math expressions, and expressions
//! can reference other variables by their fully qualified `namespace.name`.

mod expression;
mod namespace;
mod system;
mod variable;

pub use expression::{
    CompiledExpression, ExprEvalError, ExprParsingError, ExprParsingErrorKind, SourceSpan,
};
pub use namespace::{FQName, IntoName, InvalidName, Name, Namespace};
pub use system::{VariablesError, VariablesSystem};
pub use variable::{VariableId, VariableOptions};
