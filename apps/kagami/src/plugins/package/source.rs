//! Bounded deterministic source-local compilation; no ambient provider lookup.
use super::*;
use orishu_plugin::source::SourcePayload;

pub(super) enum Input {
    Exact(Box<Payload>),
    Local(SourcePayload),
    Opaque(Vec<u8>),
}

pub(super) enum Compiled {
    Exact(Box<Payload>),
    Opaque(Vec<u8>),
}

impl Input {
    pub(super) fn compile(
        self,
        contracts: &BTreeMap<LocalContributionId, ScientificContractRef>,
        artifacts: &BTreeMap<LocalContributionId, ArtifactDigest>,
        limits: &Limits,
    ) -> Result<Compiled, Error> {
        Ok(match self {
            Self::Local(draft) => {
                Compiled::Exact(Box::new(draft.compile(contracts, artifacts, limits)?))
            }
            Self::Exact(payload) => Compiled::Exact(payload),
            Self::Opaque(bytes) => Compiled::Opaque(bytes),
        })
    }
}

// At most 256 declarations, with bounded requirement lists. Scanning ready nodes
// is O((V² + V×E) log V) worst case, cold packaging work only. No recursive graph walk or
// filesystem re-read; success removes one node on every iteration.
pub(super) fn ready(
    pending: &BTreeMap<LocalContributionId, (SourceContribution, Input)>,
    contracts: &BTreeMap<LocalContributionId, ScientificContractRef>,
) -> Result<LocalContributionId, Error> {
    for (id, (_, input)) in pending {
        let Input::Local(draft) = input else {
            return Ok(id.clone());
        };
        if draft
            .dependencies()
            .iter()
            .all(|dependency| contracts.contains_key(dependency))
        {
            return Ok(id.clone());
        }
    }
    Err(Error::new(
        Code::InvalidSelection,
        "source-local contract dependency is missing, opaque or cyclic",
    ))
}
