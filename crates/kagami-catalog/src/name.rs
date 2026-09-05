//! Catalog identifier new-types.
//!
//! Every catalog, template, component, property, parameter, and helper name
//! is a validated [`orishu_variables::Name`] rather than a free-form string.
//! That is not stylistic: ADR 0018 projects every expression-capable template
//! property into the shared variable environment under a
//! catalog/template/component/property path, so a name that cannot be a
//! variable-name segment could never be referenced. Parsing names into these
//! types at the document boundary makes an unprojectable identity
//! unrepresentable rather than a failure discovered later during projection.

use std::fmt;

use orishu_variables::{InvalidName, Name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a catalog identifier could not be accepted.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NameError {
    /// A segment is not a valid variable-name segment.
    #[error(transparent)]
    Segment(#[from] InvalidName),
    /// A dotted plugin identifier had no segments at all.
    #[error("plugin identifier must not be empty")]
    EmptyPluginId,
    /// A dotted plugin identifier exceeded [`MAX_PLUGIN_ID_SEGMENTS`].
    #[error("plugin identifier has {found} segments, limit is {limit}")]
    TooManyPluginSegments { found: usize, limit: usize },
}

/// Upper bound on the dotted segments of a [`PluginId`], so a hostile
/// document cannot turn one identifier into an unbounded allocation.
pub const MAX_PLUGIN_ID_SEGMENTS: usize = 8;

macro_rules! catalog_name {
    ($(#[$meta:meta])* $type_name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $type_name(Name);

        impl $type_name {
            /// Validate and construct the identifier from its text.
            pub fn new(value: impl Into<String>) -> Result<Self, NameError> {
                Ok(Self(Name::new(value)?))
            }

            /// The identifier's text.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// The identifier as a variable-name segment, for building the
            /// canonical binding identity this name participates in.
            pub fn as_name(&self) -> &Name {
                &self.0
            }
        }

        impl fmt::Display for $type_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl TryFrom<String> for $type_name {
            type Error = NameError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $type_name {
            type Error = NameError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$type_name> for String {
            fn from(value: $type_name) -> Self {
                value.0.as_str().to_owned()
            }
        }
    };
}

catalog_name!(
    /// Names a catalog: the outermost scope of a template identity, and the
    /// first segment of every binding that catalog publishes.
    CatalogName
);
catalog_name!(
    /// Names one object template within a catalog.
    TemplateName
);
catalog_name!(
    /// Names a component within a template. Unique per template, because it
    /// is the binding namespace segment its properties are published under.
    ComponentName
);
catalog_name!(
    /// Names an authored property of a component.
    PropertyName
);
catalog_name!(
    /// Names a template parameter supplied at instantiation.
    ParameterName
);
catalog_name!(
    /// Names a template-local helper definition.
    HelperName
);

/// A dotted simulation-plugin identifier such as `kagami.mass_sources`.
///
/// Plugin identity qualifies a component type so two plugins may contribute
/// components with the same local name. It deliberately does *not* appear in
/// the binding namespace: bindings are addressed through the template-local
/// component name, which a template already has to keep unique.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PluginId(Vec<Name>);

impl PluginId {
    /// Parse a dotted plugin identifier, validating every segment.
    pub fn new(value: &str) -> Result<Self, NameError> {
        let segments = value.split('.').count();
        if segments > MAX_PLUGIN_ID_SEGMENTS {
            return Err(NameError::TooManyPluginSegments {
                found: segments,
                limit: MAX_PLUGIN_ID_SEGMENTS,
            });
        }
        let segments = value
            .split('.')
            .map(Name::new)
            .collect::<Result<Vec<_>, _>>()?;
        if segments.is_empty() {
            return Err(NameError::EmptyPluginId);
        }
        Ok(Self(segments))
    }

    /// The dotted segments, outermost first.
    pub fn segments(&self) -> &[Name] {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut segments = self.0.iter();
        if let Some(first) = segments.next() {
            write!(formatter, "{first}")?;
        }
        for segment in segments {
            write!(formatter, ".{segment}")?;
        }
        Ok(())
    }
}

impl TryFrom<String> for PluginId {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<PluginId> for String {
    fn from(value: PluginId) -> Self {
        value.to_string()
    }
}

/// The plugin-qualified identity of a component type, as declared by a
/// simulation plugin's schema and referenced by a template.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ComponentTypeId {
    /// The plugin that contributes this component type.
    pub plugin: PluginId,
    /// The component's name within that plugin.
    pub name: ComponentName,
}

impl ComponentTypeId {
    /// Build a component type identity from already-validated parts.
    pub fn new(plugin: PluginId, name: ComponentName) -> Self {
        Self { plugin, name }
    }
}

impl fmt::Display for ComponentTypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.plugin, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_identifier_shaped_names() {
        assert_eq!(CatalogName::new("planets").unwrap().as_str(), "planets");
        assert_eq!(
            TemplateName::new("anti_proton").unwrap().as_str(),
            "anti_proton"
        );
        assert_eq!(PropertyName::new("_private").unwrap().as_str(), "_private");
    }

    #[test]
    fn rejects_names_that_could_not_be_a_variable_segment() {
        // Hyphens and spaces read naturally in YAML but could never be
        // referenced from an expression, so they are refused at the boundary.
        assert!(TemplateName::new("anti-proton").is_err());
        assert!(TemplateName::new("fancy unicorn").is_err());
        assert!(TemplateName::new("solar.system").is_err());
        assert!(TemplateName::new("").is_err());
        assert!(TemplateName::new("1st").is_err());
    }

    #[test]
    fn plugin_id_round_trips_its_dotted_form() {
        let plugin = PluginId::new("kagami.mass_sources").unwrap();
        assert_eq!(plugin.to_string(), "kagami.mass_sources");
        assert_eq!(plugin.segments().len(), 2);
    }

    #[test]
    fn plugin_id_rejects_an_invalid_or_oversized_segment_list() {
        assert!(PluginId::new("kagami..sources").is_err());
        assert!(PluginId::new("kagami.mass-sources").is_err());
        let deep = std::iter::repeat_n("a", MAX_PLUGIN_ID_SEGMENTS + 1)
            .collect::<Vec<_>>()
            .join(".");
        assert_eq!(
            PluginId::new(&deep),
            Err(NameError::TooManyPluginSegments {
                found: MAX_PLUGIN_ID_SEGMENTS + 1,
                limit: MAX_PLUGIN_ID_SEGMENTS,
            })
        );
    }

    #[test]
    fn component_type_id_displays_plugin_qualified() {
        let id = ComponentTypeId::new(
            PluginId::new("kagami.mass_sources").unwrap(),
            ComponentName::new("inertial_mass").unwrap(),
        );
        assert_eq!(id.to_string(), "kagami.mass_sources/inertial_mass");
    }

    #[test]
    fn names_round_trip_through_serde() {
        let name = CatalogName::new("planets").unwrap();
        let encoded = serde_yaml::to_string(&name).unwrap();
        assert_eq!(encoded.trim(), "planets");
        let decoded: CatalogName = serde_yaml::from_str(&encoded).unwrap();
        assert_eq!(decoded, name);
    }

    #[test]
    fn serde_rejects_an_invalid_name_at_the_document_boundary() {
        assert!(serde_yaml::from_str::<CatalogName>("anti-proton").is_err());
    }
}
