//! The `kagami.experiment` file: a versioned, pure codec.
//!
//! Under ADR 0012 the first collaboration model *is* sending someone a file, so
//! this format is the sharing protocol and is treated as one. It is not the
//! adapter representation in [`crate::wire`] — that describes a request, this
//! describes a durable artefact — and it is not the workload bundle, which is
//! compilation output.
//!
//! # Version policy
//!
//! [`FORMAT`] is checked before anything else is interpreted, then
//! [`FORMAT_VERSION`]. That ordering is why [`decode_document`] exists rather
//! than a bare `serde_json::from_slice`: it reads the two deciding fields from
//! a permissive header first, and only then hands the bytes to the DTO for
//! that version. A version *higher* than this build is refused outright rather
//! than partially read — a newer producer may mean something different by a
//! field this build recognises, and half-understanding a document is worse than
//! declining it. There is no version 0 and no "lower is probably safe" rule;
//! each older version gets an explicit conversion of its own.
//!
//! Unknown fields are refused. A producer that added a field without advancing
//! the version would otherwise have its content silently dropped on the next
//! re-save, which is exactly the failure a shared-file workflow cannot afford.
//! It is also why version 2 exists: adding ADR 0022's `defaultView` section
//! under version 1 would have made a build from before it reject the file as
//! *malformed*, when the whole purpose of the version field is to say
//! "written by something newer" instead.
//!
//! # What is and is not in the file
//!
//! Persisted: identities and the counters that minted them, component types
//! and the schema versions their values were checked against, **authored
//! expression source**, variable definitions with their identities and
//! namespaces, the setup, and the enabled plugins.
//!
//! Not persisted: resolved magnitudes. Decoding re-derives every one of them
//! from the source, so a file cannot inject an SI value its own expression does
//! not produce. Nor is any presentation state here beyond ADR 0022's separately
//! versioned default view, and no run observation or record at all.
//!
//! # The default view has the opposite version policy, deliberately
//!
//! [`StoredDefaultView`] carries its own version, and one this build cannot
//! read is *reported and dropped* while the experiment opens normally. That is
//! the exact inverse of the envelope rule above, and it is what "separately
//! versioned" is for: a camera must never be a reason a colleague cannot open
//! an experiment. The loss is not silent — [`StoredDefaultView::decode`]
//! returns why, so the window can say that saving will replace the view it
//! could not read.
//!
//! # No IO, no clock
//!
//! Encoding takes the metadata it stamps as an argument, so the same authored
//! state and the same metadata encode to the same bytes and a round trip is
//! testable as a value. Writing those bytes durably is the shell's.

use std::collections::BTreeMap;

use kagami_catalog::{
    ComponentTypeId, PropertyName, SchemaRegistry, SchemaVersion, TemplateProvenance,
};
use kagami_document::{
    ComponentRecord, DocumentRecord, Experiment, ExperimentSnapshot, Limits, ObjectRecord,
    ObjectShape, Rejection, Setup, Transform, VariableRecord, Velocity, hydrate,
};
use orishu_variables::{Name, Namespace};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::default_view::{
    AuthoringView, CameraPose, DEFAULT_VIEW_VERSION, Projection, SceneScale,
};

/// The format identifier every document carries.
pub const FORMAT: &str = "kagami.experiment";

/// The format version this build writes.
///
/// Version 2 added ADR 0022's `defaultView` section. Version 1 still loads,
/// through its own explicit conversion in [`decode_document`], and converts
/// *up*: an opened version-1 document is a version-2 value in memory, so a
/// re-save writes version 2.
pub const FORMAT_VERSION: u32 = 2;

/// The oldest format version this build still reads.
pub const MIN_FORMAT_VERSION: u32 = 1;

