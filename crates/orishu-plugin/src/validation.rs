//! Structural/scientific declaration validation, not resolution or numerical admission.
use crate::projection::Project;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

fn bound(found: usize, max: usize, path: &str) -> Result<(), Error> {
    if found > max {
        Err(Error::new(
            ErrorCode::LimitExceeded,
            path,
            "declaration budget exceeded",
        ))
    } else {
        Ok(())
    }
}
fn text(s: &str, l: &Limits, path: &str) -> Result<(), Error> {
    bound(s.len(), l.max_text_bytes, path)
}
fn nonempty(s: &str, l: &Limits, path: &str) -> Result<(), Error> {
    text(s, l, path)?;
    if s.is_empty() {
        Err(Error::malformed(path, "must not be empty"))
    } else {
        Ok(())
    }
}
fn unique<'a, T: Ord + 'a>(items: impl Iterator<Item = &'a T>, path: &str) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(Error::malformed(path, "duplicate declaration"));
        }
    }
    Ok(())
}
fn annotations(a: Option<&Annotations>, l: &Limits) -> Result<(), Error> {
    if let Some(a) = a {
        for v in [&a.label, &a.description, &a.group, &a.preferred_unit]
            .into_iter()
            .flatten()
        {
            text(v, l, "annotations")?;
        }
    }
    Ok(())
}
fn properties(p: &[Property], l: &Limits) -> Result<(), Error> {
    bound(p.len(), l.max_schema_items, "properties")?;
    unique(p.iter().map(|p| &p.id), "properties")?;
    for p in p {
        match &p.schema {
            PropertyType::Quantity {
                default_expression,
                minimum_si,
                maximum_si,
                ..
            } => {
                if let Some(e) = default_expression {
                    nonempty(e, l, "defaultExpression")?;
                    orishu_variables::CompiledExpression::parse_bounded(e, &l.expressions)
                        .map_err(|e| {
                            Error::new(
                                if e.limit_error().is_some() {
                                    ErrorCode::LimitExceeded
                                } else {
                                    ErrorCode::Malformed
                                },
                                "defaultExpression",
                                "invalid or over-budget expression syntax",
                            )
                        })?;
                }
                if minimum_si
                    .zip(*maximum_si)
                    .is_some_and(|(min, max)| min > max)
                {
                    return Err(Error::malformed("properties", "minimum exceeds maximum"));
                }
            }
            PropertyType::Text { max_bytes, default } => {
                bound(*max_bytes as usize, l.max_text_bytes, "maxBytes")?;
                if let Some(s) = default {
                    bound(s.len(), *max_bytes as usize, "default")?;
                }
            }
            PropertyType::Boolean { .. } => {}
        }
    }
    Ok(())
}
fn slots(
    slots: &[LocalContributionId],
    requirements: &[ContractRequirement],
    max: usize,
) -> Result<(), Error> {
    bound(slots.len(), max, "slots")?;
    unique(slots.iter(), "slots")?;
    for slot in slots {
        require_slot(slot, requirements)?;
    }
    Ok(())
}
fn require_slot(slot: &LocalContributionId, r: &[ContractRequirement]) -> Result<(), Error> {
    if r.iter().any(|r| &r.slot == slot) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidSelection,
            "slots",
            "undeclared requirement slot",
        ))
    }
}

