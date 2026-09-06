//! Placement value types, with their invariants in their constructors.
//!
//! Every type here has private fields and a validating constructor, so a
//! non-finite coordinate or a denormalised rotation is *unrepresentable*
//! rather than checked at each of the dozens of places that read one. That is
//! the "parse, don't validate" trade the coding-style guide asks for: the
//! invariant is proven once, where the untrusted value arrives.
//!
//! # Why not a linear-algebra crate
//!
//! These types are part of the persisted document contract, which must not
//! depend on a renderer's or solver's chosen representation — the same reason
//! `kagami-catalog`'s dependency test forbids `glam`. A renderer converts to
//! whatever it uses at its own boundary; the document does not learn about it.
//!
//! # Units
//!
//! Every magnitude is canonical SI: metres, metres per second, radians per
//! second. There is no other convention, and none is stored alongside.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a placement value could not be constructed.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum GeometryError {
    /// A component was NaN or infinite. Storing one would let an invalid
    /// number reach a solver as though it were authored intent.
    #[error("{what} must be finite")]
    NotFinite {
        /// Which value was being constructed.
        what: &'static str,
    },
    /// A rotation quaternion had (near-)zero norm and so names no rotation.
    #[error("rotation quaternion must have non-zero norm")]
    DegenerateRotation,
    /// An extent or radius was negative, or zero where zero is meaningless.
    #[error("{what} must be {expected}")]
    OutOfRange {
        /// Which value was being constructed.
        what: &'static str,
        /// The range it had to satisfy.
        expected: &'static str,
    },
    /// A bounding box's upper corner was not strictly above its lower corner.
    #[error("bounds must have a strictly positive extent on every axis")]
    EmptyBounds,
}

/// A finite Cartesian triple in canonical SI.
///
/// One type for positions, displacements, velocities and extents: three
/// separate new-types would need conversions at every call site without
/// preventing a real confusion, because the axes and units are identical.
/// What each *means* is named by the field holding it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[f64; 3]", into = "[f64; 3]")]
pub struct Vector3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vector3 {
    /// The origin, and the zero displacement.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Construct a triple, refusing any non-finite component.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, GeometryError> {
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return Err(GeometryError::NotFinite { what: "vector" });
        }
        Ok(Self { x, y, z })
    }

    /// The x component.
    pub const fn x(self) -> f64 {
        self.x
    }

    /// The y component.
    pub const fn y(self) -> f64 {
        self.y
    }

    /// The z component.
    pub const fn z(self) -> f64 {
        self.z
    }

    /// The components, in x, y, z order.
    pub const fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    /// `true` when every component is non-negative.
    pub fn is_non_negative(self) -> bool {
        self.x >= 0.0 && self.y >= 0.0 && self.z >= 0.0
    }

    /// Component-wise sum.
    ///
    /// Named `checked_add` rather than `add` because it is fallible and so
    /// cannot be `std::ops::Add`: both operands are finite, but their sum may
    /// overflow to infinity, and silently storing that is the failure this
    /// whole module exists to prevent.
    pub fn checked_add(self, other: Self) -> Result<Self, GeometryError> {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    /// The Euclidean length.
    pub fn length(self) -> f64 {
        self.x.hypot(self.y).hypot(self.z)
    }
}

impl TryFrom<[f64; 3]> for Vector3 {
    type Error = GeometryError;

    fn try_from(value: [f64; 3]) -> Result<Self, Self::Error> {
        Self::new(value[0], value[1], value[2])
    }
}

impl From<Vector3> for [f64; 3] {
    fn from(value: Vector3) -> Self {
        value.to_array()
    }
}

/// A unit quaternion, in `[x, y, z, w]` order.
///
/// Normalised on construction, so no reader has to wonder whether the value
/// it was handed still names a rotation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[f64; 4]", into = "[f64; 4]")]
pub struct Rotation {
    x: f64,
    y: f64,
    z: f64,
    w: f64,
}

