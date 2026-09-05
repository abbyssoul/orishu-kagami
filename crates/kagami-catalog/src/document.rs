//! The hand-authored object-template document format.
//!
//! These types decode and re-encode the document's *shape* only. They accept
//! validated names and units (parsing them at this boundary rather than
//! carrying strings inward), but they do not resolve component schemas,
//! expressions, visibility, or dimensions — [`crate::template`] does that.
//!
//! The wire spelling is camelCase for envelope keys (`apiVersion`) matching
//! the Kubernetes-style convention this format borrows, and snake_case
//! elsewhere, because every other identifier in the document is also a
//! variable-name segment and must look like one.
//!
//! Serialization is symmetric with deserialization: the writer emits a
//! document through these same types, so the canonical bytes it produces are
//! exactly what the loader would read back. That is what makes a
//! [`crate::source::ContentFingerprint`] independent of whitespace and key
//! order in the file a user actually edited.

use std::collections::BTreeMap;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::CatalogName;
use crate::name::{ComponentTypeId, HelperName, ParameterName, PropertyName, TemplateName};

/// The only document format version this crate reads or writes.
pub const API_VERSION: &str = "kagami.catalog/v1";

/// The only document kind this crate reads or writes.
pub const KIND: &str = "ObjectTemplate";

/// The `apiVersion`/`kind` discriminator, decoded on its own before any other
/// field of a document is trusted.
///
/// Unknown fields are permitted here (unlike [`TemplateDocument`]) precisely
/// so that a document from a *future* format version is rejected for its
/// version rather than for whichever unrecognised field happens to be read
/// first.
#[derive(Clone, Debug, Deserialize)]
pub struct Envelope {
    /// The document format version.
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    /// The document kind.
    pub kind: String,
}

/// One object-template document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDocument {
    /// The document format version. Always [`API_VERSION`] once validated.
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    /// The document kind. Always [`KIND`] once validated.
    pub kind: String,
    /// Identity and human-facing description.
    pub metadata: MetadataDocument,
    /// The reusable content: parameters, helpers, and component composition.
    pub spec: SpecDocument,
}

impl TemplateDocument {
    /// Build a document with this crate's format version and kind.
    pub fn new(metadata: MetadataDocument, spec: SpecDocument) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            kind: KIND.to_owned(),
            metadata,
            spec,
        }
    }
}

/// A template's identity and human-facing metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataDocument {
    /// The catalog this template belongs to.
    pub catalog: CatalogName,
    /// The template's name within that catalog.
    pub name: TemplateName,
    /// One-line human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Searchable tags.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Free-form provenance, kept separate from searchable `labels`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

impl MetadataDocument {
    /// Metadata with only the required identity fields.
    pub fn new(catalog: CatalogName, name: TemplateName) -> Self {
        Self {
            catalog,
            name,
            description: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
        }
    }
}

/// The reusable content of a template.
///
/// Instance identity, display name, and placement are deliberately absent:
/// ADR 0008 keeps invocation-specific inputs in the instantiation command,
/// not in reusable template content.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecDocument {
    /// Inputs an instantiation may override, each with a default so the
    /// template is always previewable.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<ParameterName, ParameterDocument>,
    /// Template-local intermediate definitions, private by default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub helpers: BTreeMap<HelperName, HelperDocument>,
    /// The components this template composes, in authored order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentDocument>,
}

/// A template parameter: a dimensioned input with a default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDocument {
    /// The default expression, used for preview and validation and when an
    /// instantiation does not override the parameter.
    pub default: String,
    /// The unit the parameter is measured in. Absent means dimensionless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// One-line human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A template-local helper definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperDocument {
    /// The authored expression.
    pub expression: String,
    /// The unit the helper's magnitude is authored in. Absent means the
    /// canonical SI unit of whatever the expression composes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Visibility outside this template. Helpers are private by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<VisibilityDocument>,
    /// One-line human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One component composed into a template, with its authored properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDocument {
    /// The plugin-qualified component type.
    #[serde(rename = "type")]
    pub component_type: ComponentTypeId,
    /// Authored property values, keyed by property name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValueDocument>,
}

/// Whether a binding resolves outside the template that declares it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisibilityDocument {
    /// Resolvable from any loaded catalog.
    #[default]
    Public,
    /// Resolvable only from within the declaring template.
    Private,
}

