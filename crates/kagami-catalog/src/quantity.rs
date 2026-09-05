//! Physical dimensions and the authored-unit table.
//!
//! ADR 0005 requires authored numeric values to be unit-aware and checked
//! against the dimension their schema declares. The shared expression engine
//! (`orishu-variables`) is deliberately dimensionless today; the dimension
//! layer that will eventually sit over it belongs to the shared variables
//! subsystem task, not here. Until it exists, the catalog declares a unit
//! *alongside* each expression and checks that unit's dimension against the
//! property schema, rather than forking the parser into a second, unit-aware
//! grammar. See the crate-level documentation for exactly which checks that
//! does and does not buy.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Number of SI base quantities, in the order used by [`Dimension`].
const BASE_COUNT: usize = 7;

/// Base-unit symbols, in [`Dimension`] exponent order, used to render a
/// dimension for a diagnostic.
const BASE_SYMBOLS: [&str; BASE_COUNT] = ["m", "kg", "s", "A", "K", "mol", "cd"];

/// A physical dimension as SI base-quantity exponents, in the order
/// length, mass, time, electric current, thermodynamic temperature, amount
/// of substance, luminous intensity.
///
/// Exponents are `i8` because no schema in this product needs a base
/// exponent outside `-128..=127`, and arithmetic is checked so an absurd
/// authored power reports an error rather than wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dimension([i8; BASE_COUNT]);

impl Dimension {
    /// A pure number: the dimension of a ratio, a count, or a factor.
    pub const DIMENSIONLESS: Self = Self([0; BASE_COUNT]);
    /// Length, `m`.
    pub const LENGTH: Self = Self([1, 0, 0, 0, 0, 0, 0]);
    /// Mass, `kg`.
    pub const MASS: Self = Self([0, 1, 0, 0, 0, 0, 0]);
    /// Time, `s`.
    pub const TIME: Self = Self([0, 0, 1, 0, 0, 0, 0]);
    /// Electric current, `A`.
    pub const CURRENT: Self = Self([0, 0, 0, 1, 0, 0, 0]);
    /// Thermodynamic temperature, `K`.
    pub const TEMPERATURE: Self = Self([0, 0, 0, 0, 1, 0, 0]);
    /// Amount of substance, `mol`.
    pub const AMOUNT: Self = Self([0, 0, 0, 0, 0, 1, 0]);
    /// Luminous intensity, `cd`.
    pub const LUMINOUS: Self = Self([0, 0, 0, 0, 0, 0, 1]);
    /// Electric charge, `A·s`.
    pub const CHARGE: Self = Self([0, 0, 1, 1, 0, 0, 0]);
    /// Frequency, `s^-1`.
    pub const FREQUENCY: Self = Self([0, 0, -1, 0, 0, 0, 0]);
    /// Velocity, `m·s^-1`.
    pub const VELOCITY: Self = Self([1, 0, -1, 0, 0, 0, 0]);
    /// Acceleration, `m·s^-2`.
    pub const ACCELERATION: Self = Self([1, 0, -2, 0, 0, 0, 0]);
    /// Force, `m·kg·s^-2`.
    pub const FORCE: Self = Self([1, 1, -2, 0, 0, 0, 0]);
    /// Energy, `m^2·kg·s^-2`.
    pub const ENERGY: Self = Self([2, 1, -2, 0, 0, 0, 0]);
    /// Power, `m^2·kg·s^-3`.
    pub const POWER: Self = Self([2, 1, -3, 0, 0, 0, 0]);
    /// Mass density, `m^-3·kg`.
    pub const DENSITY: Self = Self([-3, 1, 0, 0, 0, 0, 0]);

    /// Build a dimension from explicit base exponents.
    pub const fn new(exponents: [i8; BASE_COUNT]) -> Self {
        Self(exponents)
    }

    /// The base exponents, in [`Dimension`]'s documented order.
    pub const fn exponents(&self) -> [i8; BASE_COUNT] {
        self.0
    }

    /// `true` for a pure number.
    pub fn is_dimensionless(&self) -> bool {
        *self == Self::DIMENSIONLESS
    }