impl Rotation {
    /// No rotation.
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Construct from quaternion components, normalising them.
    ///
    /// Refuses a non-finite component and a (near-)zero norm, which names no
    /// rotation and whose normalisation would be a division by zero.
    pub fn from_quaternion(x: f64, y: f64, z: f64, w: f64) -> Result<Self, GeometryError> {
        if !(x.is_finite() && y.is_finite() && z.is_finite() && w.is_finite()) {
            return Err(GeometryError::NotFinite { what: "rotation" });
        }
        let norm = (x * x + y * y + z * z + w * w).sqrt();
        if !norm.is_finite() || norm <= f64::EPSILON {
            return Err(GeometryError::DegenerateRotation);
        }
        Ok(Self {
            x: x / norm,
            y: y / norm,
            z: z / norm,
            w: w / norm,
        })
    }

    /// Construct from an axis and an angle in radians.
    pub fn from_axis_angle(axis: Vector3, radians: f64) -> Result<Self, GeometryError> {
        if !radians.is_finite() {
            return Err(GeometryError::NotFinite { what: "angle" });
        }
        let length = axis.length();
        if !length.is_finite() || length <= f64::EPSILON {
            return Err(GeometryError::DegenerateRotation);
        }
        let (sin, cos) = (radians / 2.0).sin_cos();
        let scale = sin / length;
        Self::from_quaternion(axis.x() * scale, axis.y() * scale, axis.z() * scale, cos)
    }

    /// The components, in `[x, y, z, w]` order.
    pub const fn to_array(self) -> [f64; 4] {
        [self.x, self.y, self.z, self.w]
    }

    /// Rotate a vector.
    ///
    /// Uses the standard `v + 2 q_v × (q_v × v + w v)` form rather than
    /// building a matrix, which is both fewer operations and exact for the
    /// identity rotation.
    pub fn apply(self, vector: Vector3) -> Result<Vector3, GeometryError> {
        let q = Vector3::new(self.x, self.y, self.z)?;
        let t = cross(q, vector);
        let inner = Vector3::new(
            t.x + self.w * vector.x(),
            t.y + self.w * vector.y(),
            t.z + self.w * vector.z(),
        )?;
        let outer = cross(q, inner);
        Vector3::new(
            vector.x() + 2.0 * outer.x,
            vector.y() + 2.0 * outer.y,
            vector.z() + 2.0 * outer.z,
        )
    }
}

impl Default for Rotation {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl TryFrom<[f64; 4]> for Rotation {
    type Error = GeometryError;

    fn try_from(value: [f64; 4]) -> Result<Self, Self::Error> {
        Self::from_quaternion(value[0], value[1], value[2], value[3])
    }
}

impl From<Rotation> for [f64; 4] {
    fn from(value: Rotation) -> Self {
        value.to_array()
    }
}

/// The cross product, on raw components.
///
/// Private and infallible: both inputs are already finite, and the result is
/// re-checked by whichever [`Vector3::new`] consumes it.
fn cross(a: Vector3, b: Vector3) -> RawVector {
    RawVector {
        x: a.y() * b.z() - a.z() * b.y(),
        y: a.z() * b.x() - a.x() * b.z(),
        z: a.x() * b.y() - a.y() * b.x(),
    }
}

/// An unvalidated intermediate, so [`cross`] does not have to allocate a
/// `Result` for a value that is immediately consumed.
struct RawVector {
    x: f64,
    y: f64,
    z: f64,
}

/// Where an object is, and how it is oriented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Position in metres.
    pub translation: Vector3,
    /// Orientation.
    pub rotation: Rotation,
}

impl Transform {
    /// A transform at the origin with no rotation.
    pub const IDENTITY: Self = Self {
        translation: Vector3::ZERO,
        rotation: Rotation::IDENTITY,
    };

    /// A translation-only transform.
    pub const fn at(translation: Vector3) -> Self {
        Self {
            translation,
            rotation: Rotation::IDENTITY,
        }
    }

    /// Map a point from the object's local frame into world space.
    pub fn apply(self, point: Vector3) -> Result<Vector3, GeometryError> {
        self.rotation.apply(point)?.checked_add(self.translation)
    }
}

/// How an object is moving, when its motion is authored rather than solved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity {
    /// Linear velocity in metres per second.
    pub linear: Vector3,
    /// Angular velocity in radians per second, as an axis scaled by rate.
    pub angular: Vector3,
}

