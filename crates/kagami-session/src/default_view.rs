//! The client-owned default view: how an experiment was being looked at.
//!
//! ADR 0022 splits the saved file into two independently interpreted sections.
//! One is authoritative experiment intent, owned by
//! [`DocumentAuthority`](crate::DocumentAuthority). The other is *this*: a
//! narrow, separately versioned record of the projection and camera pose, so
//! reopening an orthographic scene does not present it as perspective.
//!
//! # Why it is not experiment state
//!
//! A camera does not affect physics. Changing it must not advance the
//! experiment revision, must not enter undo, and must not change workload
//! identity — those three properties are what make a saved view safe to keep
//! in the same file as the science. What it *does* do is advance a separate
//! [`ViewRevision`] and mark the file dirty, which is why this module carries
//! its own revision and clean-marker bookkeeping rather than borrowing the
//! authority's.
//!
//! # Why the pose is not a renderer type
//!
//! For the reason [`kagami_document::geometry`] gives for refusing `glam`: a
//! persisted contract must not depend on a renderer's chosen representation.
//! The pose here is validated `f64` with private fields; `kagami-renderer`
//! converts to its own `f32` camera at its own boundary.
//!
//! # Bounds are relative to the scene scale
//!
//! This module owns them, once. A decoded pose is *clamped* into range rather
//! than refused, because a camera slightly outside it is a presentation
//! nuisance and not a corrupt document — refusing would make an experiment
//! unopenable over where someone left the viewport. A non-finite pose is
//! refused outright, because it names no camera at all.
//!
//! What "in range" means depends on [`SceneScale`], and that is the whole
//! reason a scale exists. The camera's reach is declared in *render units* —
//! [`MIN_CAMERA_DISTANCE_UNITS`] to [`MAX_CAMERA_DISTANCE_UNITS`] — and the
//! metre-space limit is derived by multiplying through by the scale. Absolute
//! metre bounds made Kagami a viewer of room-sized scenes: a 2 nm molecule
//! could not be approached because the camera stopped a metre short of it, and
//! a planetary orbit could not be framed because the camera could not retreat
//! past two kilometres.
//!
//! Because the bound needs the scale, the *validated unit is the view* rather
//! than the pose. [`CameraPose`] guarantees only what is true of a camera at
//! any scale — finite components, a positive distance, a pitch where the pan
//! basis is defined. [`AuthoringView`] adds "and reachable at this scale", so
//! it owns its fields and its motion: [`AuthoringView::moved`] is the one
//! place an orbit, a pan or a dolly is applied, and it cannot produce a view
//! the file would then clamp.

use std::fmt;

use kagami_document::{GeometryError, Vector3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::persist::SaveAcknowledgement;

/// The version this build writes for the `defaultView` section.
///
/// Versioned separately from the file envelope on purpose. Presentation
/// evolves on its own schedule, and a view this build cannot interpret must
/// never be a reason an experiment fails to open — see
/// [`crate::document`] for how a newer section is handled.
///
/// Version 2 added [`SceneScale`], and is the change that separate versioning
/// was for: the file's `FORMAT_VERSION` did not move. A version-1 section
/// decodes with the default scale, which is what a file written before scales
/// existed meant — one metre per render unit.
pub const DEFAULT_VIEW_VERSION: u32 = 2;

/// Closest the camera may orbit to its target, in render units.
///
/// Render units, not metres, so the reach follows [`SceneScale`]: one unit at
/// nanometre scale is a nanometre, and at astronomical-unit scale it is an AU.
/// The renderer's fixed near plane sits inside this, which is why the same
/// near/far pair serves every physical scale.
pub const MIN_CAMERA_DISTANCE_UNITS: f64 = 1.0;

/// Furthest the camera may orbit from its target, in render units.
///
/// Inside the renderer's fixed far plane, and — with
/// [`MIN_CAMERA_DISTANCE_UNITS`] — the reason the two projection matrices need
/// no scale-dependent tuning of their own.
pub const MAX_CAMERA_DISTANCE_UNITS: f64 = 2_000.0;

/// Furthest from the origin the orbit target may sit, per axis, in render
/// units.
///
/// A *precision* bound, and the reason it is expressed in render units rather
/// than metres. A renderer works in `f32`, which carries about seven
/// significant digits, and it computes an eye position as the target plus a
/// direction scaled by the orbit distance. Once the target's magnitude exceeds
/// the distance by more than `f32` can resolve, that addition rounds to the
/// target itself — and a camera whose eye coincides with its target has no
/// view direction to speak of.
///
/// 1e6 units against a minimum distance of one leaves the addition resolvable
/// with margin. Being in render units is what makes the bound *mean* something
/// at every scale: 1e6 units is a thousand kilometres at metre scale, a
/// millimetre at nanometre scale, and a million astronomical units at AU
/// scale. The absolute 1e30-metre limit this replaces was chosen only to keep
/// an `f32` cast finite, and said nothing useful about either extreme.
///
/// Clamped rather than refused, like every other bound in this module: a
/// camera pointed somewhere absurd is a presentation nuisance, not a reason a
/// file will not open.
pub const MAX_CAMERA_TARGET_UNITS: f64 = 1e6;

/// Steepest elevation the camera may reach, in radians.
///
/// Just short of straight down. At exactly a right angle the camera's
/// horizontal basis degenerates and pan has no defined direction, so the limit
/// is part of what makes [`AuthoringView::moved`] total. Scale-free: an angle
/// is an angle at any magnitude.
pub const MAX_CAMERA_PITCH: f64 = 89.0 * std::f64::consts::PI / 180.0;

/// Smallest supported scene scale, in metres per render unit.
///
/// Not every finite positive `f64` is a usable ratio. At 1e-320 metres per
/// unit, dividing an ordinary metre-scale coordinate by the scale overflows to
/// infinity — the scale is *finite* and still unsupported, so it is refused
/// rather than accepted into a conversion that cannot answer.
///
/// 1e-30 metres is some five orders of magnitude below the Planck length, so
/// the bound excludes nothing anyone is modelling, and it leaves the whole
/// reachable range convertible in both directions with room to spare.
pub const MIN_SCENE_SCALE: f64 = 1e-30;

/// Largest supported scene scale, in metres per render unit.
///
/// 1e30 metres is roughly ten thousand times the radius of the observable
/// universe. The counterpart to [`MIN_SCENE_SCALE`], and bounded for the same
/// reason: beyond it, converting the reachable range back into metres stops
/// being representable.
pub const MAX_SCENE_SCALE: f64 = 1e30;

/// Radians of orbit per unit of pointer travel.
const ORBIT_SENSITIVITY: f64 = 0.008;

/// Fraction of the orbit distance panned per unit of pointer travel.
const PAN_SENSITIVITY: f64 = 0.0015;

/// Fraction of the orbit distance covered per unit of dolly.
const ZOOM_SENSITIVITY: f64 = 0.1;

/// Why a view value could not be constructed.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ViewError {
    /// A pose component was NaN or infinite, and so names no camera.
    #[error("camera {what} must be finite")]
    NotFinite {
        /// Which component was being constructed.
        what: &'static str,
    },
    /// A pan moved the orbit target somewhere unrepresentable.
    #[error("camera target left the representable range: {source}")]
    Unrepresentable {
        /// The underlying refusal.
        #[source]
        source: GeometryError,
    },
    /// A scene scale was zero, negative, or not finite.
    ///
    /// Refused rather than clamped, unlike every camera bound here. A scale is
    /// a *ratio*: zero names no mapping between metres and units at all, and a
    /// negative one mirrors the world. There is no nearest sensible value to
    /// bring either towards, so there is nothing to clamp.
    #[error("scene scale must be a finite positive length, not {metres} m per unit")]
    ScaleNotPositive {
        /// The value that was refused.
        metres: f64,
    },
    /// An explicit view change would have moved the orbit focus to make it fit.
    ///
    /// Refused rather than applied, because the focus is *what the user is
    /// looking at*. Silently relocating it — a scale change that slides the
    /// focus from a metre out to a millimetre out — leaves someone staring at
    /// a different place with no indication that anything moved, which is
    /// worse than being told the change cannot be made yet. The message names
    /// the way out, since the remedy is to bring the focus nearer the origin
    /// first.
    ///
    /// Gestures and decoded files clamp instead: a pan held against the limit
    /// must not fail, and a saved camera must never stop an experiment from
    /// opening.
    #[error(
        "the focus is {focus_metres} m from the origin, beyond the {limit_metres} m reachable at \
         {scale_metres} m per unit; bring the focus nearer the origin first"
    )]
    FocusOutOfReach {
        /// How far the focus sits from the origin, on its furthest axis.
        focus_metres: f64,
        /// How far a focus may sit at the requested scale.
        limit_metres: f64,
        /// The scale that was requested.
        scale_metres: f64,
    },
    /// A scene scale was positive and finite, and still outside what this
    /// build can convert through.
    ///
    /// Its own variant because "not a ratio" and "a ratio nothing can compute
    /// with" call for different messages: the first is a mistake, and the
    /// second is a limit worth naming.
    #[error(
        "scene scale {metres} m per unit is outside the supported range \
         {MIN_SCENE_SCALE:e} to {MAX_SCENE_SCALE:e}"
    )]
    ScaleOutOfRange {
        /// The value that was refused.
        metres: f64,
    },
    /// A typed scene scale did not parse as a length.
    #[error("scene scale is not a length expression: {message}")]
    ScaleUnparseable {
        /// What the expression engine objected to.
        message: String,
    },
    /// A typed scene scale parsed, but as some other dimension.
    #[error("scene scale must be a length, and `{source_text}` is not one")]
    ScaleNotALength {
        /// What was typed.
        source_text: String,
    },
}