    /// The dimension of a product, or `None` if an exponent would overflow.
    pub fn multiply(self, other: Self) -> Option<Self> {
        let mut out = [0i8; BASE_COUNT];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = self.0[index].checked_add(other.0[index])?;
        }
        Some(Self(out))
    }

    /// The dimension of a quotient, or `None` if an exponent would overflow.
    pub fn divide(self, other: Self) -> Option<Self> {
        let mut out = [0i8; BASE_COUNT];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = self.0[index].checked_sub(other.0[index])?;
        }
        Some(Self(out))
    }

    /// The dimension of an integer power, or `None` if an exponent would
    /// overflow.
    pub fn power(self, exponent: i8) -> Option<Self> {
        let mut out = [0i8; BASE_COUNT];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = self.0[index].checked_mul(exponent)?;
        }
        Some(Self(out))
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return formatter.write_str("1");
        }
        let mut first = true;
        for (index, exponent) in self.0.iter().copied().enumerate() {
            if exponent == 0 {
                continue;
            }
            if !first {
                formatter.write_str("·")?;
            }
            first = false;
            formatter.write_str(BASE_SYMBOLS[index])?;
            if exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        Ok(())
    }
}

/// An authored unit: the symbol a user writes, the dimension it carries, and
/// the factor converting a magnitude in this unit to canonical SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    symbol: &'static str,
    dimension: Dimension,
    si_factor: f64,
}

impl Unit {
    /// The symbol as authored.
    pub fn symbol(&self) -> &'static str {
        self.symbol
    }

    /// The dimension this unit measures.
    pub fn dimension(&self) -> Dimension {
        self.dimension
    }

    /// The factor converting a magnitude in this unit to canonical SI.
    pub fn si_factor(&self) -> f64 {
        self.si_factor
    }

    /// `true` when a magnitude in this unit already *is* its canonical SI
    /// magnitude. Catalog bindings are published in canonical SI, so an
    /// expression that references another binding may only be annotated with
    /// a canonical unit — see [`crate::template::QuantityValue`].
    pub fn is_canonical(&self) -> bool {
        self.si_factor == 1.0
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.symbol)
    }
}

/// Why a `unit:` field could not be accepted.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UnitError {
    /// The symbol is not in [`UNITS`].
    #[error("unknown unit `{symbol}`")]
    Unknown { symbol: String },
}

const fn unit(symbol: &'static str, dimension: Dimension, si_factor: f64) -> Unit {
    Unit {
        symbol,
        dimension,
        si_factor,
    }
}

/// Every unit the catalog format accepts.
///
/// Deliberately a closed, small table rather than an open prefix grammar:
/// an unrecognised symbol must be an actionable diagnostic, never a silently
/// fabricated scale factor. Extending it is a normal, reviewable change.
pub const UNITS: &[Unit] = &[
    // Dimensionless.
    unit("1", Dimension::DIMENSIONLESS, 1.0),
    // Length.
    unit("m", Dimension::LENGTH, 1.0),
    unit("km", Dimension::LENGTH, 1.0e3),
    unit("cm", Dimension::LENGTH, 1.0e-2),
    unit("mm", Dimension::LENGTH, 1.0e-3),
    unit("um", Dimension::LENGTH, 1.0e-6),
    unit("nm", Dimension::LENGTH, 1.0e-9),
    unit("pm", Dimension::LENGTH, 1.0e-12),
    unit("fm", Dimension::LENGTH, 1.0e-15),
    unit("AU", Dimension::LENGTH, 1.495_978_707e11),
    // Mass.
    unit("kg", Dimension::MASS, 1.0),
    unit("g", Dimension::MASS, 1.0e-3),
    unit("mg", Dimension::MASS, 1.0e-6),
    unit("t", Dimension::MASS, 1.0e3),
    unit("u", Dimension::MASS, 1.660_539_068_92e-27),
    // Time.
    unit("s", Dimension::TIME, 1.0),
    unit("ms", Dimension::TIME, 1.0e-3),
    unit("us", Dimension::TIME, 1.0e-6),
    unit("ns", Dimension::TIME, 1.0e-9),
    unit("min", Dimension::TIME, 60.0),
    unit("h", Dimension::TIME, 3_600.0),
    unit("d", Dimension::TIME, 86_400.0),
    unit("yr", Dimension::TIME, 31_557_600.0),
    // Remaining base quantities.
    unit("A", Dimension::CURRENT, 1.0),
    unit("K", Dimension::TEMPERATURE, 1.0),
    unit("mol", Dimension::AMOUNT, 1.0),
    unit("cd", Dimension::LUMINOUS, 1.0),
    // Common derived quantities.
    unit("C", Dimension::CHARGE, 1.0),
    unit("Hz", Dimension::FREQUENCY, 1.0),
    unit("N", Dimension::FORCE, 1.0),
    unit("J", Dimension::ENERGY, 1.0),
    unit("eV", Dimension::ENERGY, 1.602_176_634e-19),
    unit("MeV", Dimension::ENERGY, 1.602_176_634e-13),
    unit("W", Dimension::POWER, 1.0),
];

