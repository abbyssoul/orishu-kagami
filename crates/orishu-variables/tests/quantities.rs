//! Dimensioned evaluation, exercised through the public API.
//!
//! These are S-VARIABLES slice 2's acceptance cases. What they are really
//! asserting is that there is *one* grammar: units are ordinary symbols, a
//! dimension is derived by the arithmetic rather than declared beside it, and
//! a pure number is simply a dimensionless quantity — so nothing here needs a
//! second, unit-aware parser to exist.

use orishu_variables::{
    CompiledExpression, Dimension, ExprEvalError, Namespace, VariableOptions, VariablesError,
    VariablesSystem, lookup,
};

fn eval(source: &str) -> Result<orishu_variables::Quantity, VariablesError> {
    VariablesSystem::default().eval(source)
}

/// Assert a magnitude to within a relative tolerance, because these are
/// floating-point products of several factors.
fn assert_close(actual: f64, expected: f64) {
    let tolerance = expected.abs() * 1e-12;
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn a_unit_bearing_literal_resolves_to_canonical_si() {
    let mass = eval("1e32 kg").expect("a mass");
    assert_eq!(mass.dimension(), Dimension::MASS);
    assert_close(mass.magnitude(), 1e32);

    // The unit scales the magnitude; canonical SI is what is stored.
    let grams = eval("2.7 g").expect("a mass");
    assert_eq!(grams.dimension(), Dimension::MASS);
    assert_close(grams.magnitude(), 2.7e-3);

    let distance = eval("1 AU").expect("a length");
    assert_eq!(distance.dimension(), Dimension::LENGTH);
    assert_close(distance.magnitude(), 1.495_978_707e11);
}

#[test]
fn a_derived_dimension_comes_from_the_arithmetic() {
    // The case the whole value layer exists for: nothing declared this a
    // density.
    let density = eval("2.7 g / cm^3").expect("a density");
    assert_eq!(density.dimension(), Dimension::DENSITY);
    assert_close(density.magnitude(), 2_700.0);

    assert_eq!(
        eval("9.8 m / s^2").expect("g").dimension(),
        Dimension::ACCELERATION
    );
    assert_eq!(
        eval("2 kg * 9.8 m / s^2").expect("a force").dimension(),
        Dimension::FORCE
    );
    assert_eq!(
        eval("100 km / 1 h").expect("a speed").dimension(),
        Dimension::VELOCITY
    );
    assert_close(
        eval("100 km / 1 h").expect("a speed").magnitude(),
        1e5 / 3600.0,
    );
}

#[test]
fn a_pure_number_is_a_dimensionless_quantity() {
    let ratio = eval("1 / 3 + 0.1").expect("a number");
    assert!(ratio.is_dimensionless());
    assert_close(ratio.magnitude(), 1.0 / 3.0 + 0.1);

    // A ratio of like quantities cancels to a pure number.
    let cancelled = eval("2 km / 1 m").expect("a ratio");
    assert!(cancelled.is_dimensionless());
    assert_close(cancelled.magnitude(), 2_000.0);
}

#[test]
fn adding_unlike_quantities_is_refused() {
    let error = eval("1 kg + 1 m").expect_err("a mass is not a length");
    assert_eq!(
        error,
        VariablesError::Eval(ExprEvalError::DimensionMismatch {
            left: Dimension::MASS,
            right: Dimension::LENGTH,
        })
    );
    // The message names both sides in base units, so it is actionable.
    assert_eq!(error.to_string(), "cannot add or subtract kg and m");

    assert!(eval("1 kg - 1 s").is_err());
    // Adding a pure number to a dimensioned one is the same mistake.
    assert!(eval("1 kg + 1").is_err());
    // Like quantities in different units add fine.
    assert_close(eval("1 kg + 1 g").expect("a mass").magnitude(), 1.001);
}

#[test]
fn an_exponent_must_be_a_pure_whole_number_over_a_dimensioned_base() {
    assert!(matches!(
        eval("m^0.5"),
        Err(VariablesError::Eval(ExprEvalError::FractionalExponent(_)))
    ));
    assert!(matches!(
        eval("2^(1 kg)"),
        Err(VariablesError::Eval(ExprEvalError::DimensionedExponent(_)))
    ));
    // A dimensionless base may take any power: this is an ordinary number.
    assert_close(eval("2^0.5").expect("a number").magnitude(), 2f64.sqrt());
    // A negative whole power inverts the dimension.
    assert_eq!(
        eval("s^-1").expect("a frequency").dimension(),
        Dimension::FREQUENCY
    );
}

#[test]
fn a_unit_annotation_binds_tighter_than_the_operators_around_it() {
    // `2.7 g / cm^3` must group as `(2.7 g) / (cm^3)`, and `2 m^2` as
    // `2 * (m^2)` rather than `(2 m)^2` — which would be four square metres.
    let area = eval("2 m^2").expect("an area");
    assert_eq!(area.dimension(), Dimension::LENGTH.power(2).expect("area"));
    assert_close(area.magnitude(), 2.0);
}

#[test]
fn only_a_magnitude_may_be_juxtaposed_with_a_symbol() {
    // Two adjacent names are far more likely to be a typo than a product, so
    // the grammar keeps exactly one implicit multiplication.
    assert!(matches!(eval("kg m"), Err(VariablesError::Parsing(_))));
    assert!(matches!(eval("2 kg 3"), Err(VariablesError::Parsing(_))));
    // Written explicitly, it is fine.
    assert_eq!(
        eval("kg * m").expect("a product").dimension(),
        Dimension::MASS.multiply(Dimension::LENGTH).expect("valid")
    );
}

#[test]
fn a_symbol_resolves_to_a_variable_before_a_unit() {
    let mut system = VariablesSystem::default();
    let sun = system
        .define(
            &Namespace::new("planets"),
            "solar_mass",
            CompiledExpression::parse("1.989e30 kg").expect("parses"),
            VariableOptions::default(),
        )
        .expect("defined");

    // A variable carries its dimension to everything that references it.
    assert_eq!(
        system.value(sun).expect("a mass").dimension(),
        Dimension::MASS
    );
    let half = system.eval("planets.solar_mass / 2").expect("a mass");
    assert_eq!(half.dimension(), Dimension::MASS);
    assert_close(half.magnitude(), 1.989e30 / 2.0);

    // And a unit still resolves in the same expression.
    let ratio = system
        .eval("planets.solar_mass / 1 kg")
        .expect("a pure number");
    assert!(ratio.is_dimensionless());
}

#[test]
fn a_root_variable_may_not_shadow_a_unit_symbol() {
    let mut system = VariablesSystem::default();
    let refused = system
        .define(
            &Namespace::new(""),
            "m",
            CompiledExpression::parse("42").expect("parses"),
            VariableOptions::default(),
        )
        .expect_err("`m` already means metres");
    assert!(matches!(refused, VariablesError::ShadowsUnit { .. }));

    // But a *namespaced* one may: `K` is Coulomb's constant where it belongs,
    // and no expression can confuse it with kelvin, because reaching it means
    // writing `electricity.K`.
    let coulomb = system
        .define(
            &Namespace::new("electricity"),
            "K",
            CompiledExpression::parse("8.99e9").expect("parses"),
            VariableOptions::default(),
        )
        .expect("a namespaced name cannot be confused with a unit");
    assert_close(system.value(coulomb).expect("a value").magnitude(), 8.99e9);
    // A bare `K` in an expression is still kelvin.
    assert_eq!(
        system.eval("1 K").expect("a temperature").dimension(),
        Dimension::TEMPERATURE
    );
}

#[test]
fn an_unknown_symbol_is_neither_a_variable_nor_a_unit() {
    let error = eval("1 furlong").expect_err("not in the unit table");
    assert_eq!(
        error,
        VariablesError::Eval(ExprEvalError::UnknownVariable("furlong".to_owned()))
    );
    assert!(lookup("furlong").is_err());
}

#[test]
fn a_non_finite_or_undefined_result_is_never_a_quantity() {
    assert!(matches!(
        eval("1 kg / 0"),
        Err(VariablesError::Eval(ExprEvalError::DivisionByZero))
    ));
    assert!(matches!(
        eval("1e308 kg * 1e308"),
        Err(VariablesError::Eval(ExprEvalError::NonFinite))
    ));
}

#[test]
fn a_dimension_that_leaves_the_representable_range_is_reported() {
    // Exponents are `i8`; an absurd authored power must report rather than
    // wrap into a plausible-looking dimension.
    assert!(matches!(
        eval("m^100 * m^100"),
        Err(VariablesError::Eval(ExprEvalError::DimensionOverflow))
    ));
}