impl ViewError {
    /// A stable identifier for this reason.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFinite { .. } => "camera_not_finite",
            Self::Unrepresentable { .. } => "camera_unrepresentable",
            Self::ScaleNotPositive { .. } => "scale_not_positive",
            Self::ScaleOutOfRange { .. } => "scale_out_of_range",
            Self::FocusOutOfReach { .. } => "focus_out_of_reach",
            Self::ScaleUnparseable { .. } => "scale_unparseable",
            Self::ScaleNotALength { .. } => "scale_not_a_length",
        }
    }
}

/// How many metres one render unit represents.
///
/// The setting that lets one camera serve an experiment of molecules and an
/// experiment of planets. It converts an SI world value into render space at
/// the rendering boundary and is **never a second way to store a position**: no
/// authored coordinate, extent, velocity or constant is expressed in these
/// units, and nothing a solver reads is affected by it.
///
/// # Why the division comes before the narrowing
///
/// [`Self::to_render`] divides, and the renderer narrows the result to `f32`
/// afterwards. That order is the whole mechanism: it makes the *magnitude* of
/// the value, rather than its absolute distance from an arbitrary origin,
/// decide how much of `f32`'s seven digits it consumes. A 2 nm radius cast
/// straight to `f32` is 2e-9 and disappears into the camera's near plane; the
/// same radius divided by a nanometre scale is 2.0, and every fixed limit the
/// renderer has is suddenly the right size.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct SceneScale(f64);

impl Default for SceneScale {
    fn default() -> Self {
        Self::METRE
    }
}

impl TryFrom<f64> for SceneScale {
    type Error = ViewError;

    fn try_from(metres: f64) -> Result<Self, Self::Error> {
        Self::from_metres(metres)
    }
}

impl From<SceneScale> for f64 {
    fn from(scale: SceneScale) -> Self {
        scale.0
    }
}

impl SceneScale {
    /// One render unit is one nanometre.
    pub const NANOMETRE: Self = Self(1.0e-9);
    /// One render unit is one micrometre.
    pub const MICROMETRE: Self = Self(1.0e-6);
    /// One render unit is one millimetre.
    pub const MILLIMETRE: Self = Self(1.0e-3);
    /// One render unit is one metre. The default, and the scale at which this
    /// setting has no effect at all.
    pub const METRE: Self = Self(1.0);
    /// One render unit is one kilometre.
    pub const KILOMETRE: Self = Self(1.0e3);
    /// One render unit is one astronomical unit.
    pub const ASTRONOMICAL_UNIT: Self = Self(1.495_978_707e11);
    /// One render unit is one light-year.
    pub const LIGHT_YEAR: Self = Self(9.460_730_472_580_8e15);

    /// The named scales a control offers, smallest first.
    ///
    /// Ordered because a picker should read as a ruler rather than a set. The
    /// range is deliberate: nanometre reaches molecular structure and
    /// light-year reaches galactic, so no experiment this product is for falls
    /// outside a preset *and* a custom entry.
    ///
    /// Carries the values only; names come from [`Self::label`], so a preset
    /// has exactly one spelling. A `'static` slice rather than something built
    /// on demand, so a control that offers it every frame allocates nothing.
    pub const PRESETS: &'static [Self] = &[
        Self::NANOMETRE,
        Self::MICROMETRE,
        Self::MILLIMETRE,
        Self::METRE,
        Self::KILOMETRE,
        Self::ASTRONOMICAL_UNIT,
        Self::LIGHT_YEAR,
    ];

    /// Construct a scale from metres per render unit.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::ScaleNotPositive`] for zero, a negative value, or
    /// a non-finite one, and [`ViewError::ScaleOutOfRange`] for a positive
    /// finite value outside [`MIN_SCENE_SCALE`]`..=`[`MAX_SCENE_SCALE`] —
    /// where the conversions this type exists for stop being computable.
    pub fn from_metres(metres: f64) -> Result<Self, ViewError> {
        if !metres.is_finite() || metres <= 0.0 {
            return Err(ViewError::ScaleNotPositive { metres });
        }
        if !(MIN_SCENE_SCALE..=MAX_SCENE_SCALE).contains(&metres) {
            return Err(ViewError::ScaleOutOfRange { metres });
        }
        Ok(Self(metres))
    }

    /// Read a scale from typed text, as a length.
    ///
    /// Goes through the one expression engine rather than a second parser:
    /// `orishu_variables` already treats a unit as part of the language, so
    /// `1 nm`, `2.5e-10 m`, `1 AU` and `1/2 km` all work, and a dimension
    /// other than length is refused rather than silently taken as metres.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::ScaleUnparseable`] when the text is not an
    /// expression, [`ViewError::ScaleNotALength`] when it is not a length, and
    /// [`ViewError::ScaleNotPositive`] when it is not a positive one.
    pub fn parse(source: &str) -> Result<Self, ViewError> {
        Self::parse_bounded(source, &orishu_variables::Limits::DEFAULT)
    }

    /// Read a scale from typed text under caller-supplied bounds.
    ///
    /// The bounded counterpart to [`Self::parse`], mirroring
    /// [`orishu_variables::CompiledExpression::parse_bounded`]: source length,
    /// parse depth and evaluation work are all charged against `limits`, so a
    /// caller facing untrusted input can be stricter than the default.
    ///
    /// Evaluated against an **empty** variable system, which is the important
    /// part. Only shared units and constants resolve; a scale cannot reference
    /// a document variable, a catalog value or an observation, so it is a
    /// scalar and not a retained dependency on anything that could later
    /// change underneath it.
    ///
    /// # Errors
    ///
    /// As [`Self::parse`].
    pub fn parse_bounded(
        source: &str,
        limits: &orishu_variables::Limits,
    ) -> Result<Self, ViewError> {
        let variables = orishu_variables::VariablesSystem::with_limits(*limits);
        let quantity = variables
            .eval(source)
            .map_err(|error| ViewError::ScaleUnparseable {
                message: error.to_string(),
            })?;
        // A bare number is dimensionless, and reading it as metres is the one
        // convenience worth having: "1000" in a metres-per-unit field means
        // metres. Any *other* dimension is a mistake, not a shorthand.
        if !quantity.is_dimensionless()
            && quantity.dimension() != orishu_variables::Dimension::LENGTH
        {
            return Err(ViewError::ScaleNotALength {
                source_text: source.to_owned(),
            });
        }
        Self::from_metres(quantity.magnitude())
    }

    /// Metres per render unit.
    pub const fn metres(self) -> f64 {
        self.0
    }

    /// The name of the preset this is, or `"Custom"`.
    ///
    /// So a control cannot report a preset it is not on. A value typed to
    /// within a rounding error of a preset is genuinely a custom scale and
    /// says so.
    pub fn label(self) -> &'static str {
        match self {
            Self::NANOMETRE => "Nanometre",
            Self::MICROMETRE => "Micrometre",
            Self::MILLIMETRE => "Millimetre",
            Self::METRE => "Metre",
            Self::KILOMETRE => "Kilometre",
            Self::ASTRONOMICAL_UNIT => "Astronomical unit",
            Self::LIGHT_YEAR => "Light-year",
            _ => "Custom",
        }
    }

    /// An SI-metre world value in render units, if that lands anywhere.
    ///
    /// `None` when the quotient is not finite. Fallible because a small scale
    /// and a large coordinate genuinely have no answer: at 1e-30 metres per
    /// unit, a finite `f64::MAX` divided through overflows to infinity, and a
    /// renderer handed that produces nothing meaningful.
    ///
    /// This is the **geometry** direction, and out-of-range geometry is
    /// *omitted* rather than pinned to a boundary it does not occupy — an
    /// object drawn at the edge of the reachable region because its real
    /// position could not be converted is a lie about where it is. The
    /// camera's defensive clamping at the render boundary is a separate thing
    /// with a separate justification: a camera must always render *something*,
    /// so it saturates where geometry declines.
    pub fn to_render(self, world_metres: f64) -> Option<f64> {
        let units = world_metres / self.0;
        units.is_finite().then_some(units)
    }

    /// A render-space value back in SI metres, if that names a place.
    ///
    /// `None` when the product is not finite. Fallible because this is the
    /// direction that produces *authored candidates* — a picked position, a
    /// measured distance — and an authored coordinate is never allowed to be
    /// non-finite (the same rule [`Vector3`] enforces). Multiplying a large
    /// render value by a large scale is exactly how that could happen, so the
    /// caller is made to deal with it rather than discovering an infinity
    /// downstream.
    pub fn to_world(self, render_units: f64) -> Option<f64> {
        let metres = render_units * self.0;
        metres.is_finite().then_some(metres)
    }
}