impl Velocity {
    /// At rest.
    pub const ZERO: Self = Self {
        linear: Vector3::ZERO,
        angular: Vector3::ZERO,
    };
}

/// An object's spatial extent.
///
/// An object with *no* shape is a point: that is the intermediate state every
/// composed object passes through, and both a charge and a mass treat it as
/// one. So this enum has no `Point` variant — `Option<ObjectShape>::None`
/// already says it, and two spellings of the same state would be one too many.
/// Decoded through [`ObjectShape::sphere`] and [`ObjectShape::boxed`], so a
/// document or a wire message cannot introduce a shape those constructors
/// would refuse — a negative radius names nothing, wherever it came from.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "shape",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    try_from = "ObjectShapeRepr"
)]
pub enum ObjectShape {
    /// A ball of the given radius in metres.
    Sphere {
        /// Radius in metres; finite and strictly positive.
        radius: f64,
    },
    /// An axis-aligned box, in the object's local frame, of the given
    /// half-extent in metres.
    Box {
        /// Half-extent in metres; finite and non-negative on every axis.
        half_extent: Vector3,
    },
}

/// The wire shape of an [`ObjectShape`], before its constructor has seen it.
#[derive(Deserialize)]
#[serde(
    tag = "shape",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ObjectShapeRepr {
    Sphere { radius: f64 },
    Box { half_extent: Vector3 },
}

impl TryFrom<ObjectShapeRepr> for ObjectShape {
    type Error = GeometryError;

    fn try_from(value: ObjectShapeRepr) -> Result<Self, Self::Error> {
        match value {
            ObjectShapeRepr::Sphere { radius } => Self::sphere(radius),
            ObjectShapeRepr::Box { half_extent } => Self::boxed(half_extent),
        }
    }
}

impl ObjectShape {
    /// A sphere, refusing a non-finite or non-positive radius.
    ///
    /// Zero is refused rather than accepted as a degenerate sphere: a
    /// zero-radius ball is a point, and a point is `None`.
    pub fn sphere(radius: f64) -> Result<Self, GeometryError> {
        if !radius.is_finite() {
            return Err(GeometryError::NotFinite { what: "radius" });
        }
        if radius <= 0.0 {
            return Err(GeometryError::OutOfRange {
                what: "radius",
                expected: "strictly positive",
            });
        }
        Ok(Self::Sphere { radius })
    }

    /// A box, refusing a negative half-extent.
    ///
    /// A zero half-extent on one axis is allowed: a flat slab is a legitimate
    /// authored shape, unlike a negative one, which names nothing.
    pub fn boxed(half_extent: Vector3) -> Result<Self, GeometryError> {
        if !half_extent.is_non_negative() {
            return Err(GeometryError::OutOfRange {
                what: "half-extent",
                expected: "non-negative on every axis",
            });
        }
        Ok(Self::Box { half_extent })
    }