/// Why a document could not be decoded.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum DocumentError {
    /// The bytes are not the JSON this format is written in.
    #[error("document is not well-formed JSON: {message}")]
    Malformed {
        /// What the decoder objected to, including where.
        message: String,
    },
    /// The document is not a Kagami experiment at all.
    #[error("document declares format `{found}`, not `{expected}`")]
    WrongFormat {
        /// The identifier the document carried.
        found: String,
        /// The identifier this build reads.
        expected: &'static str,
    },
    /// The document was written by a newer build.
    ///
    /// Refused rather than partially interpreted: this build cannot know what
    /// a later version means by a field it happens to recognise.
    #[error("document is format version {found}, and this build reads at most {supported}")]
    UnsupportedVersion {
        /// The version the document carried.
        found: u32,
        /// The newest version this build reads.
        supported: u32,
    },
    /// The document's contents are not a valid experiment.
    #[error(transparent)]
    Invalid {
        /// Why the model refused it.
        #[from]
        source: Rejection,
    },
}

impl DocumentError {
    /// A stable identifier for this reason.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed { .. } => "malformed_document",
            Self::WrongFormat { .. } => "wrong_format",
            Self::UnsupportedVersion { .. } => "unsupported_format_version",
            Self::Invalid { source } => source.code(),
        }
    }

    /// `true` when the bytes are *damaged* rather than merely declined.
    ///
    /// The distinction the recovery protocol turns on, and the two cases could
    /// not be more different in what they ask for:
    ///
    /// - **Damage** — truncated or garbled bytes — means an earlier save was
    ///   interrupted. Falling back to the backup is the whole point of keeping
    ///   one, and replacing the damaged primary on the next save loses nothing.
    /// - **Declined** — a newer format version, or another format entirely —
    ///   means the file is intact and something else understands it. Treating
    ///   that as damage would open an *older* backup and then overwrite the
    ///   newer document with it, which is data loss dressed up as recovery.
    ///
    /// So [`crate::store::load`] falls back only for damage, and
    /// [`crate::store::save`] preserves anything that is not damage as the
    /// backup before replacing it.
    pub const fn is_damage(&self) -> bool {
        match self {
            Self::Malformed { .. } => true,
            // Well-formed, and a document. This build just will not read it.
            Self::WrongFormat { .. } | Self::UnsupportedVersion { .. } => false,
            // Structurally a document; the model refused its contents. Not
            // this build's to overwrite either.
            Self::Invalid { .. } => false,
        }
    }
}

/// What a save stamps on the file, supplied by the shell.
///
/// An argument rather than something read here, so the codec has no clock and
/// two encodes of the same state agree byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DocumentMetadata {
    /// What wrote the file. For support, never for parsing.
    pub generator: String,
    /// When the experiment was first saved, preserved across re-saves.
    pub created: String,
    /// When this particular save happened.
    pub saved: String,
    /// The revision the shell captured for this save.
    ///
    /// Provenance about the *save*, which is why it is here and not in the
    /// experiment: opening establishes a fresh history, so a revision read
    /// back from a file would describe a session that no longer exists. Keeping
    /// it out of the authored section is also what makes re-encoding
    /// unchanged content byte-identical, so a shared file does not churn.
    pub saved_revision: u64,
}

/// A stored property value, as the file carries it: authored, never resolved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StoredValue {
    /// A dimensioned physical value, as its author wrote it.
    Quantity {
        /// Expression source.
        expression: String,
        /// The unit symbol the magnitude was written in, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// A flag.
    Boolean {
        /// The flag.
        value: bool,
    },
    /// Free-form text.
    Text {
        /// The text.
        value: String,
    },
}

/// One component of one stored object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredComponent {
    /// The plugin-qualified component type.
    pub component: ComponentTypeId,
    /// The schema version these values were checked against.
    pub schema_version: SchemaVersion,
    /// Authored property values.
    pub properties: BTreeMap<PropertyName, StoredValue>,
}