impl fmt::Display for SceneScale {
    /// A preset's name, or a custom value as a canonical-SI quantity.
    ///
    /// Reusing [`orishu_variables::Quantity`]'s display rather than printing a
    /// bare float keeps the one existing spelling of a dimensioned value. It
    /// renders a nanometre as `1e-9 m` rather than picking a prefix — a
    /// prefix-selecting formatter belongs to S-VARIABLES and every dimensioned
    /// field wants it, so this feature does not grow a private one. The preset
    /// labels carry the readable names in the meantime, which is why a control
    /// showing "Nanometre" reads well even though the number does not.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.label() {
            "Custom" => {
                match orishu_variables::Quantity::new(self.0, orishu_variables::Dimension::LENGTH) {
                    Ok(quantity) => write!(formatter, "{quantity}/unit"),
                    // Unreachable: the value is finite and positive by
                    // construction, which is all `Quantity` asks of a length.
                    Err(_) => write!(formatter, "{} m/unit", self.0),
                }
            }
            named => formatter.write_str(named),
        }
    }
}

/// How the scene is projected onto the window.
///
/// Explicitly one of two choices (ADR 0022) rather than a continuum: an
/// orthographic view is what makes relative extents comparable by eye, and a
/// perspective view is what makes depth legible. Nothing here is a physical
/// parameter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Projection {
    /// A perspective frustum. The default.
    #[default]
    Perspective,
    /// A parallel projection, preserving relative extents.
    Orthographic,
}

impl Projection {
    /// The other projection, for a control that toggles between them.
    pub const fn other(self) -> Self {
        match self {
            Self::Perspective => Self::Orthographic,
            Self::Orthographic => Self::Perspective,
        }
    }

    /// A stable identifier, for a control label or a report.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Perspective => "Perspective",
            Self::Orthographic => "Orthographic",
        }
    }
}

impl fmt::Display for Projection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One camera motion, in pointer-travel units.
///
/// A value rather than a method call, so the gesture a window recognises and
/// the pose arithmetic that answers it stay on opposite sides of the boundary:
/// `apps/kagami` turns pointer events into these, and [`AuthoringView::moved`]
/// is the only thing that interprets them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraMotion {
    /// Swing around the target.
    Orbit {
        /// Horizontal pointer travel.
        dx: f64,
        /// Vertical pointer travel.
        dy: f64,
    },
    /// Slide the target across the view plane.
    Pan {
        /// Horizontal pointer travel.
        dx: f64,
        /// Vertical pointer travel.
        dy: f64,
    },
    /// Move towards or away from the target.
    Dolly {
        /// Positive moves closer.
        amount: f64,
    },
}

/// Where the camera is, as an orbit around a target point.
///
/// Spherical rather than a matrix or an eye/look-at pair, because that is what
/// the interaction is: every gesture the viewport offers changes exactly one of
/// these four numbers, and a pose that cannot be reached by orbiting is not a
/// pose this camera has.
///
/// Z-up, matching the document's own convention.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "CameraPoseRepr")]
pub struct CameraPose {
    target: Vector3,
    distance: f64,
    yaw: f64,
    pitch: f64,
}

/// The encoded shape of a [`CameraPose`], before its constructor has seen it.
///
/// Decoding straight onto the fields would let a file assert a pose the
/// constructor would have bounded — the same reason
/// [`kagami_document::ObjectShape`] decodes through one of these.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CameraPoseRepr {
    target: Vector3,
    distance: f64,
    yaw: f64,
    pitch: f64,
}

impl TryFrom<CameraPoseRepr> for CameraPose {
    type Error = ViewError;

    fn try_from(value: CameraPoseRepr) -> Result<Self, Self::Error> {
        Self::new(value.target, value.distance, value.yaw, value.pitch)
    }
}

impl Default for CameraPose {
    fn default() -> Self {
        Self {
            target: Vector3::ZERO,
            distance: 18.0,
            yaw: -45.0_f64.to_radians(),
            pitch: 35.0_f64.to_radians(),
        }
    }
}

impl CameraPose {
    /// Construct a pose, refusing what names no camera and normalising the
    /// rest.
    ///
    /// What this guarantees is *scale-free*: every component is finite, the
    /// distance is strictly positive, the pitch is one where the pan basis is
    /// defined, and the yaw is normalised into `(-π, π]` so orbiting for a
    /// while cannot make an otherwise identical pose compare unequal — which
    /// is what keeps [`AuthoringViewState`] from dirtying a file over a full
    /// turn.
    ///
    /// What it deliberately does *not* guarantee is that the pose is reachable
    /// or renderable: how near and far a camera may sit depends on
    /// [`SceneScale`], so [`AuthoringView`] owns those bounds. A `CameraPose`
    /// on its own is a direction and a separation, not yet a view.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::NotFinite`] for a NaN or infinite component, or
    /// for a distance that is not strictly positive — an eye coincident with
    /// its target has no view direction.
    pub fn new(target: Vector3, distance: f64, yaw: f64, pitch: f64) -> Result<Self, ViewError> {
        if !distance.is_finite() || distance <= 0.0 {
            return Err(ViewError::NotFinite { what: "distance" });
        }
        if !yaw.is_finite() {
            return Err(ViewError::NotFinite { what: "yaw" });
        }
        if !pitch.is_finite() {
            return Err(ViewError::NotFinite { what: "pitch" });
        }
        Ok(Self {
            target,
            distance,
            yaw: normalise_angle(yaw),
            pitch: pitch.clamp(-MAX_CAMERA_PITCH, MAX_CAMERA_PITCH),
        })
    }

    /// The point being orbited.
    pub const fn target(self) -> Vector3 {
        self.target
    }

    /// How far the camera sits from its target, in metres.
    pub const fn distance(self) -> f64 {
        self.distance
    }

    /// Rotation about the world Z axis, in radians.
    pub const fn yaw(self) -> f64 {
        self.yaw
    }

    /// Elevation above the target's horizontal plane, in radians.
    pub const fn pitch(self) -> f64 {
        self.pitch
    }

    /// Where the camera is, in world space.
    pub fn eye(self) -> Result<Vector3, ViewError> {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let offset = Vector3::new(
            cos_pitch * cos_yaw * self.distance,
            cos_pitch * sin_yaw * self.distance,
            sin_pitch * self.distance,
        )
        .map_err(unrepresentable)?;
        self.target.checked_add(offset).map_err(unrepresentable)
    }

    /// Apply one motion, without regard to how far the scale allows it.
    ///
    /// Produces the pose the gesture asks for; [`AuthoringView::moved`] brings
    /// it inside the scale's window afterwards. Splitting it that way is what
    /// keeps every gesture *total*: a dolly that would drive the distance to
    /// zero or a pan that would leave the reachable region is clamped rather
    /// than refused, and a scroll wheel held against the stop does nothing
    /// instead of failing.
    ///
    /// Pan is proportional to the orbit distance, so it is already
    /// scale-relative with no reference to [`SceneScale`] at all: a camera
    /// close to a molecule pans in nanometres and one framing an orbit pans in
    /// astronomical units, because both pan by a fraction of how far away they
    /// are.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::Unrepresentable`] when a pan would carry the
    /// target outside the representable range. Both operands are finite, so
    /// only their sum can fail — the same reason [`Vector3::checked_add`] is
    /// fallible.
    fn stepped(self, motion: CameraMotion) -> Result<RawPose, ViewError> {
        Ok(match motion {
            CameraMotion::Orbit { dx, dy } => RawPose {
                target: self.target,
                distance: self.distance,
                yaw: self.yaw - dx * ORBIT_SENSITIVITY,
                pitch: self.pitch + dy * ORBIT_SENSITIVITY,
            },
            CameraMotion::Pan { dx, dy } => {
                // Closed form rather than cross products of a normalised
                // forward vector: for an orbit camera the basis is a direct
                // function of yaw and pitch, and writing it out avoids both a
                // linear-algebra dependency and the degenerate normalisation a
                // pitch of exactly ±90° would hit. `MAX_CAMERA_PITCH` keeps
                // `cos(pitch)` away from zero, so this basis is always defined.
                let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
                let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
                let right = [-sin_yaw, cos_yaw, 0.0];
                let up = [-cos_yaw * sin_pitch, -sin_yaw * sin_pitch, cos_pitch];
                let step_scale = self.distance * PAN_SENSITIVITY;
                let step = Vector3::new(
                    (-right[0] * dx + up[0] * dy) * step_scale,
                    (-right[1] * dx + up[1] * dy) * step_scale,
                    (-right[2] * dx + up[2] * dy) * step_scale,
                )
                .map_err(unrepresentable)?;
                RawPose {
                    target: self.target.checked_add(step).map_err(unrepresentable)?,
                    distance: self.distance,
                    yaw: self.yaw,
                    pitch: self.pitch,
                }
            }
            CameraMotion::Dolly { amount } => RawPose {
                target: self.target,
                distance: self.distance * (1.0 - amount * ZOOM_SENSITIVITY),
                yaw: self.yaw,
                pitch: self.pitch,
            },
        })
    }
}

/// A pose a gesture asked for, before the scale's bounds have been applied.
///
/// Private, and never stored: it exists so the arithmetic and the clamping can
/// be separate steps without a `CameraPose` ever briefly holding a distance
/// its own constructor would refuse.
#[derive(Clone, Copy, Debug)]
struct RawPose {
    target: Vector3,
    distance: f64,
    yaw: f64,
    pitch: f64,
}

