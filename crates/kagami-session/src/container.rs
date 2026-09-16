//! Scientific experiment file version 4: `document.json` plus immutable blobs.
//!
//! This pure codec restores captured state without installed code or a runtime.
//! The returned `ExperimentDocument` is normalized to the existing in-memory
//! document version, just as the JSON v1/v2 reader normalizes older documents.
//! Its scientific setup still refuses bare JSON serialization: use this codec.
//! No automatic conversion of legacy domain/boundary semantics is performed.

use crate::document::{
    DocumentError, DocumentMetadata, ExperimentDocument, FORMAT, FORMAT_VERSION, StoredDefaultView,
    StoredExperiment,
};
use kagami_document::{
    Setup,
    scientific::{KernelCapture, ScientificDescription, ScientificLimits, ScientificSetup},
};
use orishu_plugin::{
    ArtifactDigest,
    archive::{self, ArchiveLimits, Root},
    selected::{self, SelectionLimits},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
mod metadata;
pub use metadata::MetadataLimits;

/// On-disk scientific container version; JSON v1–v3 remain separate readable forms.
pub const CONTAINER_VERSION: u32 = 4;

/// Explicit byte/count limits applied before copying opaque blobs into a draft.
#[derive(Clone, Copy, Debug)]
pub struct ContainerLimits {
    /// Strict archive framing/physical byte limits.
    pub archive: ArchiveLimits,
    /// Exact selected declaration closure limits.
    pub selection: SelectionLimits,
    /// Captured setup limits, counting repeated input references conservatively.
    pub scientific: ScientificLimits,
    /// Streaming JSON preflight bounds, before constructing typed collections.
    pub metadata: MetadataLimits,
}
impl Default for ContainerLimits {
    fn default() -> Self {
        Self {
            archive: ArchiveLimits {
                max_bytes: 512 * 1024 * 1024,
                max_entries: 8192,
                root_bytes: 16 * 1024 * 1024,
                blob_bytes: 128 * 1024 * 1024,
                total_blob_bytes: 512 * 1024 * 1024,
            },
            selection: SelectionLimits::default(),
            scientific: ScientificLimits::default(),
            metadata: MetadataLimits::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Container {
    format: String,
    format_version: u32,
    metadata: DocumentMetadata,
    experiment: StoredExperiment<ScientificDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_view: Option<StoredDefaultView>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    format: String,
    format_version: u32,
}

fn malformed(message: impl ToString) -> DocumentError {
    DocumentError::Malformed {
        message: message.to_string(),
    }
}
fn scientific(error: kagami_document::scientific::ScientificError) -> DocumentError {
    DocumentError::Invalid {
        source: error.into(),
    }
}
fn limit() -> DocumentError {
    scientific(kagami_document::scientific::ScientificError::Limit)
}
fn framing(source: orishu_plugin::Error) -> DocumentError {
    DocumentError::Container {
        source: Box::new(source),
    }
}

/// Encode a scientific draft, preserving captured bytes and exact declaration
/// evidence. Executables and unrelated plugin contributions are not embedded.
/// Legacy drafts use the JSON writer; choosing a scientific domain is an edit.
/// No kernel executes here and no current inventory is consulted.
pub fn encode(
    document: &ExperimentDocument,
    limits: ContainerLimits,
) -> Result<Vec<u8>, DocumentError> {
    if document.format != FORMAT {
        return Err(DocumentError::WrongFormat {
            found: document.format.clone(),
            expected: FORMAT,
        });
    }
    if document.format_version != FORMAT_VERSION {
        return Err(DocumentError::UnsupportedVersion {
            found: document.format_version,
            supported: FORMAT_VERSION,
        });
    }
    let setup = document
        .experiment
        .setup
        .scientific()
        .ok_or_else(|| malformed("container requires explicit scientific setup"))?;
    let description = setup.describe();
    if description.kernels.len() > limits.scientific.kernels {
        return Err(limit());
    }
    let evidence = setup
        .declarations()
        .blobs(&limits.selection.declarations)
        .map_err(malformed)?;
    let mut blobs: BTreeMap<_, &[u8]> = evidence
        .iter()
        .map(|(id, bytes)| (*id, bytes.as_slice()))
        .collect();
    for (id, capture) in setup.captures() {
        let d = description
            .kernels
            .iter()
            .find(|d| &d.context.instance == id)
            .expect("constructed capture");
        blobs.insert(d.context.configuration.digest, &capture.configuration);
        blobs.insert(d.state.digest, &capture.state);
        if let (Some(identity), Some(bytes)) = (&d.history_entities, &capture.history_entities) {
            blobs.insert(identity.digest, bytes);
        }
    }
    let root = Container {
        format: FORMAT.into(),
        format_version: CONTAINER_VERSION,
        metadata: document.metadata.clone(),
        default_view: document.default_view.clone(),
        experiment: StoredExperiment {
            counters: document.experiment.counters,
            setup: description,
            variables: document.experiment.variables.clone(),
            objects: document.experiment.objects.clone(),
        },
    };
    // Bound serialization as it writes, not after an unbounded output allocation.
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        limit: limits.archive.root_bytes,
    };
    serde_json::to_writer_pretty(&mut writer, &root).map_err(malformed)?;
    let bytes =
        archive::pack(Root::Document, &writer.bytes, &blobs, limits.archive).map_err(framing)?;
    // A stricter caller policy must not produce an unreadable document.
    decode(&bytes, limits)?;
    Ok(bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(std::io::Error::other("document metadata budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Decode strict stored ZIP, independently verify declaration membership and all
/// input identities, and restore bytes without initialization. Call ordinary
/// `into_experiment`/authority Open to validate authored expressions and objects.
/// Declined versions are not classified as damage for backup recovery.
pub fn decode(bytes: &[u8], limits: ContainerLimits) -> Result<ExperimentDocument, DocumentError> {
    let archive = archive::read(bytes, Root::Document, limits.archive).map_err(framing)?;
    let header: Header = serde_json::from_slice(archive.root).map_err(malformed)?;
    if header.format != FORMAT {
        return Err(DocumentError::WrongFormat {
            found: header.format,
            expected: FORMAT,
        });
    }
    if header.format_version != CONTAINER_VERSION {
        return Err(DocumentError::UnsupportedVersion {
            found: header.format_version,
            supported: CONTAINER_VERSION,
        });
    }
    metadata::check(archive.root, limits)?;
    let root: Container = serde_json::from_slice(archive.root).map_err(malformed)?;
    let d = &root.experiment.setup;
    if d.api_version != "kagami.scientific-setup/v1" {
        return Err(DocumentError::Invalid {
            source: kagami_document::scientific::ScientificError::Mismatch.into(),
        });
    }
    if d.kernels.len() > limits.scientific.kernels {
        return Err(limit());
    }
    let declarations =
        selected::verify_declarations(d.selection.clone(), &archive.blobs, limits.selection)
            .map_err(|error| scientific(error.into()))?;
    let evidence = declarations
        .blobs(&limits.selection.declarations)
        .map_err(malformed)?;
    let mut used: BTreeSet<ArtifactDigest> = evidence.keys().copied().collect();
    let mut total = 0usize;
    // Check aggregate references before allocating *any* owned opaque bytes.
    for k in &d.kernels {
        for identity in [&k.context.configuration, &k.state]
            .into_iter()
            .chain(k.history_entities.iter())
        {
            let bytes = archive
                .blobs
                .get(&identity.digest)
                .ok_or_else(|| malformed("referenced scientific blob is absent"))?;
            if identity.byte_length != bytes.len() as u64 {
                return Err(malformed("scientific blob length mismatch"));
            }
            if bytes.len() > limits.scientific.blob_bytes {
                return Err(limit());
            }
            total = total
                .checked_add(bytes.len())
                .filter(|n| *n <= limits.scientific.total_bytes)
                .ok_or_else(limit)?;
            used.insert(identity.digest);
        }
    }
    if used.len() != archive.blobs.len() || archive.blobs.keys().any(|id| !used.contains(id)) {
        return Err(malformed("container contains unrelated blobs"));
    }
    for (digest, bytes) in &archive.blobs {
        if !digest.matches(bytes) {
            return Err(malformed("container blob digest mismatch"));
        }
    }
    // One allocation per distinct captured blob, shared between all references.
    let mut owned = BTreeMap::<ArtifactDigest, Arc<[u8]>>::new();
    let mut get = |id: ArtifactDigest| {
        owned
            .entry(id)
            .or_insert_with(|| Arc::from(archive.blobs[&id]))
            .clone()
    };
    let captures = d
        .kernels
        .iter()
        .map(|k| KernelCapture {
            context: k.context.clone(),
            authored: k.authored.clone(),
            configuration: get(k.context.configuration.digest),
            state: get(k.state.digest),
            state_values: k.state.value_count,
            history_entities: k.history_entities.as_ref().map(|id| get(id.digest)),
        })
        .collect();
    let setup = ScientificSetup::restore(
        &declarations,
        d.domain.clone(),
        d.time_step,
        captures,
        limits.scientific,
    )
    .map_err(scientific)?;
    // Compares format/schema/count as well as hashes; enforces canonical instance
    // order and prevents a forged history/state descriptor surviving a roundtrip.
    if setup.describe() != *d {
        return Err(malformed("captured scientific descriptors disagree"));
    }
    Ok(ExperimentDocument {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        metadata: root.metadata,
        default_view: root.default_view,
        experiment: StoredExperiment {
            counters: root.experiment.counters,
            setup: Setup::Scientific(Arc::new(setup)),
            objects: root.experiment.objects,
            variables: root.experiment.variables,
        },
    })
}
