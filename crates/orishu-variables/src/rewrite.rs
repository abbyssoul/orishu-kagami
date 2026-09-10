//! Renaming the symbols in an authored expression.
//!
//! Two callers need this and both need it to agree with the parser: renaming
//! a document variable rewrites the expressions that referred to it by name,
//! and catalog instantiation copies definitions into object-local identities.
//! The rewrite is textual because the authored source is what is retained and
//! persisted (ADR 0005), but it is *tokenised* rather than pattern-matched: a
//! naive search-and-replace would corrupt `mass_of_sun` while renaming
//! `mass`, or rewrite the `e30` inside `1.989e30`.
//!
//! It lives beside [`crate::expression`]'s lexer rather than in either
//! consumer for exactly that reason — a symbol starts with a letter or `_`,
//! continues with alphanumerics and `_`, and a `.` continues it only when
//! another identifier character follows. A copy elsewhere would be a second
//! definition of what a symbol is, free to drift from the one that parses.

use std::collections::BTreeMap;

use crate::limits::{LimitError, LimitKind, Limits, ensure};

/// Replace every whole symbol in `source` that appears in `renames`, under
/// [`Limits::DEFAULT`].
///
/// The convenient form of [`rewrite_symbols_bounded`]; see it for the
/// behavior and the bound.
pub fn rewrite_symbols(
    source: &str,
    renames: &BTreeMap<String, String>,
) -> Result<String, LimitError> {
    rewrite_symbols_bounded(source, renames, &Limits::DEFAULT)
}

/// Replace every whole symbol in `source` that appears in `renames`, leaving
/// numbers, operators, and unmatched symbols untouched.
///
/// Renaming is single-pass: a symbol is replaced by its mapped value and the
/// result is never rescanned, so a rename map whose values collide with its
/// keys cannot cascade.
///
/// The result is another expression source, so it is held to
/// [`Limits::max_expression_bytes`] at both ends. The output bound is not
/// implied by the input one: a map from short names to long ones grows what it
/// rewrites, and the growth factor belongs to the caller's rename map rather
/// than to the source. Checking as the output is appended means an oversized
/// rewrite is refused while it is being built rather than after it has been
/// allocated.
pub fn rewrite_symbols_bounded(
    source: &str,
    renames: &BTreeMap<String, String>,
    limits: &Limits,
) -> Result<String, LimitError> {
    ensure(
        LimitKind::ExpressionBytes,
        source.len(),
        limits.max_expression_bytes,
    )?;
    if renames.is_empty() {
        return Ok(source.to_owned());
    }
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut position = 0usize;

    while position < bytes.len() {
        let current = bytes[position] as char;
        if current.is_ascii_digit() {
            let end = scan_number(bytes, position);
            push(&mut out, &source[position..end], limits)?;
            position = end;
        } else if current.is_alphabetic() || current == '_' {
            let end = scan_symbol(bytes, position);
            let symbol = &source[position..end];
            match renames.get(symbol) {
                Some(replacement) => push(&mut out, replacement, limits)?,
                None => push(&mut out, symbol, limits)?,
            }
            position = end;
        } else {
            push(
                &mut out,
                &source[position..position + current.len_utf8()],
                limits,
            )?;
            position += current.len_utf8();
        }
    }
    Ok(out)
}

/// Append `fragment`, refusing before the output passes the byte bound.
fn push(out: &mut String, fragment: &str, limits: &Limits) -> Result<(), LimitError> {
    let Some(length) = out.len().checked_add(fragment.len()) else {
        return Err(LimitError {
            limit: LimitKind::ExpressionBytes,
            found: u64::MAX,
            allowed: limits.max_expression_bytes as u64,
        });
    };
    ensure(
        LimitKind::ExpressionBytes,
        length,
        limits.max_expression_bytes,
    )?;
    out.push_str(fragment);
    Ok(())
}

/// Consume a numeric literal, including a `.` fraction and an `e`/`E`
/// exponent, so `1.989e30` is never mistaken for the symbol `e30`.
fn scan_number(bytes: &[u8], start: usize) -> usize {
    let mut position = start;
    while position < bytes.len() && (bytes[position] as char).is_ascii_digit() {
        position += 1;
    }
    if position < bytes.len() && bytes[position] as char == '.' {
        position += 1;
        while position < bytes.len() && (bytes[position] as char).is_ascii_digit() {
            position += 1;
        }
    }
    if position < bytes.len() && matches!(bytes[position] as char, 'e' | 'E') {
        let exponent = position;
        position += 1;
        if position < bytes.len() && matches!(bytes[position] as char, '+' | '-') {
            position += 1;
        }
        if position < bytes.len() && (bytes[position] as char).is_ascii_digit() {
            while position < bytes.len() && (bytes[position] as char).is_ascii_digit() {
                position += 1;
            }
        } else {
            // Not an exponent after all: `1e` is the literal `1` followed by
            // the symbol `e`, exactly as the lexer reads it.
            position = exponent;
        }
    }
    position
}