/// One stored object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredObject {
    /// The identity it had.
    pub id: u64,
    /// The human label.
    pub name: String,
    /// Where it is.
    pub transform: Transform,
    /// How it is moving.
    pub velocity: Velocity,
    /// Its extent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ObjectShape>,
    /// Its attached components, in component-type order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<StoredComponent>,
    /// The catalog template it was materialised from, if any.
    ///
    /// Historical evidence (ADR 0008): it says where these values came from,
    /// and nothing reads it to go back and look. A colleague opening the file
    /// without that catalog installed sees the same object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<TemplateProvenance>,
}

/// One stored variable definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredVariable {
    /// The identity it had.
    pub id: u64,
    /// Where it lives. Absent is the root namespace.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub namespace: String,
    /// Its editable name.
    pub name: String,
    /// Expression source.
    pub expression: String,
    /// What the author says it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// How many identities of each kind the experiment had ever allocated.
///
/// Persisted because an identity that was minted and then removed must never
/// be handed out again: a probe, a selection or a recorded series keyed to it
/// would silently rebind to a different object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredCounters {
    /// Object identities allocated over the experiment's whole history.
    pub objects: u64,
    /// Variable identities allocated over the experiment's whole history.
    pub variables: u64,
}

/// The experiment section of the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoredExperiment {
    /// The identity allocation to restore.
    pub counters: StoredCounters,
    /// The numerical setup and plugin composition.
    pub setup: Setup,
    /// Variable definitions, in identity order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<StoredVariable>,
    /// Objects, in identity order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<StoredObject>,
}

/// Why a saved default view could not be used.
///
/// Never a reason the document fails to load — see this module's header.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DefaultViewError {
    /// The section was written by a build with a newer view format.
    #[error("saved view is version {found}, and this build reads {supported}")]
    UnsupportedVersion {
        /// The version the section carried.
        found: u32,
        /// The version this build reads.
        supported: u32,
    },
    /// The section does not say which version it is.
    ///
    /// Its own failure rather than a malformed body, because the version is
    /// what decides how the body is read: without one there is no version
    /// whose rules the rest could be checked against.
    #[error("saved view does not declare a readable version")]
    NoVersion,
    /// The section claims a version this build reads, but its body is not the
    /// shape that version has.
    #[error("saved view could not be read: {message}")]
    Malformed {
        /// What the decoder objected to.
        message: String,
    },
}

impl DefaultViewError {
    /// A stable identifier for this reason.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedVersion { .. } => "unsupported_view_version",
            Self::NoVersion => "view_without_version",
            Self::Malformed { .. } => "malformed_view",
        }
    }
}

/// The whole of a version-1 `defaultView` section, version field included.
///
/// Version 1 had no scene scale, which meant one render unit was one metre.
/// Converting to the default scale is not a substitution for a missing value:
/// it is exactly what the section meant.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DefaultViewV1 {
    #[allow(dead_code)]
    version: u32,
    projection: Projection,
    camera: CameraPose,
}

/// The whole of a version-2 `defaultView` section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DefaultViewV2 {
    version: u32,
    projection: Projection,
    scale: SceneScale,
    camera: CameraPose,
}

/// ADR 0022's client-owned `defaultView` section, as the file carries it.
///
/// Held as *entirely* uninterpreted JSON, including its version header. That
/// is what makes this section non-blocking in the way ADR 0022 requires: there
/// is no field here — not even `version` — whose absence, presence or type a
/// stricter decode could object to before the section has had its say. A
/// missing version, a version from a newer build, a body of the wrong shape
/// and a section that is not even an object are all
/// [`DefaultViewError`]s from [`Self::decode`], and none of them is a reason
/// the experiment fails to open.
///
/// Ignored entirely by workload compilation, by Orishu, and by a headless
/// document authority (ADR 0022). Nothing in it enters the experiment
/// revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StoredDefaultView {
    section: serde_json::Value,
}

