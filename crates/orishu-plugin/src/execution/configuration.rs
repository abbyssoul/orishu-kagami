//! Shared declarative configuration compilation and portable resolved values.
//! Worker-side readers never evaluate a default expression or consult variables.
use crate::{
    Dimension, Error, ErrorCode, FiniteF64, Limits, LocalContributionId, Property, PropertyType,
    codec,
    projection::{Project, object},
};
use orishu_resource::ApiVersion;
use orishu_variables::{VariablesError, VariablesSystem};
use orishu_workload::canonical::CanonicalValue as V;
use serde::{Deserialize, Serialize};

/// Portable resolved configuration schema; one configuration per grant.
pub const CONFIGURATION_SCHEMA: &str = "orishu.simulation.configuration/v1";

/// A concrete configuration value. Quantities retain their physical dimension
/// despite using SI magnitudes; a unitless number never becomes a mass implicitly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ConfigurationValue {
    /// Resolved finite SI scalar, with the dimension derived by the evaluator.
    Quantity {
        /// SI magnitude, not the original authored expression.
        #[serde(rename = "valueSI")]
        value_si: FiniteF64,
        /// SI exponents in shared dimension order.
        dimension: Dimension,
    },
    /// Resolved literal boolean.
    Boolean {
        /// Literal value, including explicitly selected false.
        value: bool,
    },
    /// Bounded UTF-8 string, never code or an implicit artifact selection.
    Text {
        /// Literal value, interpreted only by the selected schema/kernel.
        value: String,
    },
}

/// Borrowed resolved property input, shared by authoring and workload validation.
/// This is not an accepted configuration: the selected declaration must govern it.
#[derive(Clone, Copy, Debug)]
pub enum PropertyValueRef<'a> {
    /// Scalar in canonical SI units; non-finite magnitudes are always refused.
    Quantity {
        /// Canonical SI magnitude.
        value_si: f64,
        /// Physical dimension of the resolved value.
        dimension: Dimension,
    },
    /// Literal flag.
    Boolean(bool),
    /// UTF-8 text, checked by encoded byte length.
    Text(&'a str),
}

impl ConfigurationValue {
    /// Borrow without copying strings or allocating intermediate configurations.
    pub fn as_property_value(&self) -> PropertyValueRef<'_> {
        match self {
            Self::Quantity {
                value_si,
                dimension,
            } => PropertyValueRef::Quantity {
                value_si: value_si.get(),
                dimension: *dimension,
            },
            Self::Boolean { value } => PropertyValueRef::Boolean(*value),
            Self::Text { value } => PropertyValueRef::Text(value),
        }
    }
}

impl PropertyType {
    /// Check a resolved value against this declaration's type, dimension and
    /// inclusive limits. Does not insert defaults, evaluate source or prove that
    /// this declaration belongs to an enabled/selected provider.
    pub fn accepts(&self, value: PropertyValueRef<'_>) -> bool {
        match (self, value) {
            (
                Self::Quantity {
                    dimension,
                    minimum_si,
                    maximum_si,
                    ..
                },
                PropertyValueRef::Quantity {
                    value_si,
                    dimension: actual,
                },
            ) => {
                *dimension == actual
                    && value_si.is_finite()
                    && minimum_si.is_none_or(|min| value_si >= min.get())
                    && maximum_si.is_none_or(|max| value_si <= max.get())
            }
            (Self::Boolean { .. }, PropertyValueRef::Boolean(_)) => true,
            (Self::Text { max_bytes, .. }, PropertyValueRef::Text(value)) => {
                value.len() <= *max_bytes as usize
            }
            _ => false,
        }
    }
}
/// One exact named value. Portable resolved sets sort strictly by ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationProperty {
    /// Property in the selected computational contribution's declaration.
    pub id: LocalContributionId,
    /// Explicit resolved value.
    pub value: ConfigurationValue,
}
/// Versioned, raw resolved configuration. Acceptance requires bounded encoding/
/// decoding and agreement with the selected contribution's property declarations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedConfiguration {
    /// Exactly `orishu.simulation.configuration/v1`.
    pub api_version: ApiVersion,
    /// Canonical sorted named values; omission is different from a default value.
    pub properties: Vec<ConfigurationProperty>,
}