    /// The radius of a sphere enclosing this shape, for coarse spatial
    /// reasoning that must not depend on the exact shape kind.
    pub fn bounding_radius(self) -> f64 {
        match self {
            Self::Sphere { radius } => radius,
            Self::Box { half_extent } => half_extent.length(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(x: f64, y: f64, z: f64) -> Vector3 {
        Vector3::new(x, y, z).expect("finite components")
    }

    #[test]
    fn a_non_finite_vector_cannot_be_constructed() {
        assert_eq!(
            Vector3::new(f64::NAN, 0.0, 0.0),
            Err(GeometryError::NotFinite { what: "vector" })
        );
        assert!(Vector3::new(0.0, f64::INFINITY, 0.0).is_err());
        assert!(Vector3::new(0.0, 0.0, f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn a_vector_round_trips_through_serde_and_is_revalidated() {
        let value = vector(1.5, -2.0, 3.25);
        let encoded = serde_json::to_string(&value).expect("encodes");
        assert_eq!(encoded, "[1.5,-2.0,3.25]");
        assert_eq!(
            serde_json::from_str::<Vector3>(&encoded).expect("decodes"),
            value
        );
        // The invariant is enforced on the way in, not merely on construction
        // in Rust code, because a persisted document is untrusted input.
        assert!(serde_json::from_str::<Vector3>("[1e400,0,0]").is_err());
    }

    #[test]
    fn a_shape_round_trips_through_serde_and_is_revalidated() {
        let sphere = ObjectShape::sphere(6.371e6).expect("valid radius");
        let encoded = serde_json::to_string(&sphere).expect("encodes");
        assert_eq!(
            serde_json::from_str::<ObjectShape>(&encoded).expect("decodes"),
            sphere
        );

        // Decoding straight onto the variant fields would let a document or an
        // MCP message assert a shape the constructors refuse.
        assert!(serde_json::from_str::<ObjectShape>(r#"{"shape":"sphere","radius":-1}"#).is_err());
        assert!(serde_json::from_str::<ObjectShape>(r#"{"shape":"sphere","radius":0}"#).is_err());
        assert!(
            serde_json::from_str::<ObjectShape>(r#"{"shape":"box","half_extent":[-1.0,1.0,1.0]}"#)
                .is_err()
        );
    }

    #[test]
    fn a_degenerate_rotation_is_refused() {
        assert_eq!(
            Rotation::from_quaternion(0.0, 0.0, 0.0, 0.0),
            Err(GeometryError::DegenerateRotation)
        );
        assert_eq!(
            Rotation::from_axis_angle(Vector3::ZERO, 1.0),
            Err(GeometryError::DegenerateRotation)
        );
        assert!(Rotation::from_quaternion(f64::NAN, 0.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn a_rotation_is_normalised_on_construction() {
        let rotation = Rotation::from_quaternion(0.0, 0.0, 0.0, 7.0).expect("non-zero norm");
        assert_eq!(rotation, Rotation::IDENTITY);
    }

    #[test]
    fn the_identity_rotation_leaves_a_vector_exactly_unchanged() {
        let point = vector(1.0, 2.0, 3.0);
        assert_eq!(Rotation::IDENTITY.apply(point).expect("finite"), point);
    }

    #[test]
    fn a_quarter_turn_about_z_maps_x_onto_y() {
        let rotation =
            Rotation::from_axis_angle(vector(0.0, 0.0, 1.0), std::f64::consts::FRAC_PI_2)
                .expect("valid axis");
        let rotated = rotation.apply(vector(1.0, 0.0, 0.0)).expect("finite");
        assert!((rotated.x() - 0.0).abs() < 1e-12, "{rotated:?}");
        assert!((rotated.y() - 1.0).abs() < 1e-12, "{rotated:?}");
        assert!((rotated.z() - 0.0).abs() < 1e-12, "{rotated:?}");
    }

    #[test]
    fn a_transform_rotates_then_translates() {
        let transform = Transform {
            translation: vector(10.0, 0.0, 0.0),
            rotation: Rotation::from_axis_angle(vector(0.0, 0.0, 1.0), std::f64::consts::FRAC_PI_2)
                .expect("valid axis"),
        };
        let placed = transform.apply(vector(1.0, 0.0, 0.0)).expect("finite");
        assert!((placed.x() - 10.0).abs() < 1e-12, "{placed:?}");
        assert!((placed.y() - 1.0).abs() < 1e-12, "{placed:?}");
    }

    #[test]
    fn a_sphere_needs_a_strictly_positive_radius() {
        assert!(ObjectShape::sphere(1.0).is_ok());
        assert!(ObjectShape::sphere(0.0).is_err());
        assert!(ObjectShape::sphere(-1.0).is_err());
        assert!(ObjectShape::sphere(f64::NAN).is_err());
    }

    #[test]
    fn a_box_may_be_flat_but_not_negative() {
        assert!(ObjectShape::boxed(vector(1.0, 1.0, 0.0)).is_ok());
        assert!(ObjectShape::boxed(vector(1.0, -1.0, 1.0)).is_err());
    }

    #[test]
    fn bounding_radius_does_not_depend_on_the_shape_kind() {
        assert_eq!(
            ObjectShape::sphere(2.0).expect("valid").bounding_radius(),
            2.0
        );
        let cube = ObjectShape::boxed(vector(1.0, 0.0, 0.0)).expect("valid");
        assert_eq!(cube.bounding_radius(), 1.0);
    }
}
