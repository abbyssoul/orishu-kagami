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
//! [`FORMAT_VERSION`]. A version *higher* than this build is refused outright
//! rather than partially read: a newer producer may mean something different by
//! a field this build recognises, and half-understanding a document is worse
//! than declining it. There is no version 0 and no "lower is probably safe"
//! rule; when an older version exists it gets an explicit conversion of its
//! own.
//!
//! Unknown fields are refused. A producer that added a field without advancing
//! the version would otherwise have its content silently dropped on the next
//! re-save, which is exactly the failure a shared-file workflow cannot afford.
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

/// The format identifier every document carries.
pub const FORMAT: &str = "kagami.experiment";

/// The format version this build writes, and the only one it reads.
pub const FORMAT_VERSION: u32 = 1;

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
}

impl ExperimentDocument {
    /// Describe `snapshot` as a document, stamped with `metadata`.
    ///
    /// Reads no clock and performs no IO: the same snapshot and metadata
    /// always produce the same document, which is what makes a canonical
    /// byte-identical re-encode a property rather than a hope.
    pub fn of(
        experiment: &Experiment,
        snapshot: &ExperimentSnapshot,
        metadata: DocumentMetadata,
    ) -> Self {
        let counters = experiment.counters();
        Self {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            metadata,
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