/// Consume a possibly dotted symbol.
fn scan_symbol(bytes: &[u8], start: usize) -> usize {
    let mut position = start + 1;
    while position < bytes.len() {
        let current = bytes[position] as char;
        let dot_continues = current == '.'
            && position + 1 < bytes.len()
            && ((bytes[position + 1] as char).is_alphanumeric()
                || bytes[position + 1] as char == '_');
        if current.is_alphanumeric() || current == '_' || dot_continues {
            position += 1;
        } else {
            break;
        }
    }
    position
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompiledExpression;

    fn renames(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(from, to)| ((*from).to_owned(), (*to).to_owned()))
            .collect()
    }

    /// Rewrite under the default bounds, which every behavioral case here is
    /// far below.
    fn rewrite(source: &str, renames: &BTreeMap<String, String>) -> String {
        rewrite_symbols(source, renames).expect("within the default bounds")
    }

    #[test]
    fn an_empty_rename_map_returns_the_source_unchanged() {
        assert_eq!(rewrite("a + b", &BTreeMap::new()), "a + b");
    }

    #[test]
    fn a_qualified_symbol_is_replaced_whole() {
        assert_eq!(
            rewrite(
                "planets.sun.mass / 2",
                &renames(&[("planets.sun.mass", "objects.o.sun_mass")])
            ),
            "objects.o.sun_mass / 2"
        );
    }

    #[test]
    fn a_symbol_that_merely_contains_the_renamed_text_is_left_alone() {
        assert_eq!(
            rewrite("mass_of_sun + mass", &renames(&[("mass", "m2")])),
            "mass_of_sun + m2"
        );
    }

    #[test]
    fn a_prefix_of_a_longer_qualified_name_is_left_alone() {
        assert_eq!(
            rewrite(
                "planets.sun.mass",
                &renames(&[("planets.sun", "objects.o")])
            ),
            "planets.sun.mass"
        );
    }

    #[test]
    fn an_exponent_is_never_mistaken_for_a_symbol() {
        assert_eq!(
            rewrite("1.989e30 * e", &renames(&[("e", "objects.o.e")])),
            "1.989e30 * objects.o.e"
        );
        assert_eq!(
            rewrite("1e+5 - 2E-3", &renames(&[("e", "x"), ("E", "y")])),
            "1e+5 - 2E-3"
        );
    }

    #[test]
    fn a_bare_e_after_a_number_is_a_symbol_just_as_the_lexer_reads_it() {
        assert_eq!(rewrite("1e", &renames(&[("e", "x")])), "1x");
    }

    #[test]
    fn operators_parentheses_and_spacing_survive_verbatim() {
        assert_eq!(
            rewrite("-(a ^ 2) / (b + 3.5)", &renames(&[("a", "x"), ("b", "y")])),
            "-(x ^ 2) / (y + 3.5)"
        );
    }

    #[test]
    fn renaming_does_not_cascade_through_its_own_output() {
        assert_eq!(
            rewrite("a + b", &renames(&[("a", "b"), ("b", "a")])),
            "b + a"
        );
    }

    #[test]
    fn an_oversized_source_is_refused_before_it_is_scanned() {
        let limits = Limits {
            max_expression_bytes: 4,
            ..Limits::DEFAULT
        };
        assert_eq!(
            rewrite_symbols_bounded("a + b", &renames(&[("a", "x")]), &limits),
            Err(LimitError {
                limit: LimitKind::ExpressionBytes,
                found: 5,
                allowed: 4,
            })
        );
        assert_eq!(
            rewrite_symbols_bounded("a+ b", &renames(&[("a", "x")]), &limits),
            Ok("x+ b".to_owned())
        );
    }

    #[test]
    fn a_rewrite_that_grows_past_the_bound_is_refused_as_it_is_built() {
        // The input fits; the *output* does not, because the rename map maps
        // a short name to a long one. Nothing about the source says so.
        let limits = Limits {
            max_expression_bytes: 6,
            ..Limits::DEFAULT
        };
        assert_eq!(
            rewrite_symbols_bounded("a + b", &renames(&[("a", "xyz")]), &limits),
            Err(LimitError {
                limit: LimitKind::ExpressionBytes,
                found: 7,
                allowed: 6,
            })
        );
        // One byte shorter, and the same rewrite is accepted.
        assert_eq!(
            rewrite_symbols_bounded("a + b", &renames(&[("a", "xy")]), &limits),
            Ok("xy + b".to_owned())
        );
    }

    #[test]
    fn a_rewritten_expression_parses_to_the_renamed_symbols() {
        let rewritten = rewrite(
            "planets.sun.solar_mass * planets.sun.scale + 1.0e3",
            &renames(&[
                ("planets.sun.solar_mass", "objects.o.solar_mass"),
                ("planets.sun.scale", "objects.o.scale"),
            ]),
        );
        let compiled = CompiledExpression::parse(&rewritten).unwrap();
        assert_eq!(
            compiled.variables(),
            vec![
                "objects.o.solar_mass".to_owned(),
                "objects.o.scale".to_owned()
            ]
        );
    }
}