/// Resolve an authored unit symbol.
pub fn lookup(symbol: &str) -> Result<&'static Unit, UnitError> {
    UNITS
        .iter()
        .find(|candidate| candidate.symbol == symbol)
        .ok_or_else(|| UnitError::Unknown {
            symbol: symbol.to_owned(),
        })
}

/// The canonical (factor 1) unit for `dimension`, when the table has one.
///
/// Used to name the unit a value is published in, and to give a dimension
/// mismatch a symbol the user recognises rather than an exponent vector.
pub fn canonical_for(dimension: Dimension) -> Option<&'static Unit> {
    UNITS
        .iter()
        .find(|candidate| candidate.dimension == dimension && candidate.is_canonical())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensionless_renders_as_one() {
        assert_eq!(Dimension::DIMENSIONLESS.to_string(), "1");
    }

    #[test]
    fn dimension_renders_base_symbols_with_exponents() {
        assert_eq!(Dimension::FORCE.to_string(), "m·kg·s^-2");
        assert_eq!(Dimension::MASS.to_string(), "kg");
    }

    #[test]
    fn dimension_arithmetic_follows_exponent_algebra() {
        assert_eq!(
            Dimension::LENGTH.divide(Dimension::TIME),
            Some(Dimension::VELOCITY)
        );
        assert_eq!(
            Dimension::VELOCITY.divide(Dimension::TIME),
            Some(Dimension::ACCELERATION)
        );
        assert_eq!(
            Dimension::MASS.multiply(Dimension::ACCELERATION),
            Some(Dimension::FORCE)
        );
        assert_eq!(
            Dimension::LENGTH.power(3),
            Some(Dimension::new([3, 0, 0, 0, 0, 0, 0]))
        );
        assert_eq!(
            Dimension::MASS.divide(Dimension::LENGTH.power(3).unwrap()),
            Some(Dimension::DENSITY)
        );
    }

    #[test]
    fn dimension_arithmetic_reports_overflow_rather_than_wrapping() {
        let extreme = Dimension::new([120, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extreme.multiply(extreme), None);
        assert_eq!(extreme.power(2), None);
    }

    #[test]
    fn known_units_resolve_to_their_dimension_and_factor() {
        let km = lookup("km").unwrap();
        assert_eq!(km.dimension(), Dimension::LENGTH);
        assert_eq!(km.si_factor(), 1.0e3);
        assert!(!km.is_canonical());
        assert!(lookup("m").unwrap().is_canonical());
    }

    #[test]
    fn unknown_unit_is_an_actionable_error_not_a_fabricated_factor() {
        assert_eq!(
            lookup("furlong"),
            Err(UnitError::Unknown {
                symbol: "furlong".to_owned()
            })
        );
    }

    #[test]
    fn every_tabled_unit_has_a_finite_positive_factor_and_unique_symbol() {
        let mut seen = std::collections::BTreeSet::new();
        for unit in UNITS {
            assert!(
                unit.si_factor().is_finite() && unit.si_factor() > 0.0,
                "unit {} has a non-physical factor",
                unit.symbol()
            );
            assert!(
                seen.insert(unit.symbol()),
                "duplicate unit {}",
                unit.symbol()
            );
        }
    }

    #[test]
    fn canonical_unit_is_found_for_every_dimension_a_unit_declares() {
        assert_eq!(canonical_for(Dimension::MASS).unwrap().symbol(), "kg");
        assert_eq!(canonical_for(Dimension::CHARGE).unwrap().symbol(), "C");
        assert_eq!(
            canonical_for(Dimension::DIMENSIONLESS).unwrap().symbol(),
            "1"
        );
        assert!(canonical_for(Dimension::new([9, 9, 0, 0, 0, 0, 0])).is_none());
    }
}
