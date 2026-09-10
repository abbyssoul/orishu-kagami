use thiserror::Error;

use crate::limits::{Budget, LimitError, LimitKind, Limits, admit_one, ensure};
use crate::quantity::{Dimension, Quantity, QuantityError};

/// A half-open byte range into the source text an error or token came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// What went wrong while parsing an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprParsingErrorKind {
    /// A character sequence that does not fit the grammar.
    Syntax,
    /// The source ended while a construct (e.g. a parenthesized group) was
    /// still open.
    UnexpectedEnd,
    /// A declared bound refused the source before it was fully read.
    ///
    /// Carried by a parsing error rather than surfaced on its own so that the
    /// refusal keeps a [`SourceSpan`]: the bytes past the byte budget, or the
    /// token whose node or nesting level would have been one too many.
    LimitExceeded(LimitError),
}

/// A parse failure, with the byte span in the source it occurred at.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct ExprParsingError {
    pub kind: ExprParsingErrorKind,
    pub message: String,
    pub span: SourceSpan,
}

impl ExprParsingError {
    /// Report a bound that refused the source, at the span it refused.
    ///
    /// Cold and outlined: it allocates a message, and it sits behind checks
    /// run once per node that are otherwise a compare and a branch.
    #[cold]
    #[inline(never)]
    fn limit(error: LimitError, span: SourceSpan) -> Self {
        Self {
            kind: ExprParsingErrorKind::LimitExceeded(error),
            message: error.to_string(),
            span,
        }
    }