impl Payload {
    /// Check schema bounds, role/property relationships and local slot declarations.
    /// Does not resolve providers, evaluate authored expressions or execute kernels.
    pub fn validate(&self, l: &Limits) -> Result<(), Error> {
        bound(
            self.requirements().len(),
            l.max_requirements,
            "requirements",
        )?;
        unique(self.requirements().iter().map(|r| &r.slot), "requirements")?;
        annotations(self.presentation(), l)?;
        match self {
            Self::Components(d) => {
                let s = &d.scientific;
                properties(&s.properties, l)?;
                let b = &s.bindings;
                match s.role {
                    ComponentRole::Data
                        if b.inertial_mass.is_some()
                            || b.source.is_some()
                            || b.response.is_some() =>
                    {
                        return Err(Error::malformed(
                            "bindings",
                            "data role cannot bind Dynamics or coupling",
                        ));
                    }
                    ComponentRole::Dynamics
                        if b.inertial_mass.is_none()
                            || b.source.is_some()
                            || b.response.is_some() =>
                    {
                        return Err(Error::malformed(
                            "bindings",
                            "Dynamics requires only inertialMass",
                        ));
                    }
                    ComponentRole::FieldCoupling
                        if b.inertial_mass.is_some()
                            || (b.source.is_none() && b.response.is_none()) =>
                    {
                        return Err(Error::malformed(
                            "bindings",
                            "coupling requires source and/or response",
                        ));
                    }
                    _ => {}
                }
                for (name, mass) in [
                    (b.inertial_mass.as_ref(), true),
                    (b.source.as_ref(), false),
                    (b.response.as_ref(), false),
                ] {
                    if let Some(name) = name {
                        let Some(p) = s.properties.iter().find(|p| &p.id == name) else {
                            return Err(Error::malformed("bindings", "unknown bound property"));
                        };
                        let PropertyType::Quantity { dimension, .. } = &p.schema else {
                            return Err(Error::malformed(
                                "bindings",
                                "bound property must be a scalar quantity",
                            ));
                        };
                        if !p.required || (mass && *dimension != Dimension::MASS) {
                            return Err(Error::malformed(
                                "bindings",
                                "bound property must be required and inertial mass must have mass dimension",
                            ));
                        }
                    }
                }
            }
            Self::Fields(d) => {
                let s = &d.scientific;
                if s.domain_dimension != 3 {
                    return Err(Error::new(
                        ErrorCode::UnsupportedVersion,
                        "domainDimension",
                        "v1 requires a three-dimensional domain",
                    ));
                }
                bound(
                    s.domain_requirements.len(),
                    l.max_schema_items,
                    "domainRequirements",
                )?;
                unique(s.domain_requirements.iter(), "domainRequirements")?;
                slots(&s.required_observables, &s.requirements, l.max_channels)?;
            }
            Self::Observables(d) => {
                let s = &d.scientific;
                nonempty(&s.meaning, l, "meaning")?;
                nonempty(&s.frame, l, "frame")?;
                nonempty(&s.conventions, l, "conventions")?;
                let rank = match s.shape {
                    Shape::Scalar => 0,
                    Shape::Vector { length } if (1..=16).contains(&length) => 1,
                    Shape::Matrix { rows, columns }
                        if (1..=16).contains(&rows) && (1..=16).contains(&columns) =>
                    {
                        2
                    }
                    _ => {
                        return Err(Error::malformed(
                            "shape",
                            "vector/matrix axes must be within 1..=16",
                        ));
                    }
                };
                if s.axes.len() != rank {
                    return Err(Error::malformed(
                        "axes",
                        "axis meanings must match shape rank",
                    ));
                }
                for a in &s.axes {
                    nonempty(a, l, "axes")?;
                }
            }
            Self::Constants(d) => {
                bound(
                    d.scientific.constants.len(),
                    l.max_schema_items,
                    "constants",
                )?;
                unique(d.scientific.constants.iter().map(|c| &c.id), "constants")?;
                for c in &d.scientific.constants {
                    nonempty(&c.meaning, l, "meaning")?;
                }
            }
            Self::FieldModels(d) => {
                let s = &d.scientific;
                if s.execution_contract != ExecutionContractId::Field {
                    return Err(Error::new(
                        ErrorCode::UnsupportedVersion,
                        "executionContract",
                        "field model requires field contract",
                    ));
                }
                properties(&s.configuration, l)?;
                require_slot(&s.field, &s.requirements)?;
                slots(&s.couplings, &s.requirements, l.max_requirements)?;
                slots(&s.observables, &s.requirements, l.max_channels)?;
                nonempty(&s.field_time_convention, l, "fieldTimeConvention")?;
                if s.max_state_bytes == 0 {
                    return Err(Error::malformed(
                        "maxStateBytes",
                        "field state bound must be positive",
                    ));
                }
            }
            Self::Integrators(d) => {
                let s = &d.scientific;
                if s.execution_contract != ExecutionContractId::Dynamics {
                    return Err(Error::new(
                        ErrorCode::UnsupportedVersion,
                        "executionContract",
                        "integrator requires dynamics contract",
                    ));
                }
                properties(&s.configuration, l)?;
                require_slot(&s.dynamics, &s.requirements)?;
                nonempty(&s.history.cold_start, l, "coldStart")?;
                if s.history.samples > 0 && s.history.max_bytes_per_entity == 0 {
                    return Err(Error::malformed(
                        "history",
                        "nonempty history needs a positive byte bound",
                    ));
                }
            }
        }
        crate::codec::encode(&self.project(), l, l.max_payload_bytes)?;
        Ok(())
    }
}

/// A root checked against caller limits. This is not a verified package or workload.
#[derive(Debug)]
pub struct ValidatedRelease<'a> {
    root: &'a Release,
    limits: Limits,
    artifacts: BTreeMap<ArtifactDigest, &'a Artifact>,
    contributions: BTreeMap<&'a LocalContributionId, &'a Contribution>,
}