/// The whole of what the file remembers about the view.
///
/// Deliberately small. ADR 0022 admits projection, camera pose and orbit focus
/// — the orbit target *is* the focus — and leaves window layout and selection
/// to application preferences. A follow target is admitted by the ADR too but
/// belongs to the follow slice, and adding one later advances
/// [`DEFAULT_VIEW_VERSION`] rather than the file's envelope version, which is
/// the entire reason this section is versioned on its own. Adding
/// [`SceneScale`] is the first time that happened.
///
/// # Why the fields are private
///
/// This is the validated unit, not a bag. Whether a camera is reachable
/// depends on the scale sitting beside it, so the two cannot be set
/// independently and still be consistent: changing the scale re-bounds the
/// camera, and setting a camera bounds it against the scale in force. Public
/// fields would have made an inconsistent pair constructible, which is exactly
/// the state a renderer cannot draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoringView {
    projection: Projection,
    scale: SceneScale,
    camera: CameraPose,
}

impl Default for AuthoringView {
    fn default() -> Self {
        // Constructed literally rather than through `new`: the default pose is
        // 18 render units at the default scale of one metre per unit, which is
        // inside every bound below by inspection. Going through the fallible
        // constructor here would force an `expect` on a value this module
        // itself chose.
        Self {
            projection: Projection::Perspective,
            scale: SceneScale::METRE,
            camera: CameraPose::default(),
        }
    }
}

impl AuthoringView {
    /// Assemble a view, bringing `camera` inside what `scale` can render.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError`] when the bounded pose is not representable, which
    /// clamping alone cannot cause — the failure path exists because
    /// [`Vector3`] is validated on construction, not because a clamp can
    /// overflow.
    pub fn new(
        projection: Projection,
        scale: SceneScale,
        camera: CameraPose,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            projection,
            scale,
            // Clamped, not refused: this is the path a decoded file takes, and
            // ADR 0022 does not let a camera stop an experiment from opening.
            camera: bounded_raw(camera.raw(), scale, FocusPolicy::Clamp)?,
        })
    }

    /// How the scene is projected.
    pub const fn projection(self) -> Projection {
        self.projection
    }

    /// How many metres one render unit represents.
    pub const fn scale(self) -> SceneScale {
        self.scale
    }

    /// Where the camera is, in SI metres.
    pub const fn camera(self) -> CameraPose {
        self.camera
    }

    /// The same view under a different projection.
    ///
    /// Infallible: a projection is not a bound on anything.
    pub const fn with_projection(mut self, projection: Projection) -> Self {
        self.projection = projection;
        self
    }

    /// The same view at a different scale, keeping the focus exactly where it
    /// is.
    ///
    /// The distance is re-bounded, because how far away a camera may sit is
    /// what a scale decides — and the amount it moved comes back as a
    /// [`ViewAdjustment`] so it can be reported rather than discovered. The
    /// *focus* is never moved: if it is beyond what the requested scale can
    /// reach, the change is refused and the view is left exactly as it was.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::FocusOutOfReach`] when the focus would have had to
    /// move, and [`ViewError`] when the re-bounded pose is not representable.
    pub fn with_scale(self, scale: SceneScale) -> Result<(Self, ViewAdjustment), ViewError> {
        let camera = bounded_raw(self.camera.raw(), scale, FocusPolicy::Refuse)?;
        Ok((
            Self {
                projection: self.projection,
                scale,
                camera,
            },
            distance_adjustment(self.camera, camera),
        ))
    }

    /// The same view with a different camera, bounded by the scale in force.
    ///
    /// An explicit placement, so it follows the same rule as
    /// [`Self::with_scale`]: the distance may be brought inside the reachable
    /// window and reported, and a focus beyond reach is refused rather than
    /// quietly moved.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::FocusOutOfReach`] for a focus the scale in force
    /// cannot reach, and [`ViewError`] when the bounded pose is not
    /// representable.
    pub fn with_camera(self, camera: CameraPose) -> Result<(Self, ViewAdjustment), ViewError> {
        let bounded = bounded_raw(camera.raw(), self.scale, FocusPolicy::Refuse)?;
        Ok((
            Self {
                projection: self.projection,
                scale: self.scale,
                camera: bounded,
            },
            distance_adjustment(camera, bounded),
        ))
    }

    /// Apply one camera motion, bounded by the scale in force.
    ///
    /// The one place a gesture becomes a view. A motion the bounds absorb
    /// entirely returns an equal view rather than an error, so a control held
    /// against a limit is a no-op and not a refusal — which is also why this
    /// clamps the focus instead of refusing: a pan that reaches the edge of
    /// the reachable region must stop there, not fail.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::Unrepresentable`] when a pan names somewhere no
    /// coordinate can describe.
    pub fn moved(self, motion: CameraMotion) -> Result<Self, ViewError> {
        let raw = self.camera.stepped(motion)?;
        Ok(Self {
            projection: self.projection,
            scale: self.scale,
            camera: bounded_raw(raw, self.scale, FocusPolicy::Clamp)?,
        })
    }
}

/// How far the bounding moved the camera, if it did.
fn distance_adjustment(requested: CameraPose, bounded: CameraPose) -> ViewAdjustment {
    if requested.distance() == bounded.distance() {
        ViewAdjustment::None
    } else {
        ViewAdjustment::Distance {
            from: requested.distance(),
            to: bounded.distance(),
        }
    }
}

/// What to do when a pose's focus is beyond what a scale can reach.
///
/// The distinction the review of K14 turned on. Both answers are right, for
/// different callers:
///
/// - [`FocusPolicy::Clamp`] for anything that must not fail — decoding a saved
///   file, where a camera can never be a reason an experiment will not open,
///   and applying a gesture, where a pan held against the limit must be a
///   no-op rather than an error.
/// - [`FocusPolicy::Refuse`] for an explicit choice. The focus is what the
///   user is looking at, and relocating it to make a scale change fit is a
///   change nobody asked for and nobody is told about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusPolicy {
    Clamp,
    Refuse,
}

/// Bring a requested pose inside what `scale` can render.
///
/// Every bound here is declared in render units and multiplied through by the
/// scale, which is what makes the same numbers serve a molecule and a solar
/// system. The pose that comes out is in SI metres, as stored.
///
/// The *distance* is always clamped: adjusting how far away the camera sits is
/// the declared consequence of changing scale, and is reported through
/// [`ViewAdjustment`] rather than hidden. Only the focus is subject to
/// `policy`.
fn bounded_raw(
    raw: RawPose,
    scale: SceneScale,
    policy: FocusPolicy,
) -> Result<CameraPose, ViewError> {
    let metres = scale.metres();
    let target_limit = MAX_CAMERA_TARGET_UNITS * metres;
    let [x, y, z] = raw.target.to_array();
    let furthest = x.abs().max(y.abs()).max(z.abs());

    let target = if furthest <= target_limit {
        raw.target
    } else if policy == FocusPolicy::Refuse {
        return Err(ViewError::FocusOutOfReach {
            focus_metres: furthest,
            limit_metres: target_limit,
            scale_metres: metres,
        });
    } else {
        // Component-wise, which for a target this far out distorts the
        // direction it sits in. That is not a loss worth avoiding *on this
        // path*: the pose arrived from a file or a gesture already naming
        // somewhere the renderer cannot look, and what matters is that what
        // comes back is a place rather than a rounding artefact.
        Vector3::new(
            x.clamp(-target_limit, target_limit),
            y.clamp(-target_limit, target_limit),
            z.clamp(-target_limit, target_limit),
        )
        .map_err(unrepresentable)?
    };

    // NaN cannot survive `clamp`, so it is refused before it reaches one.
    if !raw.distance.is_finite() {
        return Err(ViewError::NotFinite { what: "distance" });
    }
    CameraPose::new(
        target,
        raw.distance.clamp(
            MIN_CAMERA_DISTANCE_UNITS * metres,
            MAX_CAMERA_DISTANCE_UNITS * metres,
        ),
        raw.yaw,
        raw.pitch,
    )
}

impl CameraPose {
    /// This pose as a raw request, for re-bounding at another scale.
    fn raw(self) -> RawPose {
        RawPose {
            target: self.target,
            distance: self.distance,
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

/// What applying a view change had to adjust to make it fit.
///
/// Returned rather than swallowed. Selecting nanometre scale takes the default
/// camera from eighteen metres to two micrometres — a change of twelve orders
/// of magnitude in where the camera is — and a user who is not told that
/// happened has no way to understand what they are now looking at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewAdjustment {
    /// The change applied exactly as asked.
    None,
    /// The orbit distance was brought inside the reachable window.
    Distance {
        /// Where the camera was, in metres.
        from: f64,
        /// Where it is now, in metres.
        to: f64,
    },
}

impl ViewAdjustment {
    /// `true` when nothing had to be adjusted.
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    /// What to tell the user, if anything.
    ///
    /// The wording lives here so both workspace modes report an adjustment the
    /// same way, rather than each surface inventing its own sentence.
    pub fn message(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Distance { from, to } => Some(format!(
                "Camera distance adjusted from {from:e} m to {to:e} m to stay within reach at \
                 this scale."
            )),
        }
    }
}

/// Whether a view change did anything, and what it had to adjust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewChange {
    /// `false` when the request was already in force.
    pub changed: bool,
    /// What had to move to make the request fit.
    pub adjustment: ViewAdjustment,
}

impl ViewChange {
    /// A request that was already in force.
    pub const UNCHANGED: Self = Self {
        changed: false,
        adjustment: ViewAdjustment::None,
    };
}

/// A monotonic counter over authoring-view changes.
///
/// Separate from [`kagami_document::ExperimentRevision`] because the two must
/// be able to disagree: that is exactly what lets a save report "the camera
/// moved since, but the science did not". Monotonic and without history —
/// there is no undo for a camera, and ADR 0022 says so.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ViewRevision(u64);