fn limited(path: &str) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        path,
        "configuration budget exceeded",
    )
}
impl ResolvedConfiguration {
    fn validate(&self, limits: &Limits) -> Result<(), Error> {
        if self.api_version.as_str() != CONFIGURATION_SCHEMA {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "apiVersion",
                "unsupported configuration version",
            ));
        }
        if self.properties.len() > limits.max_schema_items {
            return Err(limited("properties"));
        }
        if self.properties.windows(2).any(|p| p[0].id >= p[1].id) {
            return Err(Error::malformed(
                "properties",
                "configuration properties must be unique and sorted",
            ));
        }
        for p in &self.properties {
            if let ConfigurationValue::Text { value } = &p.value
                && value.len() > limits.max_text_bytes
            {
                return Err(limited(p.id.as_str()));
            }
        }
        Ok(())
    }
    /// Lookup without allocation. A decoded/compiled configuration is sorted;
    /// raw direct values must be validated before using this accessor.
    pub fn get(&self, id: &str) -> Option<&ConfigurationValue> {
        self.properties
            .binary_search_by(|p| p.id.as_str().cmp(id))
            .ok()
            .map(|i| &self.properties[i].value)
    }
    /// Validate against an independently selected property schema. Required
    /// omissions, extra keys, wrong types/dimensions and range excess are refused.
    /// This never inserts defaults or changes captured workload values.
    pub fn validate_against(&self, schema: &[Property], limits: &Limits) -> Result<(), Error> {
        self.validate(limits)?;
        crate::validation::properties(schema, limits)?;
        for p in &self.properties {
            let declared = schema.iter().find(|s| s.id == p.id).ok_or_else(|| {
                Error::malformed(p.id.as_str(), "undeclared configuration property")
            })?;
            if !declared.schema.accepts(p.value.as_property_value()) {
                return Err(Error::malformed(
                    p.id.as_str(),
                    "configuration type, dimension or constraint mismatch",
                ));
            }
        }
        for declared in schema {
            if declared.required && self.get(declared.id.as_str()).is_none() {
                return Err(Error::malformed(
                    declared.id.as_str(),
                    "required configuration value is missing",
                ));
            }
        }
        Ok(())
    }
    /// Encode the explicit canonical projection; does not guess a property schema.
    pub fn to_cbor(&self, limits: &Limits) -> Result<Vec<u8>, Error> {
        self.validate(limits)?;
        codec::encode(&self.project(), limits, limits.max_payload_bytes)
    }
    /// Decode with byte/depth/value/collection/text bounds and require canonical
    /// bytes. Validate against the exact selected schema before admission.
    pub fn from_cbor(bytes: &[u8], limits: &Limits) -> Result<Self, Error> {
        let config: Self = codec::structured_from_cbor(bytes, limits, limits.max_payload_bytes)?;
        if config.to_cbor(limits)? != bytes {
            return Err(Error::malformed(
                "configuration",
                "noncanonical configuration",
            ));
        }
        Ok(config)
    }
}
impl Project for ConfigurationValue {
    fn project(&self) -> V {
        match self {
            Self::Quantity {
                value_si,
                dimension,
            } => object(vec![
                ("kind", Some(V::text("quantity"))),
                ("valueSI", Some(value_si.project())),
                ("dimension", Some(dimension.project())),
            ]),
            Self::Boolean { value } => object(vec![
                ("kind", Some(V::text("boolean"))),
                ("value", Some(value.project())),
            ]),
            Self::Text { value } => object(vec![
                ("kind", Some(V::text("text"))),
                ("value", Some(value.project())),
            ]),
        }
    }
}
impl Project for ConfigurationProperty {
    fn project(&self) -> V {
        object(vec![
            ("id", Some(self.id.project())),
            ("value", Some(self.value.project())),
        ])
    }
}
impl Project for ResolvedConfiguration {
    fn project(&self) -> V {
        object(vec![
            ("apiVersion", Some(V::text(self.api_version.as_str()))),
            ("properties", Some(self.properties.project())),
        ])
    }
}

/// Retained authoring input. Compilation borrows it; the document remains the
/// authority retaining source/provenance. This is not a workload-resolved value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ConfigurationInput {
    /// Quantity expression using the one shared variables engine.
    Expression {
        /// Original source, never replaced by the computed SI value.
        source: String,
    },
    /// Explicit boolean, overriding any default.
    Boolean {
        /// Authored boolean.
        value: bool,
    },
    /// Explicit bounded text, overriding any default.
    Text {
        /// Authored text.
        value: String,
    },
}
/// One authored configuration input; input order does not select semantics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoredConfigurationProperty {
    /// Exact selected-schema property name.
    pub id: LocalContributionId,
    /// Retained expression or literal.
    pub input: ConfigurationInput,
}

