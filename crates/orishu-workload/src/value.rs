//! The scalar values a manifest may carry, and the one float type it uses.
//!
//! # Why floats are a new-type here
//!
//! A workload's identity is the digest of its canonical encoding, so every
//! value in it must have exactly one encoding. Two IEEE-754 values break that
//! rule on their own:
//!
//! - **NaN** has many bit patterns and no meaningful equality, so two manifests
//!   that compare equal could encode differently, and one manifest could fail
//!   to equal itself.
//! - **Negative zero** is a distinct bit pattern that compares equal to `+0.0`,
//!   so two manifests that compare equal *would* encode differently.
//!
//! Infinities are excluded for a different reason: an infinite domain bound or
//! time step is not a workload anyone can run, and admitting it here only moves
//! the failure closer to the guest.
//!
//! [`FiniteF64`] resolves all three at construction rather than at encoding
//! time. Its constructor rejects NaN and both infinities and normalises `-0.0`
//! to `+0.0`, which makes the canonical encoder total over the typed model: a
//! value that exists can always be encoded, so there is no "this manifest
//! cannot be identified" state to handle downstream.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// Why a float could not be accepted into a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FloatError {
    /// The value was NaN, which has no single encoding and no useful equality.
    #[error("a workload value must be a finite number, got NaN")]
    NotANumber,
    /// The value was an infinity.
    #[error("a workload value must be finite, got {sign}infinity")]
    Infinite {
        /// `"-"` for negative infinity, empty for positive.
        sign: &'static str,
    },
}

/// A finite `f64` with exactly one encoding.
///
/// The inner value is private: the only way to obtain one is through
/// [`FiniteF64::new`], so every value of this type is finite and has a
/// normalised zero.
#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FiniteF64(f64);

impl FiniteF64 {
    /// Zero.
    pub const ZERO: Self = Self(0.0);

    /// Accepts `value` if it is finite, normalising `-0.0` to `+0.0`.
    ///
    /// # Errors
    ///
    /// Returns [`FloatError`] for NaN and for either infinity.
    pub fn new(value: f64) -> Result<Self, FloatError> {
        if value.is_nan() {
            return Err(FloatError::NotANumber);
        }
        if value.is_infinite() {
            return Err(FloatError::Infinite {
                sign: if value.is_sign_negative() { "-" } else { "" },
            });
        }
        // `+ 0.0` maps -0.0 to +0.0 and leaves every other finite value alone.
        Ok(Self(value + 0.0))
    }

    /// The underlying value, guaranteed finite.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Whether the value is strictly greater than zero.
    #[must_use]
    pub fn is_positive(self) -> bool {
        self.0 > 0.0
    }
}

// Derived `Eq`/`Ord` are unavailable for `f64`, but this type has ruled out the
// two values that make them unsound: there is no NaN, so the ordering is total,
// and there is no -0.0, so equality and bit equality agree.
impl Eq for FiniteF64 {}

impl Ord for FiniteF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("a finite float always compares")
    }
}

// Written in terms of `Ord` rather than derived, so the two orderings cannot
// disagree. A derived `PartialOrd` over `f64` would keep IEEE semantics while
// `Ord` above uses the total order, and code that sorted through one and
// searched through the other would be subtly wrong.
impl PartialOrd for FiniteF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for FiniteF64 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl fmt::Debug for FiniteF64 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for FiniteF64 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<f64> for FiniteF64 {
    type Error = FloatError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<FiniteF64> for f64 {
    fn from(value: FiniteF64) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for FiniteF64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // YAML spells `.nan` and `.inf`, so this rejection is reachable from an
        // authored document even though JSON has no literal for either.
        Self::new(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// One already-resolved configuration value.
///
/// Deliberately small. A manifest carries values a guest can consume directly;
/// retaining authored variable and expression *source* alongside them is
/// specified by `docs/protocol-client.md` and arrives with the variables
/// integration. Adding a free-form nested value here now would let a workload
/// smuggle structure the canonical encoding has no rule for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScalarValue {
    /// A boolean flag.
    Bool(bool),
    /// A signed integer.
    Integer(i64),
    /// A finite real number, in canonical SI units for its field.
    Real(FiniteF64),
    /// An enumerated or symbolic value.
    Text(String),
}

impl ScalarValue {
    /// Builds a real value, rejecting a non-finite one.
    ///
    /// # Errors
    ///
    /// Returns [`FloatError`] for NaN or an infinity.
    pub fn real(value: f64) -> Result<Self, FloatError> {
        FiniteF64::new(value).map(ScalarValue::Real)
    }
}

impl From<bool> for ScalarValue {
    fn from(value: bool) -> Self {
        ScalarValue::Bool(value)
    }
}

impl From<i64> for ScalarValue {
    fn from(value: i64) -> Self {
        ScalarValue::Integer(value)
    }
}

impl From<FiniteF64> for ScalarValue {
    fn from(value: FiniteF64) -> Self {
        ScalarValue::Real(value)
    }
}

impl From<String> for ScalarValue {
    fn from(value: String) -> Self {
        ScalarValue::Text(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finite_value_survives_construction_unchanged() {
        assert_eq!(FiniteF64::new(1.5).expect("finite").get(), 1.5);
        assert_eq!(FiniteF64::new(-273.15).expect("finite").get(), -273.15);
    }

    #[test]
    fn nan_is_refused() {
        assert_eq!(FiniteF64::new(f64::NAN), Err(FloatError::NotANumber));
    }

    #[test]
    fn both_infinities_are_refused_and_say_which() {
        assert_eq!(
            FiniteF64::new(f64::INFINITY),
            Err(FloatError::Infinite { sign: "" })
        );
        assert_eq!(
            FiniteF64::new(f64::NEG_INFINITY),
            Err(FloatError::Infinite { sign: "-" })
        );
    }

    #[test]
    fn negative_zero_becomes_positive_zero() {
        // Otherwise two manifests that compare equal would have different
        // canonical bytes, and therefore different identities.
        let negative = FiniteF64::new(-0.0).expect("finite");
        assert_eq!(negative.get().to_bits(), 0.0f64.to_bits());
        assert_eq!(negative, FiniteF64::ZERO);
    }

    #[test]
    fn equal_values_hash_equally() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FiniteF64::new(-0.0).expect("finite"));
        assert!(set.contains(&FiniteF64::new(0.0).expect("finite")));
    }

    #[test]
    fn yaml_nan_and_infinity_are_refused_at_the_authoring_boundary() {
        assert!(serde_yaml::from_str::<FiniteF64>(".nan").is_err());
        assert!(serde_yaml::from_str::<FiniteF64>(".inf").is_err());
        assert!(serde_yaml::from_str::<FiniteF64>("-.inf").is_err());
        assert!(serde_yaml::from_str::<FiniteF64>("1.5").is_ok());
    }

    #[test]
    fn scalar_values_keep_their_authored_shape() {
        assert_eq!(
            serde_json::from_str::<ScalarValue>("true").expect("decodes"),
            ScalarValue::Bool(true)
        );
        assert_eq!(
            serde_json::from_str::<ScalarValue>("7").expect("decodes"),
            ScalarValue::Integer(7)
        );
        assert_eq!(
            serde_json::from_str::<ScalarValue>("\"velocity-verlet\"").expect("decodes"),
            ScalarValue::Text("velocity-verlet".to_owned())
        );
        assert_eq!(
            serde_json::from_str::<ScalarValue>("2.5").expect("decodes"),
            ScalarValue::real(2.5).expect("finite")
        );
    }
}