impl StoredDefaultView {
    /// Describe `view` as the current section version.
    pub fn of(view: &AuthoringView) -> Self {
        let section = DefaultViewV2 {
            version: DEFAULT_VIEW_VERSION,
            projection: view.projection(),
            scale: view.scale(),
            camera: view.camera(),
        };
        Self {
            // Unreachable failure: the section is a `u32`, a plain enum and a
            // finite-checked pose, none of which can fail to encode. A null
            // rather than a panic, which `decode` then reports through the
            // ordinary non-blocking path.
            section: serde_json::to_value(&section).unwrap_or(serde_json::Value::Null),
        }
    }

    /// The version this section declares, if it declares a readable one.
    pub fn declared_version(&self) -> Option<u32> {
        self.section
            .get("version")?
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
    }

    /// Interpret the section, if this build can.
    ///
    /// Each supported version gets its own shape and its own conversion, the
    /// same policy the envelope follows — the difference being what happens to
    /// a version this build does *not* have, which here is a report rather
    /// than a refusal.
    ///
    /// # Errors
    ///
    /// Returns [`DefaultViewError`] for a section with no readable version, a
    /// newer version, or a body that is not the shape its version declares.
    /// The caller reports it and carries on with a default view; the
    /// experiment is unaffected.
    pub fn decode(&self) -> Result<AuthoringView, DefaultViewError> {
        let version = self.declared_version().ok_or(DefaultViewError::NoVersion)?;
        let (projection, scale, camera) = match version {
            1 => {
                let decoded: DefaultViewV1 = self.body()?;
                // Version 1 predates scene scales, and meant one metre per
                // render unit. That is the default, so this is a conversion
                // rather than a default substituted for a missing field.
                (decoded.projection, SceneScale::default(), decoded.camera)
            }
            DEFAULT_VIEW_VERSION => {
                let decoded: DefaultViewV2 = self.body()?;
                (decoded.projection, decoded.scale, decoded.camera)
            }
            found => {
                return Err(DefaultViewError::UnsupportedVersion {
                    found,
                    supported: DEFAULT_VIEW_VERSION,
                });
            }
        };
        // Bounding happens here rather than being trusted from the file: a
        // saved pose is untrusted input, and what it can reach depends on the
        // scale saved beside it.
        AuthoringView::new(projection, scale, camera).map_err(|error| DefaultViewError::Malformed {
            message: error.to_string(),
        })
    }

    /// Deserialize the section as one version's shape.
    fn body<T: serde::de::DeserializeOwned>(&self) -> Result<T, DefaultViewError> {
        serde_json::from_value(self.section.clone()).map_err(|error| DefaultViewError::Malformed {
            message: error.to_string(),
        })
    }
}

/// A whole `kagami.experiment` document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExperimentDocument {
    /// Always [`FORMAT`]. Checked before anything else is read.
    pub format: String,
    /// The format version. Checked second, and never partially interpreted.
    pub format_version: u32,
    /// Support metadata, supplied by whatever saved the file.
    pub metadata: DocumentMetadata,
    /// The authored experiment.
    pub experiment: StoredExperiment,
    /// How the experiment was being looked at, if the writer saved a view.
    ///
    /// `None` has defined meaning in every version that has this field: no
    /// opening view was saved, so a reader opens at its own default camera.
    /// Version 1 had no such field at all and converts to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_view: Option<StoredDefaultView>,
}

/// The two fields that decide how the rest of a document is read.
///
/// Deliberately *not* `deny_unknown_fields`: its whole job is to answer "what
/// is this, and which version" for bytes whose remaining shape is not yet
/// known — including bytes from a build newer than this one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentHeader {
    format: String,
    format_version: u32,
}

/// A version-1 document, before `defaultView` existed.
///
/// Kept as its own DTO rather than reusing the current one with an optional
/// field: an explicit shape per supported version is what makes "this is what
/// version 1 meant" checkable, and it is what refuses a version-1 file that
/// carries a field version 1 never had.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DocumentV1 {
    format: String,
    metadata: DocumentMetadata,
    experiment: StoredExperiment,
    /// Read but unused: [`decode_document`] has already checked it.
    #[allow(dead_code)]
    format_version: u32,
}