/// An authored quantity: an expression plus how to read its magnitude.
#[derive(Clone, Debug, PartialEq)]
pub struct QuantityDocument {
    /// The authored expression source, retained verbatim.
    pub expression: String,
    /// The unit the expression's magnitude is authored in. Absent means the
    /// canonical SI unit of the property's declared dimension.
    pub unit: Option<String>,
    /// Visibility outside this template. Properties are public by default.
    pub visibility: Option<VisibilityDocument>,
}

impl QuantityDocument {
    /// A quantity authored in canonical SI with default visibility.
    pub fn canonical(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            unit: None,
            visibility: None,
        }
    }

    /// A quantity authored in `unit`.
    pub fn in_unit(expression: impl Into<String>, unit: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            unit: Some(unit.into()),
            visibility: None,
        }
    }

    /// `true` when this quantity carries nothing beyond its expression, and
    /// so round-trips through the `mass: "1.0"` shorthand spelling.
    fn is_shorthand(&self) -> bool {
        self.unit.is_none() && self.visibility.is_none()
    }
}

/// Accepts both the shorthand `"1.989e30"` and the full
/// `{ expression: "...", unit: kg }` spelling, so a catalog of plain SI
/// literals stays readable without giving up unit and visibility annotations
/// where they matter.
impl<'de> Deserialize<'de> for QuantityDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Full {
            expression: String,
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            visibility: Option<VisibilityDocument>,
        }

        struct QuantityVisitor;

        impl<'de> Visitor<'de> for QuantityVisitor {
            type Value = QuantityDocument;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an expression string, or a map with an `expression` key")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(QuantityDocument::canonical(value))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let full = Full::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(QuantityDocument {
                    expression: full.expression,
                    unit: full.unit,
                    visibility: full.visibility,
                })
            }
        }

        deserializer.deserialize_any(QuantityVisitor)
    }
}

impl Serialize for QuantityDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.is_shorthand() {
            return serializer.serialize_str(&self.expression);
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("expression", &self.expression)?;
        if let Some(unit) = &self.unit {
            map.serialize_entry("unit", unit)?;
        }
        if let Some(visibility) = &self.visibility {
            map.serialize_entry("visibility", visibility)?;
        }
        map.end()
    }
}

/// The wire form of one authored property value.
///
/// Encoded as a single-key map (`{ quantity: "1.0" }`, `{ boolean: true }`)
/// rather than an internally tagged `kind:` field: the single-key form is
/// what a scientist reads most easily, and hand-writing the two serde impls
/// is cheaper than the alternative of YAML `!Tag` syntax, which serde's
/// default externally-tagged representation would require.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValueDocument {
    /// A dimensioned physical value.
    Quantity(QuantityDocument),
    /// A flag.
    Boolean(bool),
    /// Free-form text.
    Text(String),
}

impl PropertyValueDocument {
    /// The variant name as it appears in the document.
    pub fn label(&self) -> &'static str {
        match self {
            PropertyValueDocument::Quantity(_) => "quantity",
            PropertyValueDocument::Boolean(_) => "boolean",
            PropertyValueDocument::Text(_) => "text",
        }
    }
}

const PROPERTY_VARIANTS: &[&str] = &["quantity", "boolean", "text"];

impl<'de> Deserialize<'de> for PropertyValueDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PropertyVisitor;

        impl<'de> Visitor<'de> for PropertyVisitor {
            type Value = PropertyValueDocument;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a map with exactly one of: quantity, boolean, text")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::custom("expected exactly one property-value key"))?;
                let value = match key.as_str() {
                    "quantity" => PropertyValueDocument::Quantity(map.next_value()?),
                    "boolean" => PropertyValueDocument::Boolean(map.next_value()?),
                    "text" => PropertyValueDocument::Text(map.next_value()?),
                    other => return Err(de::Error::unknown_variant(other, PROPERTY_VARIANTS)),
                };
                if map.next_key::<String>()?.is_some() {
                    return Err(de::Error::custom("expected exactly one property-value key"));
                }
                Ok(value)
            }
        }

        deserializer.deserialize_map(PropertyVisitor)
    }
}

impl Serialize for PropertyValueDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            PropertyValueDocument::Quantity(quantity) => {
                map.serialize_entry("quantity", quantity)?
            }
            PropertyValueDocument::Boolean(value) => map.serialize_entry("boolean", value)?,
            PropertyValueDocument::Text(value) => map.serialize_entry("text", value)?,
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: planets
  name: sun
  description: Sol
  annotations:
    source: NASA/JPL Planetary Fact Sheet
