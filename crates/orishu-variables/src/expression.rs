use thiserror::Error;

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
}

/// A parse failure, with the byte span in the source it occurred at.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct ExprParsingError {
    pub kind: ExprParsingErrorKind,
    pub message: String,
    pub span: SourceSpan,
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
            Expr::Symbol(_) => false,
            Expr::Unary { expr, .. } => expr.is_const(),
            Expr::Binary { lhs, rhs, .. } => lhs.is_const() && rhs.is_const(),
        }
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    source: &'a str,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Result<Self, ExprParsingError> {
        let mut lexer = Lexer::new(source);
        let current = Self::pull(&mut lexer, source.len())?;
        Ok(Self {
            lexer,
            current,
            source,
        })
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

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ExprParsingError> {
        let mut lhs = self.parse_prefix()?;

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
            self.advance()?;
            let next_min_bp = if right_assoc { bp } else { bp + 1 };
            let rhs = self.parse_expr(next_min_bp)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ExprParsingError> {
        if matches!(self.peek().kind, TokenKind::Minus) {
            self.advance()?;
            let expr = self.parse_expr(3)?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprParsingError> {
        let token = self.advance()?;
        match token.kind {
            TokenKind::Number(value) => Ok(Expr::Literal(value)),
            TokenKind::Symbol(name) => Ok(Expr::Symbol(name)),
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
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledExpression {
    source: String,
    pub(crate) ast: Expr,
}

impl CompiledExpression {
    /// Parse a math expression from its authored source text.
    pub fn parse(source: &str) -> Result<Self, ExprParsingError> {
        let mut parser = Parser::new(source)?;
        let ast = parser.parse_expr(0)?;
        match parser.peek().kind {
            TokenKind::Eof => Ok(CompiledExpression {
                source: source.to_owned(),
                ast,
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
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ExprEvalError {
    /// A referenced symbol has no matching variable.
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
}

pub(crate) fn eval_ast(
    ast: &Expr,
    resolve: &mut dyn FnMut(&str) -> Result<f64, ExprEvalError>,
) -> Result<f64, ExprEvalError> {
    match ast {
        Expr::Literal(value) => Ok(*value),
        Expr::Symbol(name) => resolve(name),
        Expr::Unary { op, expr } => {
            let value = eval_ast(expr, resolve)?;
            Ok(match op {
                UnaryOp::Neg => -value,
            })
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = eval_ast(lhs, resolve)?;
            let rhs = eval_ast(rhs, resolve)?;
            let value = match op {
                BinaryOp::Add => lhs + rhs,
                BinaryOp::Sub => lhs - rhs,
                BinaryOp::Mul => lhs * rhs,
                BinaryOp::Div => {
                    if rhs == 0.0 {
                        return Err(ExprEvalError::DivisionByZero);
                    }
                    lhs / rhs
                }
                BinaryOp::Pow => lhs.powf(rhs),
            };
            if value.is_finite() {
                Ok(value)
            } else {
                Err(ExprEvalError::NonFinite)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integer_and_float_literals() {
        assert!(CompiledExpression::parse("0").unwrap().is_const());
        assert!(CompiledExpression::parse("3.1415").unwrap().is_const());
    }

    #[test]
    fn parse_scientific_notation_literal() {
        let expr = CompiledExpression::parse("3.1e-3").unwrap();
        assert!(expr.is_const());
        let mut resolve = |_: &str| -> Result<f64, ExprEvalError> { unreachable!() };
        assert_eq!(eval_ast(&expr.ast, &mut resolve).unwrap(), 3.1e-3);
    }

    #[test]
    fn parse_arithmetic_respects_precedence_and_parens() {
        let expr = CompiledExpression::parse("(21 - 1) * (0.15 + 0.1) + 7.3e2").unwrap();
        let mut resolve = |_: &str| -> Result<f64, ExprEvalError> { unreachable!() };
        let value = eval_ast(&expr.ast, &mut resolve).unwrap();
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
    fn variables_lists_all_referenced_symbol_names_once_each() {
        let expr = CompiledExpression::parse("a + b*a - c").unwrap();
        assert_eq!(
            expr.variables(),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
    }
}