impl From<DocumentV1> for ExperimentDocument {
    fn from(value: DocumentV1) -> Self {
        Self {
            format: value.format,
            // Converted *up*. Everything above the codec deals with one
            // shape, so a re-save of an opened version-1 document writes
            // version 2 — which is correct: it now has a view section.
            format_version: FORMAT_VERSION,
            metadata: value.metadata,
            experiment: value.experiment,
            // Version 1 remembered no view, and that absence has defined
            // meaning rather than being a missing value to invent.
            default_view: None,
        }
    }
}

/// Read `bytes` as a document, checking what it is before what it says.
///
/// The one entry point for untrusted bytes. It reads [`FORMAT`] and the
/// version from a permissive header, refuses anything that is not this format
/// or is newer than this build, and only then interprets the body with the DTO
/// for that version.
///
/// # Errors
///
/// Returns [`DocumentError::Malformed`] for bytes that are not this format's
/// JSON, [`DocumentError::WrongFormat`] for another format, and
/// [`DocumentError::UnsupportedVersion`] for a version outside
/// [`MIN_FORMAT_VERSION`]`..=`[`FORMAT_VERSION`].
pub fn decode_document(bytes: &[u8]) -> Result<ExperimentDocument, DocumentError> {
    let header: DocumentHeader =
        serde_json::from_slice(bytes).map_err(|error| DocumentError::Malformed {
            message: error.to_string(),
        })?;
    if header.format != FORMAT {
        return Err(DocumentError::WrongFormat {
            found: header.format,
            expected: FORMAT,
        });
    }

    match header.format_version {
        1 => Ok(serde_json::from_slice::<DocumentV1>(bytes)
            .map_err(|error| DocumentError::Malformed {
                message: error.to_string(),
            })?
            .into()),
        FORMAT_VERSION => serde_json::from_slice(bytes).map_err(|error| DocumentError::Malformed {
            message: error.to_string(),
        }),
        found => Err(DocumentError::UnsupportedVersion {
            found,
            supported: FORMAT_VERSION,
        }),
    }
}