impl ViewRevision {
    /// The revision of a view nobody has changed yet.
    pub const INITIAL: Self = Self(0);

    /// The next revision.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The underlying counter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ViewRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "v{}", self.0)
    }
}

/// The client's authoring view, and its unsaved-change bookkeeping.
///
/// Owned by whichever client is authoring — this crate holds the *rules* about
/// what a view is and when it is dirty, not a global. A headless authority has
/// no reason to construct one, and ADR 0022 says workload compilation and
/// Orishu ignore this section entirely.
///
/// Observation/replay view changes never come here. They are ephemeral by ADR
/// 0022, so the window keeps its own throwaway copy and this state is not
/// touched while a run is being watched.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringViewState {
    view: AuthoringView,
    revision: ViewRevision,
    clean_revision: ViewRevision,
    acknowledged_revision: Option<ViewRevision>,
}

impl Default for AuthoringViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthoringViewState {
    /// A default view that has never been changed, and so is clean.
    pub fn new() -> Self {
        Self {
            view: AuthoringView::default(),
            revision: ViewRevision::INITIAL,
            clean_revision: ViewRevision::INITIAL,
            acknowledged_revision: None,
        }
    }

    /// What the window is drawing.
    pub const fn view(&self) -> AuthoringView {
        self.view
    }

    /// The projection in force.
    pub const fn projection(&self) -> Projection {
        self.view.projection()
    }

    /// The scale in force.
    pub const fn scale(&self) -> SceneScale {
        self.view.scale()
    }

    /// The pose in force.
    pub const fn camera(&self) -> CameraPose {
        self.view.camera()
    }

    /// The view revision in force.
    pub const fn revision(&self) -> ViewRevision {
        self.revision
    }

    /// `true` when the view has advanced past the revision last persisted.
    ///
    /// Derived rather than remembered, for the reason
    /// [`crate::SessionView::dirty`] gives: a flag somebody has to clear is a
    /// flag that will be wrong after a failed save.
    pub const fn is_dirty(&self) -> bool {
        self.revision.get() != self.clean_revision.get()
    }

    /// Choose a projection. Returns whether anything changed.
    ///
    /// Selecting the projection that is already in force advances no revision
    /// and dirties nothing: a control the user clicks twice must not make a
    /// saved file look modified.
    pub fn set_projection(&mut self, projection: Projection) -> bool {
        self.adopt_candidate(self.view.with_projection(projection))
    }

    /// Choose a scene scale.
    ///
    /// The camera's *distance* is re-bounded against the new scale, because a
    /// pose the previous scale could reach and this one cannot would otherwise
    /// be left pointing somewhere unrenderable. That re-bounding counts as part
    /// of the same change — one revision, not two — and comes back as a
    /// [`ViewAdjustment`] so a caller can report it.
    ///
    /// The *focus* is never moved to make a scale fit; see
    /// [`AuthoringView::with_scale`].
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::FocusOutOfReach`] when the focus is beyond the
    /// requested scale's reach, or [`ViewError`] when the re-bounded pose is
    /// not representable. Either way the view and the revision are exactly as
    /// they were.
    pub fn set_scale(&mut self, scale: SceneScale) -> Result<ViewChange, ViewError> {
        let (candidate, adjustment) = self.view.with_scale(scale)?;
        Ok(ViewChange {
            changed: self.adopt_candidate(candidate),
            adjustment,
        })
    }

    /// Place the camera.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::FocusOutOfReach`] for a focus the scale in force
    /// cannot reach, or [`ViewError`] when the bounded pose is not
    /// representable, leaving the view exactly as it was.
    pub fn set_camera(&mut self, camera: CameraPose) -> Result<ViewChange, ViewError> {
        let (candidate, adjustment) = self.view.with_camera(camera)?;
        Ok(ViewChange {
            changed: self.adopt_candidate(candidate),
            adjustment,
        })
    }

    /// Apply one motion to the camera. Returns whether anything changed.
    ///
    /// A motion that the bounds absorb entirely — dollying further out at the
    /// limit — changes nothing and is reported as such, so holding the scroll
    /// wheel against the stop does not accumulate revisions.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError`] when the motion names no reachable pose, leaving
    /// the view exactly as it was.
    pub fn move_camera(&mut self, motion: CameraMotion) -> Result<bool, ViewError> {
        Ok(self.adopt_candidate(self.view.moved(motion)?))
    }

    /// Adopt `candidate` if it differs, advancing the revision if it does.
    ///
    /// One place, so every setter answers the "did anything actually change"
    /// question the same way. Comparing the whole *view* rather than the field
    /// that was set is what makes a control clicked twice, or a gesture the
    /// bounds absorbed, cost nothing.
    fn adopt_candidate(&mut self, candidate: AuthoringView) -> bool {
        if self.view == candidate {
            return false;
        }
        self.view = candidate;
        self.revision = self.revision.next();
        true
    }

    /// Adopt `view` as the saved state of a document just opened or created.
    ///
    /// The revision moves *forward* onto the next one rather than back to the
    /// initial, for the reason [`crate::DocumentAuthority`] rebases an opened
    /// experiment forward: a renderer that has already caught up must never be
    /// told it is now at an earlier revision. Nothing in the file names a view
    /// revision — the section carries the view, not its history — so there is
    /// no persisted counter to rewind.
    pub fn adopt(&mut self, view: AuthoringView) {
        self.view = view;
        self.revision = self.revision.next();
        self.clean_revision = self.revision;
        self.acknowledged_revision = None;
    }

    /// Acknowledge that `revision` of the view was successfully written.
    ///
    /// The counterpart to
    /// [`DocumentAuthority::acknowledge_save`](crate::DocumentAuthority::acknowledge_save),
    /// and load-bearing for the same reason: a save is asynchronous, so the
    /// camera can have moved between the revision being captured and the bytes
    /// landing. Only an acknowledgement naming the revision still in force
    /// makes the view clean.
    ///
    /// No target is adopted here. Where the document lives is one fact about
    /// the file, and the authority owns it.
    pub fn acknowledge_save(
        &mut self,
        revision: ViewRevision,
    ) -> SaveAcknowledgement<ViewRevision> {
        if let Some(acknowledged) = self.acknowledged_revision
            && revision < acknowledged
        {
            return SaveAcknowledgement::Stale { acknowledged };
        }

        self.acknowledged_revision = Some(revision);
        if revision == self.revision {
            self.clean_revision = revision;
            SaveAcknowledgement::Clean
        } else {
            SaveAcknowledgement::Superseded {
                current: self.revision,
            }
        }
    }
}

/// Bring an angle into `(-π, π]`.
fn normalise_angle(radians: f64) -> f64 {
    use std::f64::consts::{PI, TAU};

    let wrapped = radians.rem_euclid(TAU);
    if wrapped > PI { wrapped - TAU } else { wrapped }
}