/// Compile declared inputs/defaults through the caller's shared variables engine,
/// then check required values, types, dimensions and numeric/text constraints.
/// No variable definition or authoring input is changed. Unknown/duplicate inputs
/// are refused before evaluation. Defaults are applied here, never at worker load.
///
/// Work is O(properties² + evaluated expression work), with at most the declared
/// property count evaluations. Each uses the supplied `VariablesSystem`'s owned
/// source/dependency/evaluation bounds. Schema/source/output have `limits` bounds.
pub fn resolve_configuration(
    schema: &[Property],
    inputs: &[AuthoredConfigurationProperty],
    variables: &VariablesSystem,
    limits: &Limits,
) -> Result<ResolvedConfiguration, Error> {
    crate::validation::properties(schema, limits)?;
    if inputs.len() > limits.max_schema_items {
        return Err(limited("inputs"));
    }
    for (i, input) in inputs.iter().enumerate() {
        if inputs[..i].iter().any(|p| p.id == input.id) || !schema.iter().any(|p| p.id == input.id)
        {
            return Err(Error::malformed(
                input.id.as_str(),
                "duplicate or undeclared configuration input",
            ));
        }
        let length = match &input.input {
            ConfigurationInput::Expression { source } => source.len(),
            ConfigurationInput::Text { value } => value.len(),
            ConfigurationInput::Boolean { .. } => 0,
        };
        if length > limits.max_text_bytes {
            return Err(limited(input.id.as_str()));
        }
    }
    let quantity = |source: &str, id: &LocalContributionId| -> Result<ConfigurationValue, Error> {
        // Tight schema/source parsing bounds apply in addition to the variable
        // environment's evaluation/dependency policy.
        orishu_variables::CompiledExpression::parse_bounded(source, &limits.expressions).map_err(
            |e| {
                Error::new(
                    if e.limit_error().is_some() {
                        ErrorCode::LimitExceeded
                    } else {
                        ErrorCode::Malformed
                    },
                    id.as_str(),
                    "invalid or over-budget configuration expression",
                )
            },
        )?;
        let value = variables.eval(source).map_err(|e| {
            let limit = matches!(e, VariablesError::Limit(_))
                || matches!(&e,VariablesError::Parsing(p) if p.limit_error().is_some());
            Error::new(
                if limit {
                    ErrorCode::LimitExceeded
                } else {
                    ErrorCode::Malformed
                },
                id.as_str(),
                "configuration expression could not be resolved",
            )
        })?;
        Ok(ConfigurationValue::Quantity {
            value_si: FiniteF64::new(value.magnitude())
                .map_err(|_| Error::malformed(id.as_str(), "nonfinite configuration value"))?,
            dimension: value.dimension(),
        })
    };
    let mut properties = Vec::new();
    properties
        .try_reserve(schema.len())
        .map_err(|_| limited("properties"))?;
    for p in schema {
        let value = if let Some(input) = inputs.iter().find(|v| v.id == p.id) {
            Some(match &input.input {
                ConfigurationInput::Expression { source } => quantity(source, &p.id)?,
                ConfigurationInput::Boolean { value } => {
                    ConfigurationValue::Boolean { value: *value }
                }
                ConfigurationInput::Text { value } => ConfigurationValue::Text {
                    value: value.clone(),
                },
            })
        } else {
            match &p.schema {
                PropertyType::Quantity {
                    default_expression, ..
                } => default_expression
                    .as_ref()
                    .map(|source| quantity(source, &p.id))
                    .transpose()?,
                PropertyType::Boolean { default } => {
                    default.map(|value| ConfigurationValue::Boolean { value })
                }
                PropertyType::Text { default, .. } => {
                    default.as_ref().map(|value| ConfigurationValue::Text {
                        value: value.clone(),
                    })
                }
            }
        };
        if let Some(value) = value {
            properties.push(ConfigurationProperty {
                id: p.id.clone(),
                value,
            });
        }
    }
    properties.sort_by(|a, b| a.id.cmp(&b.id));
    let config = ResolvedConfiguration {
        api_version: ApiVersion::from_static(CONFIGURATION_SCHEMA),
        properties,
    };
    config.validate_against(schema, limits)?;
    // Bound the complete captured artifact, not only individual strings/counts.
    config.to_cbor(limits)?;
    Ok(config)
}