impl ExperimentDocument {
    /// Describe `snapshot` and `view` as a document, stamped with `metadata`.
    ///
    /// Reads no clock and performs no IO: the same snapshot, view and metadata
    /// always produce the same document, which is what makes a canonical
    /// byte-identical re-encode a property rather than a hope.
    ///
    /// The view is an argument rather than something reached for, because it
    /// belongs to whichever client is authoring — saving must record the view
    /// in force without being able to *change* it (ADR 0022).
    pub fn of(
        experiment: &Experiment,
        snapshot: &ExperimentSnapshot,
        view: &AuthoringView,
        metadata: DocumentMetadata,
    ) -> Self {
        let counters = experiment.counters();
        Self {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            metadata,
            default_view: Some(StoredDefaultView::of(view)),
            experiment: StoredExperiment {
                counters: StoredCounters {
                    objects: counters.objects_minted(),
                    variables: counters.variables_minted(),
                },
                setup: snapshot.setup().clone(),
                variables: snapshot
                    .variables()
                    .iter()
                    .map(|(id, definition)| StoredVariable {
                        id: id.get(),
                        namespace: definition.namespace.as_str().to_owned(),
                        name: definition.name.as_str().to_owned(),
                        expression: definition.expression.clone(),
                        description: definition.description.clone(),
                    })
                    .collect(),
                objects: snapshot
                    .objects()
                    .iter()
                    .map(|(id, object)| StoredObject {
                        id: id.get(),
                        name: object.name.as_str().to_owned(),
                        transform: object.transform,
                        velocity: object.velocity,
                        shape: object.shape,
                        provenance: object.provenance.clone(),
                        components: object
                            .components
                            .iter()
                            .map(|(type_id, component)| StoredComponent {
                                component: type_id.clone(),
                                schema_version: component.schema_version,
                                properties: component
                                    .properties
                                    .iter()
                                    .map(|(property, value)| {
                                        (property.clone(), stored_value(value))
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }

    /// Rebuild the experiment this document describes.
    ///
    /// Every magnitude is re-derived from the authored source, so nothing the
    /// file says about a resolved value is believed — it does not say anything
    /// about one. A component whose plugin is not installed loads with its
    /// authored values intact and is reported by
    /// [`kagami_document::capability`]; that is not a load failure.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError`] for a document of the wrong format or a newer
    /// version, or whose contents the model refuses.
    pub fn into_experiment(
        self,
        schemas: &SchemaRegistry,
        limits: &Limits,
    ) -> Result<Experiment, DocumentError> {
        if self.format != FORMAT {
            return Err(DocumentError::WrongFormat {
                found: self.format,
                expected: FORMAT,
            });
        }
        if self.format_version != FORMAT_VERSION {
            return Err(DocumentError::UnsupportedVersion {
                found: self.format_version,
                supported: FORMAT_VERSION,
            });
        }

        let record = DocumentRecord {
            objects_minted: self.experiment.counters.objects,
            variables_minted: self.experiment.counters.variables,
            setup: self.experiment.setup,
            variables: self
                .experiment
                .variables
                .into_iter()
                .map(|definition| {
                    Ok(VariableRecord {
                        id: definition.id,
                        namespace: Namespace::new(definition.namespace),
                        name: Name::new(definition.name).map_err(invalid_name)?,
                        expression: definition.expression,
                        description: definition.description,
                    })
                })
                .collect::<Result<Vec<_>, DocumentError>>()?,
            objects: self
                .experiment
                .objects
                .into_iter()
                .map(|object| {
                    Ok(ObjectRecord {
                        id: object.id,
                        provenance: object.provenance,
                        name: kagami_document::DisplayName::new(object.name).map_err(|error| {
                            DocumentError::Malformed {
                                message: error.to_string(),
                            }
                        })?,
                        transform: object.transform,
                        velocity: object.velocity,
                        shape: object.shape,
                        components: object
                            .components
                            .into_iter()
                            .map(|component| {
                                Ok((
                                    component.component,
                                    ComponentRecord {
                                        schema_version: component.schema_version,
                                        properties: component
                                            .properties
                                            .into_iter()
                                            .map(|(property, value)| {
                                                Ok((property, value.into_authored()?))
                                            })
                                            .collect::<Result<_, DocumentError>>()?,
                                    },
                                ))
                            })
                            .collect::<Result<_, DocumentError>>()?,
                    })
                })
                .collect::<Result<Vec<_>, DocumentError>>()?,
        };

        Ok(hydrate(&record, schemas, limits)?)
    }
}

impl StoredValue {
    fn into_authored(self) -> Result<kagami_document::AuthoredValue, DocumentError> {
        Ok(match self {
            Self::Quantity { expression, unit } => match unit {
                None => kagami_document::AuthoredValue::si(expression),
                Some(symbol) => {
                    let unit = orishu_variables::lookup(&symbol).map_err(|error| {
                        DocumentError::Malformed {
                            message: error.to_string(),
                        }
                    })?;
                    kagami_document::AuthoredValue::in_unit(expression, *unit)
                }
            },
            Self::Boolean { value } => kagami_document::AuthoredValue::Boolean(value),
            Self::Text { value } => kagami_document::AuthoredValue::Text(value),
        })
    }
}

fn stored_value(value: &kagami_document::PropertyValue) -> StoredValue {
    match value.authored() {
        kagami_document::AuthoredValue::Quantity { expression, unit } => StoredValue::Quantity {
            expression,
            unit: unit.map(|unit| unit.symbol().to_owned()),
        },
        kagami_document::AuthoredValue::Boolean(value) => StoredValue::Boolean { value },
        kagami_document::AuthoredValue::Text(value) => StoredValue::Text { value },
    }
}

fn invalid_name(error: orishu_variables::InvalidName) -> DocumentError {
    DocumentError::Malformed {
        message: error.to_string(),
    }
}