fn unrepresentable(source: GeometryError) -> ViewError {
    ViewError::Unrepresentable { source }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A render-unit value, panicking if the conversion has no answer.
    fn render(scale: SceneScale, metres: f64) -> f64 {
        scale
            .to_render(metres)
            .expect("the tested values are inside what the scale can convert")
    }

    /// Whether a view change actually changed anything.
    fn applied(outcome: Result<ViewChange, ViewError>) -> bool {
        outcome.expect("the change is accepted").changed
    }

    fn pose(distance: f64, yaw: f64, pitch: f64) -> CameraPose {
        CameraPose::new(Vector3::ZERO, distance, yaw, pitch).expect("finite components")
    }

    /// A view at `scale` looking at the origin from `distance` metres.
    fn view_at(scale: SceneScale, distance: f64) -> AuthoringView {
        AuthoringView::new(Projection::Perspective, scale, pose(distance, 0.0, 0.0))
            .expect("representable")
    }

    #[test]
    fn a_pose_refuses_what_names_no_camera_at_any_scale() {
        assert_eq!(
            CameraPose::new(Vector3::ZERO, f64::NAN, 0.0, 0.0),
            Err(ViewError::NotFinite { what: "distance" })
        );
        assert_eq!(
            CameraPose::new(Vector3::ZERO, 1.0, f64::INFINITY, 0.0),
            Err(ViewError::NotFinite { what: "yaw" })
        );
        // An eye coincident with its target has no view direction.
        assert!(CameraPose::new(Vector3::ZERO, 0.0, 0.0, 0.0).is_err());
        assert!(CameraPose::new(Vector3::ZERO, -1.0, 0.0, 0.0).is_err());

        // Pitch is scale-free, so the pose still owns that bound.
        assert_eq!(pose(10.0, 0.0, 10.0).pitch(), MAX_CAMERA_PITCH);
    }

    #[test]
    fn the_camera_reaches_a_molecule_and_frames_an_orbit() {
        // K14's whole purpose, as one assertion pair. Under absolute metre
        // bounds a 2 nm object could not be approached (the camera stopped a
        // metre short) and a planetary orbit could not be framed (the camera
        // could not retreat past two kilometres). Both work now, in the same
        // build, with no code change — only a scale.
        let molecular = view_at(SceneScale::NANOMETRE, 2.0e-9);
        assert!(
            molecular.camera().distance() <= 1.0e-8,
            "a nanometre-scale camera must sit nanometres away, not metres: {} m",
            molecular.camera().distance()
        );

        let orbital = view_at(SceneScale::ASTRONOMICAL_UNIT, 5.0 * 1.495_978_707e11);
        assert!(
            orbital.camera().distance() >= 1.0e11,
            "an AU-scale camera must be able to sit astronomical units away: {} m",
            orbital.camera().distance()
        );
    }

    #[test]
    fn the_reachable_window_is_the_same_in_render_units_at_every_scale() {
        // The invariant that replaces the old absolute constants: what the
        // camera can reach is fixed in render units, and the scale decides
        // what a render unit is worth.
        for scale in [
            SceneScale::NANOMETRE,
            SceneScale::METRE,
            SceneScale::ASTRONOMICAL_UNIT,
            SceneScale::LIGHT_YEAR,
        ] {
            let far = view_at(scale, f64::MAX / 4.0);
            let near = view_at(scale, 0.0_f64.next_up());
            assert!(
                (render(scale, far.camera().distance()) - MAX_CAMERA_DISTANCE_UNITS).abs() < 1e-6,
                "{scale:?}"
            );
            assert!(
                (render(scale, near.camera().distance()) - MIN_CAMERA_DISTANCE_UNITS).abs() < 1e-6,
                "{scale:?}"
            );
        }
    }

    #[test]
    fn a_target_beyond_the_renderable_range_is_brought_back_at_its_scale() {
        // Regression from K11: 1e40 is a perfectly finite f64 and passed every
        // other check, then became infinity when the renderer narrowed it to
        // f32. The bound is now in render units, so what "too far" means
        // follows the scale instead of being an arbitrary metre count.
        let far = Vector3::new(1e40, -1e40, 5.0).expect("finite f64");
        let view = AuthoringView::new(
            Projection::Perspective,
            SceneScale::METRE,
            CameraPose::new(far, 18.0, 0.0, 0.0).expect("names a camera"),
        )
        .expect("bounded");

        assert_eq!(view.camera().target().x(), MAX_CAMERA_TARGET_UNITS);
        assert_eq!(view.camera().target().y(), -MAX_CAMERA_TARGET_UNITS);
        assert_eq!(
            view.camera().target().z(),
            5.0,
            "an in-range axis is untouched"
        );

        // The same target is *inside* the bound at astronomical-unit scale,
        // because there it is only a few hundred thousand render units out.
        let reachable = AuthoringView::new(
            Projection::Perspective,
            SceneScale::LIGHT_YEAR,
            CameraPose::new(
                Vector3::new(1e20, 0.0, 0.0).expect("finite"),
                1e17,
                0.0,
                0.0,
            )
            .expect("names a camera"),
        )
        .expect("bounded");
        assert_eq!(
            reachable.camera().target().x(),
            1e20,
            "a light-year scale reaches where a metre scale cannot"
        );
    }

    #[test]
    fn every_scale_narrows_to_a_finite_render_value() {
        // The property that actually matters downstream: whatever the scale and
        // wherever the camera, what the renderer receives is finite.
        for scale in SceneScale::PRESETS.iter().copied() {
            let view = AuthoringView::new(
                Projection::Perspective,
                scale,
                CameraPose::new(
                    Vector3::new(1e40, -1e40, 1e40).expect("finite"),
                    f64::MAX / 4.0,
                    0.4,
                    0.3,
                )
                .expect("names a camera"),
            )
            .expect("bounded");

            let camera = view.camera();
            for component in camera.eye().expect("representable").to_array() {
                assert!(
                    (render(scale, component) as f32).is_finite(),
                    "{scale:?}: {component}"
                );
            }
            assert!(
                (render(scale, camera.distance()) as f32).is_finite(),
                "{scale:?}"
            );
        }
    }

    #[test]
    fn yaw_is_normalised_so_a_full_turn_compares_equal() {
        let turned = pose(10.0, std::f64::consts::TAU, 0.0);
        assert!(turned.yaw().abs() < 1e-9, "{}", turned.yaw());
        assert_eq!(pose(10.0, 0.0, 0.0), turned);
    }

    #[test]
    fn the_eye_sits_at_the_orbit_distance_from_the_target() {
        let camera = pose(25.0, 0.7, 0.3);
        let eye = camera.eye().expect("representable");
        let offset = (eye.x().powi(2) + eye.y().powi(2) + eye.z().powi(2)).sqrt();
        assert!((offset - 25.0).abs() < 1e-9, "{offset}");
    }

    #[test]
    fn a_pan_moves_the_target_perpendicular_to_the_view_direction() {
        // Looking along -x from +x with no elevation: panning horizontally
        // must move the target along y and leave x and z alone.
        let panned = view_at(SceneScale::METRE, 10.0)
            .moved(CameraMotion::Pan { dx: 10.0, dy: 0.0 })
            .expect("representable");
        let target = panned.camera().target();
        assert!(target.x().abs() < 1e-12, "{target:?}");
        assert!(target.y().abs() > 0.0, "{target:?}");
        assert!(target.z().abs() < 1e-12, "{target:?}");
    }

    #[test]
    fn a_pan_step_follows_the_scale_without_referring_to_it() {
        // Pan is a fraction of the orbit distance, so it is scale-relative for
        // free: the same gesture moves nanometres near a molecule and
        // astronomical units near an orbit, and neither number is written down
        // anywhere.
        let step_at = |scale: SceneScale, distance: f64| {
            view_at(scale, distance)
                .moved(CameraMotion::Pan { dx: 10.0, dy: 0.0 })
                .expect("representable")
                .camera()
                .target()
                .length()
        };
        let molecular = step_at(SceneScale::NANOMETRE, 1.0e-8);
        let orbital = step_at(SceneScale::ASTRONOMICAL_UNIT, 1.0e12);

        assert!(molecular < 1.0e-9, "{molecular}");
        assert!(orbital > 1.0e9, "{orbital}");
    }

    #[test]
    fn a_pan_at_the_pitch_limit_is_still_defined() {
        // The reason MAX_CAMERA_PITCH is short of a right angle: at exactly
        // 90° the horizontal basis degenerates and pan has no direction.
        let view = AuthoringView::new(
            Projection::Perspective,
            SceneScale::METRE,
            pose(10.0, 0.3, MAX_CAMERA_PITCH),
        )
        .expect("representable");
        let panned = view
            .moved(CameraMotion::Pan { dx: 5.0, dy: 5.0 })
            .expect("representable");
        assert_ne!(panned.camera().target(), view.camera().target());
    }

    #[test]
    fn a_motion_absorbed_by_the_bounds_changes_nothing() {
        let mut state = AuthoringViewState::new();
        // Dolly out until it stops moving, then keep going.
        for _ in 0..500 {
            let _ = state.move_camera(CameraMotion::Dolly { amount: -1.0 });
        }
        assert_eq!(
            state.camera().distance(),
            MAX_CAMERA_DISTANCE_UNITS * SceneScale::METRE.metres()
        );
        let revision = state.revision();
        assert_eq!(
            state.move_camera(CameraMotion::Dolly { amount: -1.0 }),
            Ok(false),
            "a motion the bounds absorb must not advance a revision"
        );
        assert_eq!(state.revision(), revision);
    }

    #[test]
    fn changing_the_scale_rebounds_the_camera_in_one_revision() {
        let mut state = AuthoringViewState::new();
        // Retreat to the metre-scale limit, then switch to nanometre scale.
        for _ in 0..500 {
            let _ = state.move_camera(CameraMotion::Dolly { amount: -1.0 });
        }
        assert_eq!(state.camera().distance(), 2_000.0);
        let before = state.revision();

        assert!(applied(state.set_scale(SceneScale::NANOMETRE)));

        assert_eq!(
            state.revision(),
            before.next(),
            "re-bounding is part of the same change, not a second one"
        );
        // Two kilometres is unreachable at nanometre scale, so the camera comes
        // back rather than being left pointing somewhere unrenderable. Compared
        // against the derived bound rather than a written-out number, because
        // the bound *is* the product and reproducing it as a literal is how a
        // test starts disagreeing with the thing it checks.
        assert_eq!(
            state.camera().distance(),
            MAX_CAMERA_DISTANCE_UNITS * SceneScale::NANOMETRE.metres()
        );
        assert_eq!(state.scale(), SceneScale::NANOMETRE);
    }

    #[test]
    fn choosing_the_active_scale_again_changes_nothing() {
        let mut state = AuthoringViewState::new();
        assert_eq!(state.scale(), SceneScale::METRE);
        assert!(!applied(state.set_scale(SceneScale::METRE)));
        assert!(!state.is_dirty(), "a control clicked twice is not a change");
    }

    #[test]
    fn a_scale_must_be_a_finite_positive_length() {
        for refused in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let error = SceneScale::from_metres(refused).expect_err("not a scale");
            assert_eq!(error.code(), "scale_not_positive", "{refused}");
        }
        assert_eq!(SceneScale::from_metres(1.0e-9), Ok(SceneScale::NANOMETRE));
    }

    #[test]
    fn a_finite_positive_scale_can_still_be_unsupported() {
        // Not every finite ratio is usable: at 1e-320 metres per unit, dividing
        // an ordinary coordinate by the scale overflows. Named as its own
        // refusal rather than accepted into a conversion that cannot answer.
        for refused in [f64::MIN_POSITIVE, 1e-320, 1e-31, 1e31, f64::MAX] {
            let error = SceneScale::from_metres(refused).expect_err("unsupported");
            assert_eq!(error.code(), "scale_out_of_range", "{refused:e}");
        }
        // Both ends of the supported range are accepted, and both convert.
        for accepted in [MIN_SCENE_SCALE, MAX_SCENE_SCALE] {
            let scale = SceneScale::from_metres(accepted).expect("supported");
            assert!(scale.to_render(accepted).is_some());
            assert!(scale.to_world(MAX_CAMERA_TARGET_UNITS).is_some());
            assert!(
                scale
                    .to_render(MAX_CAMERA_TARGET_UNITS * accepted)
                    .is_some()
            );
        }
        // And a file cannot smuggle one past the bound either.
        assert!(serde_json::from_str::<SceneScale>("1e-320").is_err());
        assert!(serde_json::from_str::<SceneScale>("1e31").is_err());
    }

    #[test]
    fn a_typed_scale_refuses_an_unknown_symbol_and_an_oversized_expression() {
        // Reusing the one engine means inheriting its diagnostics *and* its
        // bounds, which is most of the reason not to write a second parser.
        assert_eq!(
            SceneScale::parse("1 furlong")
                .expect_err("no such unit or variable")
                .code(),
            "scale_unparseable"
        );
        assert_eq!(
            SceneScale::parse("")
                .expect_err("nothing to evaluate")
                .code(),
            "scale_unparseable"
        );
        // Over the engine's own budget rather than over a limit invented here.
        let deep = format!("{}1 m{}", "(".repeat(4_096), ")".repeat(4_096));
        assert_eq!(
            SceneScale::parse(&deep).expect_err("over budget").code(),
            "scale_unparseable"
        );
        // A scale that parses to something unsupported reports *that*, so the
        // two kinds of refusal stay distinguishable.
        assert_eq!(
            SceneScale::parse("1e-320 m")
                .expect_err("unsupported")
                .code(),
            "scale_out_of_range"
        );
    }

    #[test]
    fn an_accepted_scale_change_moves_the_distance_and_nothing_else() {
        // Two cases, and in both the target and the orientation are untouched:
        // a scale change is not a move. Only the distance can change, and only
        // because the reachable window did.
        let camera = CameraPose::new(
            Vector3::new(3.0, -4.0, 5.0).expect("finite"),
            50.0,
            0.4,
            0.3,
        )
        .expect("names a camera");

        // Inside the new window: nothing at all changes about the pose. Note
        // the presets are three orders of magnitude apart and each window is
        // 2000 units wide, so a *preset* change nearly always re-bounds — the
        // untouched case needs a nearby custom scale, which is exactly what the
        // typed field is for.
        let mut state = AuthoringViewState::new();
        assert!(applied(state.set_camera(camera)));
        let nearby = SceneScale::from_metres(2.0).expect("supported");
        assert!(applied(state.set_scale(nearby)));
        assert_eq!(state.camera(), camera, "50 m is inside 2 m/unit's window");

        // Outside it: the distance is brought to the limit, and the target and
        // orientation still do not move.
        let mut state = AuthoringViewState::new();
        assert!(applied(state.set_camera(camera)));
        assert!(applied(state.set_scale(SceneScale::KILOMETRE)));

        let after = state.camera();
        assert_eq!(
            after.target(),
            camera.target(),
            "a scale change is not a move"
        );
        assert_eq!(after.yaw(), camera.yaw());
        assert_eq!(after.pitch(), camera.pitch());
        assert_eq!(
            after.distance(),
            MIN_CAMERA_DISTANCE_UNITS * SceneScale::KILOMETRE.metres(),
            "50 m is nearer than kilometre scale can resolve, so it comes to the limit"
        );
    }

    #[test]
    fn an_incompatible_scale_is_refused_rather_than_moving_the_focus() {
        // Regression: switching a camera focused a metre out to nanometre scale
        // *succeeded* and slid the focus to a millimetre out. Someone would
        // have been left looking at a different place with nothing to tell
        // them anything had moved.
        let mut state = AuthoringViewState::new();
        let camera = CameraPose::new(Vector3::new(1.0, 0.0, 0.0).expect("finite"), 18.0, 0.4, 0.3)
            .expect("names a camera");
        assert!(applied(state.set_camera(camera)));
        let before = state.view();
        let revision = state.revision();

        let refusal = state
            .set_scale(SceneScale::NANOMETRE)
            .expect_err("a metre-out focus is unreachable at nanometre scale");
        assert_eq!(refusal.code(), "focus_out_of_reach");
        // The message has to say what to do about it, since the remedy is not
        // obvious from "out of reach".
        assert!(
            refusal.to_string().contains("nearer the origin"),
            "{refusal}"
        );

        assert_eq!(state.view(), before, "a refusal changes nothing");
        assert_eq!(state.revision(), revision);
    }

    #[test]
    fn a_file_and_a_gesture_still_clamp_the_focus_rather_than_refusing() {
        // The other half of the policy. Refusing on these paths would let a
        // saved camera stop an experiment from opening, and would make a pan
        // held against the limit an error instead of a stop.
        let far = Vector3::new(1e12, 0.0, 0.0).expect("finite");
        let view = AuthoringView::new(
            Projection::Perspective,
            SceneScale::NANOMETRE,
            CameraPose::new(far, 1e-8, 0.0, 0.0).expect("names a camera"),
        )
        .expect("a decoded camera never blocks a load");
        assert_eq!(
            view.camera().target().x(),
            MAX_CAMERA_TARGET_UNITS * SceneScale::NANOMETRE.metres()
        );

        // And a pan that reaches the edge stops there.
        let mut at_edge = view;
        for _ in 0..200 {
            at_edge = at_edge
                .moved(CameraMotion::Pan { dx: -1e6, dy: 0.0 })
                .expect("a gesture is total");
        }
        assert_eq!(
            at_edge.camera().target().x(),
            MAX_CAMERA_TARGET_UNITS * SceneScale::NANOMETRE.metres()
        );
    }

    #[test]
    fn an_automatic_distance_adjustment_is_reported() {
        // Regression: selecting nanometre scale took the default camera from
        // eighteen metres to two micrometres — twelve orders of magnitude —
        // and said nothing at all.
        let mut state = AuthoringViewState::new();
        let outcome = state
            .set_scale(SceneScale::NANOMETRE)
            .expect("the default focus is at the origin, so this is reachable");

        assert!(outcome.changed);
        assert_eq!(
            outcome.adjustment,
            ViewAdjustment::Distance {
                from: 18.0,
                to: MAX_CAMERA_DISTANCE_UNITS * SceneScale::NANOMETRE.metres(),
            }
        );
        let message = outcome
            .adjustment
            .message()
            .expect("an adjustment this large must be reported");
        assert!(message.contains("Camera distance adjusted"), "{message}");

        // A change that needed no adjustment says nothing.
        let mut state = AuthoringViewState::new();
        let outcome = state
            .set_scale(SceneScale::from_metres(2.0).expect("supported"))
            .expect("reachable");
        assert_eq!(outcome.adjustment, ViewAdjustment::None);
        assert_eq!(outcome.adjustment.message(), None);
    }

    #[test]
    fn forward_conversion_does_not_leak_an_infinity_for_finite_input() {
        // Regression: an accepted scale of 1e-30 turned a finite `f64::MAX`
        // into infinity, and the API had no way to say so. Geometry is
        // *omitted* when it has no render-space answer rather than pinned to a
        // boundary it does not occupy.
        let scale = SceneScale::from_metres(1e-30).expect("supported");
        assert_eq!(scale.to_render(f64::MAX), None);
        assert_eq!(scale.to_render(f64::INFINITY), None);
        assert_eq!(scale.to_render(f64::NAN), None);

        // What is inside the reachable range still converts, at both extremes
        // of the supported scale range.
        for metres in [MIN_SCENE_SCALE, MAX_SCENE_SCALE] {
            let scale = SceneScale::from_metres(metres).expect("supported");
            let reachable = MAX_CAMERA_TARGET_UNITS * metres;
            assert!(scale.to_render(reachable).is_some(), "{metres:e}");
        }
    }

    #[test]
    fn a_refused_scale_change_leaves_the_view_untouched() {
        let mut state = AuthoringViewState::new();
        assert!(state.set_projection(Projection::Orthographic));
        let before = state.view();
        let revision = state.revision();
        let clean = state.is_dirty();

        // Unsupported scales cannot even be constructed, so the refusal a
        // caller actually meets is on the way in. Prove the state machine is
        // unmoved by the attempt through the parsing path the UI uses.
        assert!(SceneScale::parse("0 m").is_err());
        assert!(SceneScale::parse("1e-320 m").is_err());

        assert_eq!(state.view(), before);
        assert_eq!(state.revision(), revision);
        assert_eq!(state.is_dirty(), clean);
    }

    #[test]
    fn a_scale_reads_from_typed_text_through_the_one_expression_engine() {
        // Unit-bearing entries, because `orishu_variables` treats a unit as
        // part of the language rather than something a second parser handles.
        assert_eq!(SceneScale::parse("1 nm"), Ok(SceneScale::NANOMETRE));
        assert_eq!(SceneScale::parse("1nm"), Ok(SceneScale::NANOMETRE));
        assert_eq!(SceneScale::parse("1 km"), Ok(SceneScale::KILOMETRE));
        assert_eq!(SceneScale::parse("1 AU"), Ok(SceneScale::ASTRONOMICAL_UNIT));
        // Arithmetic comes free with the engine.
        assert_eq!(SceneScale::parse("2 m / 2"), Ok(SceneScale::METRE));
        // A bare number is metres, the one convenience worth having.
        assert_eq!(SceneScale::parse("1000"), Ok(SceneScale::KILOMETRE));

        // Another dimension is a mistake, not a shorthand for metres.
        assert_eq!(
            SceneScale::parse("1 kg").expect_err("not a length").code(),
            "scale_not_a_length"
        );
        assert_eq!(
            SceneScale::parse("not a number")
                .expect_err("not an expression")
                .code(),
            "scale_unparseable"
        );
        assert_eq!(
            SceneScale::parse("-1 m")
                .expect_err("not a positive length")
                .code(),
            "scale_not_positive"
        );

        // Only shared units and constants resolve: a scale cannot become a
        // dependency on a document variable that might later change.
        assert!(SceneScale::parse("mass_of_sun").is_err());

        // And a caller can be stricter than the default.
        // Three bytes admits `1 m` and refuses the four-byte `1 nm`.
        let strict = orishu_variables::Limits {
            max_expression_bytes: 3,
            ..orishu_variables::Limits::DEFAULT
        };
        assert_eq!(
            SceneScale::parse_bounded("1 nm", &strict)
                .expect_err("over the caller's own budget")
                .code(),
            "scale_unparseable"
        );
        assert_eq!(
            SceneScale::parse_bounded("1 m", &strict),
            Ok(SceneScale::METRE)
        );
    }

    #[test]
    fn a_scale_reports_its_preset_or_says_it_is_custom() {
        assert_eq!(SceneScale::METRE.label(), "Metre");
        assert_eq!(SceneScale::ASTRONOMICAL_UNIT.label(), "Astronomical unit");
        let custom = SceneScale::from_metres(2.5).expect("positive");
        assert_eq!(custom.label(), "Custom");
        assert_eq!(
            custom.to_string(),
            "2.5 m/unit",
            "a custom value shows as a canonical-SI quantity, not a bare float"
        );
        // A hair off a preset is a custom scale and must not claim otherwise.
        let nearly = SceneScale::from_metres(1.0 + f64::EPSILON).expect("positive");
        assert_eq!(nearly.label(), "Custom");

        // The shared unit table has `AU` but no `ly`, so the light-year preset
        // carries its value explicitly and cannot be *typed* as a unit. Pinned
        // rather than left ambiguous: adding `ly` belongs in the shared table
        // with its own tests, never in a private one here.
        assert_eq!(SceneScale::LIGHT_YEAR.metres(), 9.460_730_472_580_8e15);
        assert!(SceneScale::parse("1 ly").is_err());
        assert_eq!(
            SceneScale::parse("9.4607304725808e15 m"),
            Ok(SceneScale::LIGHT_YEAR),
            "the value is reachable by writing it out"
        );
    }

    #[test]
    fn conversion_divides_before_the_narrowing_matters() {
        // The mechanism, as the property it buys: a 2 nm radius cast straight
        // to f32 is 2e-9 and vanishes into the near plane; divided by a
        // nanometre scale first it is 2.0, which every fixed limit in the
        // renderer is the right size for.
        assert_eq!(SceneScale::NANOMETRE.to_render(2.0e-9), Some(2.0));
        assert_eq!(
            SceneScale::METRE.to_render(12.5),
            Some(12.5),
            "the identity"
        );
        assert_eq!(
            SceneScale::ASTRONOMICAL_UNIT.to_render(3.0 * 1.495_978_707e11),
            Some(3.0)
        );
        // And it round-trips.
        let scale = SceneScale::MICROMETRE;
        assert_eq!(scale.to_world(render(scale, 4.0e-6)), Some(4.0e-6));

        // The inverse refuses rather than handing back an infinity, because
        // this is the direction that produces authored candidates.
        assert_eq!(SceneScale::LIGHT_YEAR.to_world(f64::MAX), None);
        assert_eq!(SceneScale::METRE.to_world(f64::INFINITY), None);
        assert_eq!(SceneScale::METRE.to_world(f64::NAN), None);
        assert_eq!(
            SceneScale::LIGHT_YEAR.to_world(MAX_CAMERA_TARGET_UNITS),
            Some(MAX_CAMERA_TARGET_UNITS * SceneScale::LIGHT_YEAR.metres()),
            "the whole reachable range at the largest preset is still finite"
        );
    }

    #[test]
    fn a_new_view_is_clean_and_choosing_the_same_projection_keeps_it_so() {
        let mut state = AuthoringViewState::new();
        assert!(!state.is_dirty());
        assert_eq!(state.projection(), Projection::Perspective);

        assert!(!state.set_projection(Projection::Perspective));
        assert!(!state.is_dirty(), "a no-op control must not dirty a file");

        assert!(state.set_projection(Projection::Orthographic));
        assert!(state.is_dirty());
        assert_eq!(state.revision(), ViewRevision::INITIAL.next());
    }

    #[test]
    fn only_an_acknowledgement_of_the_current_revision_is_clean() {
        let mut state = AuthoringViewState::new();
        assert!(state.set_projection(Projection::Orthographic));
        let captured = state.revision();

        // The camera moved while the write was in flight.
        assert_eq!(
            state.move_camera(CameraMotion::Orbit { dx: 40.0, dy: 0.0 }),
            Ok(true)
        );
        let acknowledgement = state.acknowledge_save(captured);
        assert_eq!(
            acknowledgement,
            SaveAcknowledgement::Superseded {
                current: state.revision(),
            }
        );
        assert!(
            state.is_dirty(),
            "a completion for an older view must leave the newer one dirty"
        );

        let current = state.revision();
        assert_eq!(state.acknowledge_save(current), SaveAcknowledgement::Clean);
        assert!(!state.is_dirty());
    }

    #[test]
    fn an_out_of_order_acknowledgement_changes_nothing() {
        let mut state = AuthoringViewState::new();
        assert!(state.set_projection(Projection::Orthographic));
        let first = state.revision();
        assert!(state.set_projection(Projection::Perspective));
        let second = state.revision();

        assert_eq!(state.acknowledge_save(second), SaveAcknowledgement::Clean);
        assert_eq!(
            state.acknowledge_save(first),
            SaveAcknowledgement::Stale {
                acknowledged: second
            }
        );
        assert!(!state.is_dirty(), "the stale completion moved nothing back");
    }

    #[test]
    fn adopting_an_opened_view_is_clean_and_moves_the_revision_forward() {
        let mut state = AuthoringViewState::new();
        assert!(state.set_projection(Projection::Orthographic));
        let before = state.revision();

        state.adopt(
            AuthoringView::new(
                Projection::Perspective,
                SceneScale::METRE,
                pose(42.0, 0.1, 0.2),
            )
            .expect("representable"),
        );

        assert!(!state.is_dirty(), "what is here is what is on disk");
        assert!(
            state.revision() > before,
            "a view that had caught up must never be told it is at an earlier revision"
        );
        assert_eq!(state.camera().distance(), 42.0);
    }

    #[test]
    fn a_pose_round_trips_through_serde_and_is_revalidated() {
        let camera = pose(25.0, 0.5, 0.25);
        let encoded = serde_json::to_string(&camera).expect("encodes");
        assert_eq!(
            serde_json::from_str::<CameraPose>(&encoded).expect("decodes"),
            camera
        );

        // A pose decodes with its scale-free invariants enforced. The
        // *reachable* range is not among them, because a file's pose is only
        // out of range relative to the scale saved beside it — which is
        // `AuthoringView`'s to know, and is checked when the section decodes.
        let far = r#"{"target":[0,0,0],"distance":1e9,"yaw":0,"pitch":0}"#;
        assert_eq!(
            serde_json::from_str::<CameraPose>(far)
                .expect("decodes")
                .distance(),
            1e9
        );
        for refused in [
            r#"{"target":[0,0,0],"distance":null,"yaw":0,"pitch":0}"#,
            r#"{"target":[0,0,0],"distance":0,"yaw":0,"pitch":0}"#,
            r#"{"target":[0,0,0],"distance":-5,"yaw":0,"pitch":0}"#,
            r#"{"target":[0,0,0],"distance":1e400,"yaw":0,"pitch":0}"#,
        ] {
            assert!(
                serde_json::from_str::<CameraPose>(refused).is_err(),
                "{refused}"
            );
        }
        let extra = r#"{"target":[0,0,0],"distance":1,"yaw":0,"pitch":0,"roll":1}"#;
        assert!(
            serde_json::from_str::<CameraPose>(extra).is_err(),
            "an unknown field means a producer this build does not understand"
        );
    }

    #[test]
    fn a_scale_round_trips_through_serde_and_is_revalidated() {
        let encoded = serde_json::to_string(&SceneScale::NANOMETRE).expect("encodes");
        assert_eq!(encoded, "1e-9");
        assert_eq!(
            serde_json::from_str::<SceneScale>(&encoded).expect("decodes"),
            SceneScale::NANOMETRE
        );

        // The bound belongs to the type, so a file cannot widen it.
        for refused in ["0", "-1", "null", "\"1 nm\""] {
            assert!(
                serde_json::from_str::<SceneScale>(refused).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_projection_round_trips_as_its_own_name() {
        let encoded = serde_json::to_string(&Projection::Orthographic).expect("encodes");
        assert_eq!(encoded, "\"orthographic\"");
        assert_eq!(
            serde_json::from_str::<Projection>(&encoded).expect("decodes"),
            Projection::Orthographic
        );
        assert_eq!(Projection::Perspective.other(), Projection::Orthographic);
    }
}