    /// The bound this failure exceeded, if it was a resource refusal rather
    /// than a syntax error.
    pub fn limit_error(&self) -> Option<LimitError> {
        match self.kind {
            ExprParsingErrorKind::LimitExceeded(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f64),
    Symbol(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    kind: TokenKind,
    span: SourceSpan,
}

/// Tokenizes source text on demand, one token per `next()` call, instead of
/// materializing the whole token stream up front.
struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    fn bytes(&self) -> &'a [u8] {
        self.source.as_bytes()
    }

    fn lex_number(&mut self) -> Result<Token, ExprParsingError> {
        let bytes = self.bytes();
        let start = self.pos;
        while self.pos < bytes.len() && (bytes[self.pos] as char).is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos < bytes.len() && bytes[self.pos] as char == '.' {
            self.pos += 1;
            while self.pos < bytes.len() && (bytes[self.pos] as char).is_ascii_digit() {
                self.pos += 1;
            }
        }
        if self.pos < bytes.len() && matches!(bytes[self.pos] as char, 'e' | 'E') {
            let exp_start = self.pos;
            self.pos += 1;
            if self.pos < bytes.len() && matches!(bytes[self.pos] as char, '+' | '-') {
                self.pos += 1;
            }
            if self.pos < bytes.len() && (bytes[self.pos] as char).is_ascii_digit() {
                while self.pos < bytes.len() && (bytes[self.pos] as char).is_ascii_digit() {
                    self.pos += 1;
                }
            } else {
                self.pos = exp_start;
            }
        }
        let text = &self.source[start..self.pos];
        let value = text.parse::<f64>().map_err(|_| ExprParsingError {
            kind: ExprParsingErrorKind::Syntax,
            message: format!("`{text}` is not a valid number"),
            span: SourceSpan::new(start, self.pos),
        })?;
        Ok(Token {
            kind: TokenKind::Number(value),
            span: SourceSpan::new(start, self.pos),
        })
    }

    fn lex_symbol(&mut self) -> Token {
        let bytes = self.bytes();
        let start = self.pos;
        self.pos += 1;
        while self.pos < bytes.len() {
            let c = bytes[self.pos] as char;
            let dot_continues_identifier = c == '.'
                && self.pos + 1 < bytes.len()
                && ((bytes[self.pos + 1] as char).is_alphanumeric()
                    || bytes[self.pos + 1] as char == '_');
            if c.is_alphanumeric() || c == '_' || dot_continues_identifier {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = self.source[start..self.pos].to_owned();
        Token {
            kind: TokenKind::Symbol(text),
            span: SourceSpan::new(start, self.pos),
        }
    }

    fn lex_operator(&mut self) -> Result<Token, ExprParsingError> {
        let start = self.pos;
        let ch = self.bytes()[self.pos] as char;
        let kind = match ch {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '^' => TokenKind::Caret,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            other => {
                return Err(ExprParsingError {
                    kind: ExprParsingErrorKind::Syntax,
                    message: format!("unexpected character `{other}`"),
                    span: SourceSpan::new(start, start + 1),
                });
            }
        };
        self.pos += 1;
        Ok(Token {
            kind,
            span: SourceSpan::new(start, self.pos),
        })
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token, ExprParsingError>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.bytes();
        while self.pos < bytes.len() && (bytes[self.pos] as char).is_whitespace() {
            self.pos += 1;
        }
        if self.pos >= bytes.len() {
            return None;
        }

        let ch = bytes[self.pos] as char;
        if ch.is_ascii_digit() {
            Some(self.lex_number())
        } else if ch.is_alphabetic() || ch == '_' {
            Some(Ok(self.lex_symbol()))
        } else {
            Some(self.lex_operator())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr {
    Literal(f64),
    Symbol(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

impl Expr {
    fn collect_symbols(&self, out: &mut Vec<String>) {
        match self {
            Expr::Literal(_) => {}
            Expr::Symbol(name) => {
                if !out.iter().any(|s| s == name) {
                    out.push(name.clone());
                }
            }
            Expr::Unary { expr, .. } => expr.collect_symbols(out),
            Expr::Binary { lhs, rhs, .. } => {
                lhs.collect_symbols(out);
                rhs.collect_symbols(out);
            }
        }
    }

    /// Pushes every symbol reference in this expression into `out`, borrowed
    /// from the AST rather than cloned, and *with* duplicates — the sibling
    /// [`Self::collect_symbols`] serves the public
    /// [`CompiledExpression::variables`] contract (owned, deduplicated),
    /// while this one feeds dependency resolution, where the caller already
    /// keys visited variables by handle and a repeat is a cheap map hit.
    pub(crate) fn collect_symbol_refs<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Expr::Literal(_) => {}
            Expr::Symbol(name) => out.push(name),
            Expr::Unary { expr, .. } => expr.collect_symbol_refs(out),
            Expr::Binary { lhs, rhs, .. } => {
                lhs.collect_symbol_refs(out);
                rhs.collect_symbol_refs(out);
            }
        }
    }

    fn is_const(&self) -> bool {
        match self {
            Expr::Literal(_) => true,
            // A unit is part of the language, not a value someone else has to
            // supply: `2.7 g` is as computable standalone as `2.7`. Treating
            // it as a reference would make every unit-bearing literal look
            // like it depends on an external definition.
            Expr::Symbol(name) => crate::quantity::lookup(name).is_ok(),
            Expr::Unary { expr, .. } => expr.is_const(),
            Expr::Binary { lhs, rhs, .. } => lhs.is_const() && rhs.is_const(),
        }
    }
}

/// A parsed sub-expression and the depth of the tree it produced, where a leaf
/// is 1. Carried out of every parse function so that a left-associative chain,
/// which the Pratt loop folds without recursing, is still measured.
type Parsed = (Expr, usize);

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    source: &'a str,
    limits: &'a Limits,
    /// Nodes admitted so far, checked before each further node is built.
    nodes: usize,
    /// Live [`Self::parse_expr`] frames, checked on entry so that nesting is
    /// refused before it can exhaust the Rust call stack.
    recursion: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, limits: &'a Limits) -> Result<Self, ExprParsingError> {
        let mut lexer = Lexer::new(source);
        let current = Self::pull(&mut lexer, source.len())?;
        Ok(Self {
            lexer,
            current,
            source,
            limits,
            nodes: 0,
            recursion: 0,
        })
    }

    /// Account for one more node and compute its depth from its deepest child
    /// (0 for a leaf), refusing *before* the node is built.
    ///
    /// Run once per node, so it is inlined; both counters are plain integers
    /// and the refusal behind them is outlined.
    #[inline]
    fn admit_node(
        &mut self,
        child_depth: usize,
        span: SourceSpan,
    ) -> Result<usize, ExprParsingError> {
        self.nodes = admit_one(
            LimitKind::ExpressionNodes,
            self.nodes,
            self.limits.max_expression_nodes,
        )
        .map_err(|error| ExprParsingError::limit(error, span))?;
        admit_one(
            LimitKind::ParseDepth,
            child_depth,
            self.limits.max_parse_depth,
        )
        .map_err(|error| ExprParsingError::limit(error, span))
    }

    /// Enter one level of parser recursion.
    #[inline]
    fn enter(&mut self) -> Result<(), ExprParsingError> {
        self.recursion = admit_one(
            LimitKind::ParseDepth,
            self.recursion,
            self.limits.max_parse_depth,
        )
        .map_err(|error| ExprParsingError::limit(error, self.peek().span))?;
        Ok(())
    }

    /// Leave the level [`Self::enter`] took.
    #[inline]
    fn leave(&mut self) {
        self.recursion -= 1;
    }

    fn pull(lexer: &mut Lexer<'a>, source_len: usize) -> Result<Token, ExprParsingError> {
        match lexer.next() {
            Some(result) => result,
            None => Ok(Token {
                kind: TokenKind::Eof,
                span: SourceSpan::new(source_len, source_len),
            }),
        }
    }

    fn peek(&self) -> &Token {
        &self.current
    }

    fn advance(&mut self) -> Result<Token, ExprParsingError> {
        let next = Self::pull(&mut self.lexer, self.source.len())?;
        Ok(std::mem::replace(&mut self.current, next))
    }

    fn unexpected_end(&self) -> ExprParsingError {
        ExprParsingError {
            kind: ExprParsingErrorKind::UnexpectedEnd,
            message: "unexpected end of expression".to_owned(),
            span: self.peek().span,
        }
    }

    /// Parse an expression, counting the recursion this level costs.
    ///
    /// Every path back into the parser goes through here — a parenthesized
    /// group, a unary chain, a right-associative exponent, a unit
    /// juxtaposition — so bounding entries here bounds the whole descent.
    fn parse_expr(&mut self, min_bp: u8) -> Result<Parsed, ExprParsingError> {
        self.enter()?;
        let result = self.parse_expr_inner(min_bp);
        self.leave();
        result
    }

    fn parse_expr_inner(&mut self, min_bp: u8) -> Result<Parsed, ExprParsingError> {
        let (mut lhs, mut lhs_depth) = self.parse_prefix()?;

        loop {
            let (op, bp, right_assoc) = match &self.peek().kind {
                TokenKind::Plus => (BinaryOp::Add, 1, false),
                TokenKind::Minus => (BinaryOp::Sub, 1, false),
                TokenKind::Star => (BinaryOp::Mul, 2, false),
                TokenKind::Slash => (BinaryOp::Div, 2, false),
                TokenKind::Caret => (BinaryOp::Pow, 4, true),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            let op_span = self.peek().span;
            self.advance()?;
            let next_min_bp = if right_assoc { bp } else { bp + 1 };
            let (rhs, rhs_depth) = self.parse_expr(next_min_bp)?;
            // This fold is where a left-associative chain gets deep without
            // the parser recursing, so the node it builds is measured here.
            let depth = self.admit_node(lhs_depth.max(rhs_depth), op_span)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
            lhs_depth = depth;
        }

        Ok((lhs, lhs_depth))
    }

    /// Turn `1e32 kg` and `2.7 g` into an ordinary multiplication.
    ///
    /// Writing a unit after a magnitude is how people write quantities, and
    /// treating the juxtaposition as a product is what lets one grammar carry
    /// units without a second, unit-aware parser: `kg` is a symbol like any
    /// other, and the evaluator resolves it to a quantity.
    ///
    /// Deliberately the *only* implicit product in the grammar. `a b` stays a
    /// syntax error, because two adjacent names are far more likely to be a
    /// typo than an intended product, and the reading of `2 m` is not in
    /// doubt.
    ///
    /// The right operand is parsed at the exponent's binding power, so
    /// `2.7 g / cm^3` groups as `(2.7 * g) / (cm^3)` and `2 m^2` as
    /// `2 * (m^2)` rather than `(2 * m)^2`.
    fn parse_unit_annotation(
        &mut self,
        magnitude: f64,
        magnitude_depth: usize,
    ) -> Result<Parsed, ExprParsingError> {
        if !matches!(self.peek().kind, TokenKind::Symbol(_)) {
            return Ok((Expr::Literal(magnitude), magnitude_depth));
        }
        let unit_span = self.peek().span;
        let (unit, unit_depth) = self.parse_expr(4)?;
        let depth = self.admit_node(magnitude_depth.max(unit_depth), unit_span)?;
        Ok((
            Expr::Binary {
                op: BinaryOp::Mul,
                lhs: Box::new(Expr::Literal(magnitude)),
                rhs: Box::new(unit),
            },
            depth,
        ))
    }

    fn parse_prefix(&mut self) -> Result<Parsed, ExprParsingError> {
        if matches!(self.peek().kind, TokenKind::Minus) {
            let minus_span = self.peek().span;
            self.advance()?;
            let (expr, expr_depth) = self.parse_expr(3)?;
            let depth = self.admit_node(expr_depth, minus_span)?;
            return Ok((
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                },
                depth,
            ));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Parsed, ExprParsingError> {
        let token = self.advance()?;
        match token.kind {
            TokenKind::Number(value) => {
                let depth = self.admit_node(0, token.span)?;
                self.parse_unit_annotation(value, depth)
            }
            TokenKind::Symbol(name) => {
                let depth = self.admit_node(0, token.span)?;
                Ok((Expr::Symbol(name), depth))
            }
            TokenKind::LParen => {
                let inner = self.parse_expr(0)?;
                match self.peek().kind {
                    TokenKind::RParen => {
                        self.advance()?;
                        Ok(inner)
                    }
                    _ => Err(self.unexpected_end()),
                }
            }
            TokenKind::Eof => Err(self.unexpected_end()),
            _ => Err(ExprParsingError {
                kind: ExprParsingErrorKind::Syntax,
                message: format!(
                    "unexpected token at `{}`",
                    &self.source[token.span.start..token.span.end.max(token.span.start)]
                ),
                span: token.span,
            }),
        }
    }
}

/// A parsed expression, retaining its authored source text.
///
/// Cannot exist without having passed a [`Limits`] check: the node count and
/// depth it records are what a [`crate::VariablesSystem`] re-checks against
/// *its* bounds before adopting an expression parsed elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledExpression {
    source: String,
    pub(crate) ast: Expr,
    nodes: usize,
    depth: usize,
}

impl CompiledExpression {
    /// Parse a math expression from its authored source text, under
    /// [`Limits::DEFAULT`].
    ///
    /// The convenient form. Use [`Self::parse_bounded`] where the caller needs
    /// bounds of its own — a stricter externally-facing entry point, or a
    /// deliberately wider batch import.
    pub fn parse(source: &str) -> Result<Self, ExprParsingError> {
        Self::parse_bounded(source, &Limits::DEFAULT)
    }

    /// Parse a math expression under explicit bounds.
    ///
    /// The source length is checked before the text is lexed or retained, so
    /// an oversized expression is refused without being copied; the node count
    /// and depth are charged as the tree is built, so an expression is refused
    /// before its nodes are allocated rather than after.
    pub fn parse_bounded(source: &str, limits: &Limits) -> Result<Self, ExprParsingError> {
        if let Err(error) = ensure(
            LimitKind::ExpressionBytes,
            source.len(),
            limits.max_expression_bytes,
        ) {
            return Err(ExprParsingError::limit(
                error,
                SourceSpan::new(limits.max_expression_bytes, source.len()),
            ));
        }
        let mut parser = Parser::new(source, limits)?;
        let (ast, depth) = parser.parse_expr(0)?;
        match parser.peek().kind {
            TokenKind::Eof => Ok(CompiledExpression {
                source: source.to_owned(),
                ast,
                nodes: parser.nodes,
                depth,
            }),
            _ => Err(ExprParsingError {
                kind: ExprParsingErrorKind::Syntax,
                message: "trailing input after expression".to_owned(),
                span: parser.peek().span,
            }),
        }
    }

    /// The authored source text this expression was parsed from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// How many syntax-tree nodes this expression holds.
    ///
    /// The cost a consumer prices against
    /// [`Limits::max_expression_nodes`] — retained from the parse, so
    /// re-checking an expression never re-walks it.
    #[inline]
    pub fn nodes(&self) -> usize {
        self.nodes
    }

    /// How deep this expression's syntax tree is, a leaf being 1.
    ///
    /// The cost a consumer prices against [`Limits::max_parse_depth`], and the
    /// bound on any recursive walk of the tree.
    #[inline]
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// `true` when the expression references no symbols and can be computed
    /// standalone, without any external values.
    pub fn is_const(&self) -> bool {
        self.ast.is_const()
    }

    /// The raw dotted symbol references the expression depends on, in the
    /// order first encountered, each listed once.
    pub fn variables(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.ast.collect_symbols(&mut out);
        out
    }
}

/// What went wrong while evaluating an already-parsed expression.
///
/// Not `Eq`: [`Self::FractionalExponent`] reports the exponent the author
/// wrote, and a float has no equivalence relation worth deriving.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ExprEvalError {
    /// A referenced symbol names neither a variable nor a known unit.
    #[error("unknown variable `{0}`")]
    UnknownVariable(String),
    /// The variable's dependency graph, walked lazily, referenced itself.
    #[error("cyclic variable reference detected")]
    Cycle,
    /// A `/` had a zero right-hand side.
    #[error("division by zero")]
    DivisionByZero,
    /// The computed value was not finite (e.g. overflow).
    #[error("expression evaluated to a non-finite value")]
    NonFinite,
    /// Addition or subtraction combined quantities measuring different
    /// things.
    ///
    /// The check the whole value layer exists for: `1 kg + 1 m` is not a
    /// number that happens to be wrong, it is not a quantity at all.
    #[error("cannot add or subtract {left} and {right}")]
    DimensionMismatch {
        /// The left operand's dimension.
        left: Dimension,
        /// The right operand's dimension.
        right: Dimension,
    },
    /// A dimensioned base was raised to a power that is not a whole number.
    ///
    /// `m^0.5` would be a fractional base exponent, which
    /// [`Dimension`] cannot represent and which no schema in this product
    /// declares. A *dimensionless* base may be raised to any power.
    #[error("a dimensioned value can only be raised to a whole-number power, not {0}")]
    FractionalExponent(f64),
    /// An exponent carried a dimension of its own.
    #[error("an exponent must be a pure number, not {0}")]
    DimensionedExponent(Dimension),
    /// Combining dimensions left the representable exponent range.
    #[error("dimension exponent is out of range")]
    DimensionOverflow,
    /// A declared bound refused the evaluation.
    ///
    /// The dependency chain was deeper, or the whole evaluation more work,
    /// than the caller's [`Limits`] allow. Neither belongs to a span in the
    /// source, because neither is a property of one expression.
    #[error(transparent)]
    Limit(#[from] LimitError),
}

impl From<QuantityError> for ExprEvalError {
    fn from(value: QuantityError) -> Self {
        match value {
            QuantityError::NonFinite => Self::NonFinite,
            QuantityError::DimensionMismatch { expected, found } => Self::DimensionMismatch {
                left: expected,
                right: found,
            },
            QuantityError::DimensionOverflow => Self::DimensionOverflow,
        }
    }
}

/// Evaluate `ast`, charging one unit of `budget` per node before walking it.
///
/// Charging *before* recursing is what makes the budget a bound on the work
/// this call performs rather than a report on work it already did. Recursion
/// is bounded by the tree's depth, which [`Limits::max_parse_depth`] fixed when
/// the expression was parsed.
pub(crate) fn eval_ast(
    ast: &Expr,
    resolve: &mut dyn FnMut(&str) -> Result<Quantity, ExprEvalError>,
    budget: &Budget,
) -> Result<Quantity, ExprEvalError> {
    budget.spend(1)?;
    match ast {
        Expr::Literal(value) => Ok(Quantity::dimensionless(*value)?),
        Expr::Symbol(name) => resolve(name),
        Expr::Unary { op, expr } => {
            let value = eval_ast(expr, resolve, budget)?;
            Ok(match op {
                UnaryOp::Neg => Quantity::new(-value.magnitude(), value.dimension())?,
            })
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = eval_ast(lhs, resolve, budget)?;
            let rhs = eval_ast(rhs, resolve, budget)?;
            match op {
                BinaryOp::Add | BinaryOp::Sub => {
                    if lhs.dimension() != rhs.dimension() {
                        return Err(ExprEvalError::DimensionMismatch {
                            left: lhs.dimension(),
                            right: rhs.dimension(),
                        });
                    }
                    let magnitude = match op {
                        BinaryOp::Add => lhs.magnitude() + rhs.magnitude(),
                        _ => lhs.magnitude() - rhs.magnitude(),
                    };
                    Ok(Quantity::new(magnitude, lhs.dimension())?)
                }
                BinaryOp::Mul => {
                    let dimension = lhs
                        .dimension()
                        .multiply(rhs.dimension())
                        .ok_or(ExprEvalError::DimensionOverflow)?;
                    Ok(Quantity::new(lhs.magnitude() * rhs.magnitude(), dimension)?)
                }
                BinaryOp::Div => {
                    if rhs.magnitude() == 0.0 {
                        return Err(ExprEvalError::DivisionByZero);
                    }
                    let dimension = lhs
                        .dimension()
                        .divide(rhs.dimension())
                        .ok_or(ExprEvalError::DimensionOverflow)?;
                    Ok(Quantity::new(lhs.magnitude() / rhs.magnitude(), dimension)?)
                }
                BinaryOp::Pow => eval_pow(lhs, rhs),
            }
        }
    }
}

/// Raise `base` to `exponent`, deriving the resulting dimension.
///
/// An exponent is always a pure number. A *dimensioned* base additionally
/// needs a whole-number exponent, because a dimension is a vector of integer
/// base exponents and `m^0.5` names nothing this product can represent.
fn eval_pow(base: Quantity, exponent: Quantity) -> Result<Quantity, ExprEvalError> {
    if !exponent.is_dimensionless() {
        return Err(ExprEvalError::DimensionedExponent(exponent.dimension()));
    }
    let power = exponent.magnitude();
    let magnitude = base.magnitude().powf(power);

    if base.is_dimensionless() {
        return Ok(Quantity::dimensionless(magnitude)?);
    }
    if power.fract() != 0.0 {
        return Err(ExprEvalError::FractionalExponent(power));
    }
    let whole = i8::try_from(power as i64).map_err(|_| ExprEvalError::DimensionOverflow)?;
    let dimension = base
        .dimension()
        .power(whole)
        .ok_or(ExprEvalError::DimensionOverflow)?;
    Ok(Quantity::new(magnitude, dimension)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Evaluate an expression that references no symbols.
    fn eval_const(expr: &CompiledExpression) -> Quantity {
        let budget = Budget::new(Limits::DEFAULT.max_evaluation_work);
        eval_ast(&expr.ast, &mut |_| unreachable!("no symbols"), &budget).expect("evaluates")
    }

    /// The bound a parse refused, or `None` if it failed for another reason.
    fn refused(result: Result<CompiledExpression, ExprParsingError>) -> Option<LimitError> {
        result.err().and_then(|error| error.limit_error())
    }

    #[test]
    fn parse_integer_and_float_literals() {
        assert!(CompiledExpression::parse("0").unwrap().is_const());
        assert!(CompiledExpression::parse("3.1415").unwrap().is_const());
    }

    #[test]
    fn parse_scientific_notation_literal() {
        let expr = CompiledExpression::parse("3.1e-3").unwrap();
        assert!(expr.is_const());
        assert_eq!(eval_const(&expr).magnitude(), 3.1e-3);
    }

    #[test]
    fn parse_arithmetic_respects_precedence_and_parens() {
        let expr = CompiledExpression::parse("(21 - 1) * (0.15 + 0.1) + 7.3e2").unwrap();
        let value = eval_const(&expr).magnitude();
        assert!((value - (20.0 * 0.25 + 730.0)).abs() < 1e-9);
    }

    #[test]
    fn parse_identifier_as_bare_symbol_reference() {
        let expr = CompiledExpression::parse("a").unwrap();
        assert!(!expr.is_const());
        assert_eq!(expr.variables(), vec!["a".to_owned()]);
    }

    #[test]
    fn parse_dotted_qualified_symbol_reference() {
        let expr = CompiledExpression::parse("globals.electricity.K + 1").unwrap();
        assert_eq!(expr.variables(), vec!["globals.electricity.K".to_owned()]);
    }

    #[test]
    fn parse_rejects_malformed_syntax_with_span() {
        assert!(CompiledExpression::parse("2+").is_err());
        assert!(CompiledExpression::parse("(1+2").is_err());
    }

    #[test]
    fn is_const_true_for_literal_only_expressions() {
        assert!(CompiledExpression::parse("2+2").unwrap().is_const());
        assert!(CompiledExpression::parse("5/3 + 2").unwrap().is_const());
    }

    #[test]
    fn is_const_false_when_expression_references_a_symbol() {
        assert!(!CompiledExpression::parse("a * 3.1e-3").unwrap().is_const());
        assert!(
            !CompiledExpression::parse("3.1415*b + 21*a")
                .unwrap()
                .is_const()
        );
        assert!(
            !CompiledExpression::parse("global.value1 + 2/(local.value_j - 1/2.81)^2")
                .unwrap()
                .is_const()
        );
    }

    #[test]
    fn a_leaf_is_one_node_one_deep() {
        let expr = CompiledExpression::parse("1").unwrap();
        assert_eq!((expr.nodes(), expr.depth()), (1, 1));
    }

    #[test]
    fn node_and_depth_counts_describe_the_tree_that_was_built() {
        // `1+2+3` folds left, so it is three literals and two products of
        // them, two levels deep; `1+(2+3)` is the same size but the same
        // depth, mirrored.
        let flat = CompiledExpression::parse("1+2+3").unwrap();
        assert_eq!((flat.nodes(), flat.depth()), (5, 3));

        // A unit juxtaposition is an ordinary product, so it costs the
        // literal, the symbol, and the multiply.
        let quantity = CompiledExpression::parse("1e32 kg").unwrap();
        assert_eq!((quantity.nodes(), quantity.depth()), (3, 2));

        // Redundant parentheses build nothing.
        let parens = CompiledExpression::parse("((((1))))").unwrap();
        assert_eq!((parens.nodes(), parens.depth()), (1, 1));
    }

    #[test]
    fn source_over_the_byte_bound_is_refused_at_the_first_byte_over() {
        let limits = Limits {
            max_expression_bytes: 4,
            ..Limits::DEFAULT
        };
        assert!(CompiledExpression::parse_bounded("1+1+1", &limits).is_err());
        let error = CompiledExpression::parse_bounded("1+1+1", &limits).unwrap_err();
        assert_eq!(
            error.limit_error(),
            Some(LimitError {
                limit: LimitKind::ExpressionBytes,
                found: 5,
                allowed: 4,
            })
        );
        assert_eq!(error.span, SourceSpan::new(4, 5));
        // Exactly at the bound still parses.
        assert!(CompiledExpression::parse_bounded("1+11", &limits).is_ok());
    }

    #[test]
    fn nesting_is_refused_before_it_can_exhaust_the_call_stack() {
        // Redundant parentheses build a tree of depth 1, so only the parser's
        // own recursion counter can stop this: without it the source below is
        // a stack overflow rather than an error.
        let limits = Limits {
            max_parse_depth: 8,
            ..Limits::DEFAULT
        };
        let nested = |count: usize| format!("{}1{}", "(".repeat(count), ")".repeat(count));
        assert!(CompiledExpression::parse_bounded(&nested(7), &limits).is_ok());
        assert_eq!(
            refused(CompiledExpression::parse_bounded(&nested(8), &limits)).map(|e| e.limit),
            Some(LimitKind::ParseDepth)
        );
        // The default bounds refuse a pathological source rather than crash.
        // 2 000 levels fits the default byte budget, so depth is what stops
        // it — which is the point: bytes alone would not have.
        assert_eq!(
            refused(CompiledExpression::parse(&nested(2_000))).map(|e| e.limit),
            Some(LimitKind::ParseDepth)
        );
    }

    #[test]
    fn a_left_deep_chain_is_measured_even_though_the_parser_does_not_recurse() {
        // `1+1+…` folds in the Pratt loop, so parser recursion stays at 2. The
        // tree it builds is as deep as the chain is long, and every later walk
        // of that tree recurses once per level.
        let limits = Limits {
            max_parse_depth: 4,
            ..Limits::DEFAULT
        };
        let chain = |terms: usize| vec!["1"; terms].join("+");
        assert!(CompiledExpression::parse_bounded(&chain(4), &limits).is_ok());
        assert_eq!(
            refused(CompiledExpression::parse_bounded(&chain(5), &limits)).map(|e| e.limit),
            Some(LimitKind::ParseDepth)
        );
    }

    #[test]
    fn a_unary_chain_and_an_exponent_chain_are_both_bounded() {
        let limits = Limits {
            max_parse_depth: 6,
            ..Limits::DEFAULT
        };
        assert!(CompiledExpression::parse_bounded("-----1", &limits).is_ok());
        assert_eq!(
            refused(CompiledExpression::parse_bounded("------1", &limits)).map(|e| e.limit),
            Some(LimitKind::ParseDepth)
        );
        assert!(CompiledExpression::parse_bounded("2^2^2^2^2^2", &limits).is_ok());
        assert_eq!(
            refused(CompiledExpression::parse_bounded("2^2^2^2^2^2^2", &limits)).map(|e| e.limit),
            Some(LimitKind::ParseDepth)
        );
    }

    #[test]
    fn node_count_is_charged_before_the_node_is_built() {
        // A balanced tree, so the depth bound cannot be what fires.
        let limits = Limits {
            max_expression_nodes: 7,
            ..Limits::DEFAULT
        };
        assert!(CompiledExpression::parse_bounded("(1+1)+(1+1)", &limits).is_ok());
        assert_eq!(
            refused(CompiledExpression::parse_bounded("(1+1)+(1+1)+1", &limits)).map(|e| e.limit),
            Some(LimitKind::ExpressionNodes)
        );
    }

    #[test]
    fn variables_lists_all_referenced_symbol_names_once_each() {
        let expr = CompiledExpression::parse("a + b*a - c").unwrap();
        assert_eq!(
            expr.variables(),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
    }
}