spec:
  parameters:
    scale:
      default: "1"
  helpers:
    solar_mass:
      expression: "1.989e30"
      unit: kg
  components:
  - type:
      plugin: kagami.mass_sources
      name: inertial_mass
    properties:
      mass:
        quantity:
          expression: planets.sun.solar_mass * planets.sun.scale
  - type:
      plugin: kagami.geometry
      name: sphere
    properties:
      radius:
        quantity:
          expression: "6.9634e5"
          unit: km
  - type:
      plugin: kagami.mass_sources
      name: gravitational_mass
    properties:
      follows_inertial:
        boolean: true
"#;

    fn parse(text: &str) -> TemplateDocument {
        serde_yaml::from_str(text).unwrap()
    }

    #[test]
    fn parses_a_complete_template_document() {
        let document = parse(EXAMPLE);
        assert_eq!(document.api_version, API_VERSION);
        assert_eq!(document.kind, KIND);
        assert_eq!(document.metadata.catalog.as_str(), "planets");
        assert_eq!(document.metadata.name.as_str(), "sun");
        assert_eq!(document.metadata.description.as_deref(), Some("Sol"));
        assert_eq!(document.spec.components.len(), 3);
        assert_eq!(document.spec.parameters.len(), 1);
        assert_eq!(document.spec.helpers.len(), 1);
    }

    #[test]
    fn helper_retains_its_authored_unit() {
        let document = parse(EXAMPLE);
        let helper = &document.spec.helpers[&HelperName::new("solar_mass").unwrap()];
        assert_eq!(helper.expression, "1.989e30");
        assert_eq!(helper.unit.as_deref(), Some("kg"));
    }

    #[test]
    fn property_value_variants_decode_from_their_single_key_form() {
        let document = parse(EXAMPLE);
        let radius = &document.spec.components[1].properties[&PropertyName::new("radius").unwrap()];
        assert_eq!(
            radius,
            &PropertyValueDocument::Quantity(QuantityDocument::in_unit("6.9634e5", "km"))
        );
        let follows = &document.spec.components[2].properties
            [&PropertyName::new("follows_inertial").unwrap()];
        assert_eq!(follows, &PropertyValueDocument::Boolean(true));
    }

    #[test]
    fn quantity_shorthand_is_the_same_value_as_the_long_form() {
        let shorthand: PropertyValueDocument = serde_yaml::from_str("quantity: \"1.5\"").unwrap();
        let long: PropertyValueDocument =
            serde_yaml::from_str("quantity:\n  expression: \"1.5\"").unwrap();
        assert_eq!(shorthand, long);
    }

    #[test]
    fn documents_round_trip_through_serialization_unchanged() {
        let document = parse(EXAMPLE);
        let encoded = serde_yaml::to_string(&document).unwrap();
        assert_eq!(parse(&encoded), document);
    }

    #[test]
    fn serialization_is_canonical_regardless_of_authored_key_order() {
        let reordered = EXAMPLE.replace(
            "  catalog: planets\n  name: sun\n",
            "  name: sun\n  catalog: planets\n",
        );
        assert_ne!(reordered, EXAMPLE);
        assert_eq!(
            serde_yaml::to_string(&parse(&reordered)).unwrap(),
            serde_yaml::to_string(&parse(EXAMPLE)).unwrap()
        );
    }

    #[test]
    fn a_quantity_without_a_unit_serializes_back_to_the_shorthand() {
        let value = PropertyValueDocument::Quantity(QuantityDocument::canonical("1.5"));
        assert_eq!(
            serde_yaml::to_string(&value).unwrap().trim(),
            "quantity: '1.5'"
        );
    }

    #[test]
    fn a_misspelled_field_is_rejected_rather_than_silently_dropped() {
        let typo = EXAMPLE.replace("description: Sol", "descripton: Sol");
        assert!(serde_yaml::from_str::<TemplateDocument>(&typo).is_err());
    }

    #[test]
    fn an_unknown_property_value_variant_names_the_ones_that_exist() {
        let error = serde_yaml::from_str::<PropertyValueDocument>("colour: red").unwrap_err();
        assert!(error.to_string().contains("quantity"), "{error}");
    }

    #[test]
    fn envelope_decodes_from_a_document_whose_body_is_unrecognisable() {
        let envelope: Envelope =
            serde_yaml::from_str("apiVersion: kagami.catalog/v2\nkind: Nonsense\nwat: [1, 2]")
                .unwrap();
        assert_eq!(envelope.api_version, "kagami.catalog/v2");
        assert_eq!(envelope.kind, "Nonsense");
    }
}
