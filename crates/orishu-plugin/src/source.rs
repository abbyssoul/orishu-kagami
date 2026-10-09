//! Pure lowering of source-local aliases to ordinary exact declarations.
//!
//! This is authoring tooling, not installed-release or workload resolution. The
//! caller supplies only explicitly declared local contracts/artifacts; no names
//! cause inventory lookup, filesystem access, building or guest execution.
use crate::{
    ArtifactDigest, Error, KnownPoint, Limits, LocalContributionId, Payload, ScientificContractRef,
    codec, payload_from_json,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A bounded source declaration, not a validated scientific payload.
/// Only requirement contracts, model/integrator kernels and presentation icons
/// accept aliases. Ordinary exact declarations remain accepted unchanged.
#[derive(Debug)]
pub struct SourcePayload {
    point: KnownPoint,
    value: Value,
    dependencies: BTreeSet<LocalContributionId>,
}

impl SourcePayload {
    /// Read with the normal byte/tree/collection limits and duplicate-key refusal.
    /// `localContribution` and `localArtifact` markers must be single-key objects
    /// containing a valid local identifier. Scientific validation follows lowering.
    pub fn read(point: KnownPoint, bytes: &[u8], limits: &Limits) -> Result<Self, Error> {
        let value: Value = codec::structured_from_json(bytes, limits, limits.max_payload_bytes)?;
        let scientific = value
            .get("scientific")
            .and_then(Value::as_object)
            .ok_or_else(malformed)?;
        let requirements = scientific
            .get("requirements")
            .and_then(Value::as_array)
            .ok_or_else(malformed)?;
        let mut dependencies = BTreeSet::new();
        for requirement in requirements {
            let contract = requirement.get("contract").ok_or_else(malformed)?;
            if let Some(id) = alias(contract, "localContribution")? {
                dependencies.insert(id);
            }
        }
        if matches!(point, KnownPoint::FieldModels | KnownPoint::Integrators) {
            alias(
                scientific.get("kernel").ok_or_else(malformed)?,
                "localArtifact",
            )?;
        }
        if let Some(icon) = value
            .get("presentation")
            .and_then(|p| p.get("iconArtifact"))
        {
            alias(icon, "localArtifact")?;
        }
        Ok(Self {
            point,
            value,
            dependencies,
        })
    }

    /// Exact local contribution IDs required before this payload can be lowered.
    /// An opaque contribution cannot supply a scientific contract.
    pub fn dependencies(&self) -> &BTreeSet<LocalContributionId> {
        &self.dependencies
    }

    /// Consume a source draft and validate its exact lowered declaration.
    /// Missing aliases, malformed fields, wrong contracts and expanded output
    /// exceeding the receiving limits refuse; no partial payload is returned.
    pub fn compile(
        mut self,
        contracts: &BTreeMap<LocalContributionId, ScientificContractRef>,
        artifacts: &BTreeMap<LocalContributionId, ArtifactDigest>,
        limits: &Limits,
    ) -> Result<Payload, Error> {
        let scientific = self.value.get_mut("scientific").unwrap();
        for requirement in scientific
            .get_mut("requirements")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            let contract = requirement.get_mut("contract").ok_or_else(malformed)?;
            if let Some(id) = alias(contract, "localContribution")? {
                let exact = contracts.get(&id).ok_or_else(|| {
                    Error::malformed("source", "missing source-local scientific contract")
                })?;
                *contract = serde_json::to_value(exact).map_err(|_| malformed())?;
            }
        }
        if matches!(
            self.point,
            KnownPoint::FieldModels | KnownPoint::Integrators
        ) {
            lower_artifact(scientific.get_mut("kernel").unwrap(), artifacts)?;
        }
        if let Some(icon) = self
            .value
            .get_mut("presentation")
            .and_then(|p| p.get_mut("iconArtifact"))
        {
            lower_artifact(icon, artifacts)?;
        }
        // Re-enter the ordinary public decoder: aliases never bypass scientific
        // validation or acquire a distinct identity projection.
        let bytes = serde_json::to_vec(&self.value).map_err(|_| malformed())?;
        payload_from_json(self.point, &bytes, limits)
    }
}

fn malformed() -> Error {
    Error::malformed("source", "invalid source declaration or local alias")
}

fn alias(value: &Value, key: &str) -> Result<Option<LocalContributionId>, Error> {
    let Some(marker) = value.get(key) else {
        return Ok(None);
    };
    if value.as_object().is_none_or(|map| map.len() != 1) {
        return Err(malformed());
    }
    Ok(Some(LocalContributionId::new(
        marker.as_str().ok_or_else(malformed)?,
    )?))
}

fn lower_artifact(
    value: &mut Value,
    artifacts: &BTreeMap<LocalContributionId, ArtifactDigest>,
) -> Result<(), Error> {
    if let Some(id) = alias(value, "localArtifact")? {
        let digest = artifacts
            .get(&id)
            .ok_or_else(|| Error::malformed("source", "missing source-local artifact"))?;
        *value = Value::String(digest.to_string());
    }
    Ok(())
}