impl Release {
    /// Check the manifest without fetching anything or choosing providers.
    /// Work is O((artifacts + contributions + requirements) log(n)); allocations
    /// are bounded indexes, not code/input buffers. Raw programmatic values are
    /// checked too, so bypassing a byte reader cannot bypass root validation.
    pub fn validate(&self, l: &Limits) -> Result<ValidatedRelease<'_>, Error> {
        if self.0.api_version().as_str() != "orishu.plugin/v1"
            || self.0.kind().as_str() != "PluginRelease"
        {
            return Err(Error::new(
                ErrorCode::UnsupportedVersion,
                "apiVersion/kind",
                "unsupported release resource",
            ));
        }
        let s = &self.0.spec;
        bound(s.artifacts.len(), l.max_artifacts, "artifacts")?;
        bound(s.contributions.len(), l.max_contributions, "contributions")?;
        nonempty(&self.0.metadata.version_label, l, "versionLabel")?;
        for t in [&self.0.metadata.display_name, &self.0.metadata.description]
            .into_iter()
            .flatten()
        {
            text(t, l, "metadata")?;
        }
        let mut artifacts = BTreeMap::new();
        let mut total = 0u64;
        for a in &s.artifacts {
            if a.size_bytes > l.max_artifact_bytes {
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "artifacts",
                    "artifact exceeds byte bound",
                ));
            }
            total = total
                .checked_add(a.size_bytes)
                .filter(|n| *n <= l.max_declared_bytes)
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::LimitExceeded,
                        "artifacts",
                        "aggregate bytes exceeded",
                    )
                })?;
            nonempty(&a.media_type, l, "mediaType")?;
            if !a.media_type.is_ascii()
                || a.media_type
                    .bytes()
                    .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
                || !a.media_type.contains('/')
            {
                return Err(Error::malformed(
                    "mediaType",
                    "expected nonempty type/subtype",
                ));
            }
            if artifacts.insert(a.digest, a).is_some() {
                return Err(Error::malformed("artifacts", "duplicate digest descriptor"));
            }
        }
        let mut contributions = BTreeMap::new();
        for c in &s.contributions {
            if contributions.insert(&c.local_id, c).is_some() {
                return Err(Error::malformed(
                    "contributions",
                    "duplicate local identity",
                ));
            }
            bound(c.requirements.len(), l.max_requirements, "requirements")?;
            unique(c.requirements.iter().map(Requirement::slot), "requirements")?;
            annotations(c.annotations.as_ref(), l)?;
            if !artifacts.contains_key(&c.payload) {
                return Err(Error::new(
                    ErrorCode::InvalidSelection,
                    "payload",
                    "payload descriptor absent",
                ));
            }
            if KnownPoint::from_id(&c.extension_point).is_some()
                && artifacts[&c.payload].size_bytes > l.max_payload_bytes as u64
            {
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "payload",
                    "known payload exceeds byte bound",
                ));
            }
            if let Some(icon) = c.annotations.as_ref().and_then(|a| a.icon_artifact)
                && !artifacts.contains_key(&icon)
            {
                return Err(Error::new(
                    ErrorCode::InvalidSelection,
                    "iconArtifact",
                    "icon descriptor absent",
                ));
            }
        }
        for c in &s.contributions {
            for r in &c.requirements {
                if let Requirement::Local {
                    local_contribution, ..
                } = r
                    && !contributions.contains_key(local_contribution)
                {
                    return Err(Error::new(
                        ErrorCode::InvalidSelection,
                        "requirements",
                        "local target absent",
                    ));
                }
            }
        }
        // Availability cycles are slice 2's dormant state, not root corruption.
        crate::codec::encode(&self.project(), l, l.max_manifest_bytes)?;
        Ok(ValidatedRelease {
            root: self,
            limits: *l,
            artifacts,
            contributions,
        })
    }
}

/// Verified contribution bytes. Opaque contributions are not executable/available.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedPayload {
    extension_point: ExtensionPointId,
    digest: ArtifactDigest,
    payload: Option<Box<Payload>>,
}

impl VerifiedPayload {
    /// Read-only understood declaration, or `None` for an opaque future point.
    /// No mutable accessor permits changing bytes while retaining verification.
    pub fn payload(&self) -> Option<&Payload> {
        self.payload.as_deref()
    }
    /// Exact externally supplied extension point/version.
    pub fn extension_point(&self) -> &ExtensionPointId {
        &self.extension_point
    }
    /// Identity of the actual verified bytes, including presentation.
    pub fn digest(&self) -> ArtifactDigest {
        self.digest
    }
}

impl ValidatedRelease<'_> {
    /// Root whose structural acceptance this handle witnesses.
    pub fn root(&self) -> &Release {
        self.root
    }
    /// Verify raw declared artifact bytes; location and media type grant no trust.
    pub fn verify_artifact(&self, digest: ArtifactDigest, bytes: &[u8]) -> Result<(), Error> {
        let a = self.artifacts.get(&digest).ok_or_else(|| {
            Error::new(ErrorCode::InvalidSelection, "artifact", "descriptor absent")
        })?;
        if bytes.len() as u64 != a.size_bytes || !digest.matches(bytes) {
            return Err(Error::new(
                ErrorCode::IntegrityMismatch,
                "artifact",
                "size or digest mismatch",
            ));
        }
        Ok(())
    }
    /// Verify one contribution. Local target semantic matching requires `verify_all`;
    /// provider availability, Wasm export inspection and numerical checks are later work.
    pub fn verify_payload(
        &self,
        id: &LocalContributionId,
        bytes: &[u8],
    ) -> Result<VerifiedPayload, Error> {
        let c = self.contributions.get(id).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidSelection,
                "contribution",
                "local identity absent",
            )
        })?;
        self.verify_artifact(c.payload, bytes)?;
        let Some(point) = KnownPoint::from_id(&c.extension_point) else {
            return Ok(VerifiedPayload {
                extension_point: c.extension_point.clone(),
                digest: c.payload,
                payload: None,
            });
        };
        let payload = payload_from_cbor(point, bytes, &self.limits)?;
        if payload.requirements().len() != c.requirements.len() {
            return Err(Error::malformed(
                "requirements",
                "payload and envelope dependencies differ",
            ));
        }
        for r in payload.requirements() {
            match c.requirements.iter().find(|e| e.slot() == &r.slot) {
                Some(Requirement::Contract { contract, .. }) if contract == &r.contract => {}
                Some(Requirement::Local { .. }) => {}
                _ => {
                    return Err(Error::malformed(
                        "requirements",
                        "payload and envelope dependency identities differ",
                    ));
                }
            }
        }
        if let Some(k) = payload.kernel()
            && !self.artifacts.contains_key(&k)
        {
            return Err(Error::new(
                ErrorCode::InvalidSelection,
                "kernel",
                "kernel descriptor absent",
            ));
        }
        if let Some(icon) = payload.presentation().and_then(|a| a.icon_artifact)
            && !self.artifacts.contains_key(&icon)
        {
            return Err(Error::new(
                ErrorCode::InvalidSelection,
                "iconArtifact",
                "payload icon descriptor absent",
            ));
        }
        Ok(VerifiedPayload {
            extension_point: c.extension_point.clone(),
            digest: c.payload,
            payload: Some(Box::new(payload)),
        })
    }
    /// Verify every root artifact and contribution from borrowed caller-held bytes.
    /// The returned map owns only small declarations, never executable/input blobs.
    /// Extra caller-held blobs are ignored, not implicitly added to the release.
    /// This is declaration closure, not selected workload closure or ABI admission.
    pub fn verify_all(
        &self,
        blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    ) -> Result<BTreeMap<LocalContributionId, VerifiedPayload>, Error> {
        for digest in self.artifacts.keys() {
            let bytes = blobs.get(digest).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidSelection,
                    "artifact",
                    "required bytes absent",
                )
            })?;
            self.verify_artifact(*digest, bytes)?;
        }
        let mut result = BTreeMap::new();
        let mut kernel_contracts = BTreeMap::new();
        for c in self.contributions.values() {
            let p = self.verify_payload(&c.local_id, blobs[&c.payload])?;
            if let Some(p) = p.payload()
                && let Some(k) = p.kernel()
            {
                let contract = match p {
                    Payload::FieldModels(_) => ExecutionContractId::Field,
                    _ => ExecutionContractId::Dynamics,
                };
                if kernel_contracts
                    .insert(k, contract)
                    .is_some_and(|previous| previous != contract)
                {
                    return Err(Error::malformed(
                        "kernel",
                        "one artifact declares two execution contracts",
                    ));
                }
            }
            result.insert(c.local_id.clone(), p);
        }
        // Hash each scientific declaration once, not once per incoming local
        // edge (up to contributions * requirements edges in a hostile root).
        let contracts: BTreeMap<_, _> = result
            .iter()
            .filter_map(|(id, p)| {
                p.payload()
                    .map(|p| p.contract_ref(&self.limits).map(|r| (id, r)))
            })
            .collect::<Result<_, _>>()?;
        for c in self.contributions.values() {
            let Some(p) = result[&c.local_id].payload() else {
                continue;
            };
            for r in &c.requirements {
                if let Requirement::Local {
                    slot,
                    local_contribution,
                } = r
                {
                    // Unknown providers leave this dependency dormant; the resolver
                    // may diagnose unavailability but may not invent scientific data.
                    if let Some(target) = contracts.get(local_contribution) {
                        let expected = &p
                            .requirements()
                            .iter()
                            .find(|r| &r.slot == slot)
                            .expect("checked payload slots")
                            .contract;
                        if target != expected {
                            return Err(Error::malformed(
                                "requirements",
                                "local target scientific identity mismatch",
                            ));
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}
