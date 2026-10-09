//! Version 5: explicit standalone dependency intent alongside legacy or captured
//! setup, in the same flat archive. No nested documents or inventory lookups.
use super::*;
use orishu_plugin::authoring_lock::SelectionLock;

/// Standalone-lock container version. Unlocked JSON v3/scientific v4 are unchanged.
pub const LOCKED_CONTAINER_VERSION: u32 = 5;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockRef {
    digest: ArtifactDigest,
    byte_length: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
enum StoredSetup {
    Legacy(kagami_document::setup::LegacySetup),
    Scientific(Box<ScientificDescription>),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContainerV5 {
    format: String,
    format_version: u32,
    metadata: DocumentMetadata,
    dependencies: LockRef,
    experiment: StoredExperiment<StoredSetup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_view: Option<StoredDefaultView>,
}

pub(super) fn encode(
    document: &ExperimentDocument,
    limits: ContainerLimits,
) -> Result<Vec<u8>, DocumentError> {
    let lock = document
        .dependencies
        .as_ref()
        .expect("v5 dispatch requires lock");
    let bytes = lock.to_cbor(limits.dependencies).map_err(framing)?;
    let identity = LockRef {
        digest: ArtifactDigest::sha256_of(&bytes),
        byte_length: bytes.len() as u64,
    };
    let (setup, mut blobs) = match &document.experiment.setup {
        Setup::Legacy(legacy) => (StoredSetup::Legacy(legacy.clone()), CapturedBlobs::new()),
        Setup::Scientific(scientific) => {
            let (description, blobs) = capture_blobs(scientific, limits)?;
            (StoredSetup::Scientific(Box::new(description)), blobs)
        }
    };
    blobs.insert(identity.digest, Arc::from(bytes));
    let root = ContainerV5 {
        format: FORMAT.into(),
        format_version: LOCKED_CONTAINER_VERSION,
        metadata: document.metadata.clone(),
        default_view: document.default_view.clone(),
        dependencies: identity,
        experiment: StoredExperiment {
            counters: document.experiment.counters,
            setup,
            variables: document.experiment.variables.clone(),
            objects: document.experiment.objects.clone(),
        },
    };
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        limit: limits.archive.root_bytes,
    };
    serde_json::to_writer_pretty(&mut writer, &root).map_err(malformed)?;
    let borrowed = blobs
        .iter()
        .map(|(id, bytes)| (*id, bytes.as_ref()))
        .collect();
    let bytes =
        archive::pack(Root::Document, &writer.bytes, &borrowed, limits.archive).map_err(framing)?;
    super::decode(&bytes, limits)?;
    Ok(bytes)
}

pub(super) fn decode(
    root: &[u8],
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    limits: ContainerLimits,
) -> Result<ExperimentDocument, DocumentError> {
    metadata::check(root, limits)?;
    let root: ContainerV5 = serde_json::from_slice(root).map_err(malformed)?;
    let d = root.dependencies;
    if d.byte_length > limits.dependencies.bytes as u64 {
        return Err(limit());
    }
    let bytes = blobs
        .get(&d.digest)
        .ok_or_else(|| malformed("dependency lock blob missing"))?;
    if d.byte_length != bytes.len() as u64 || !d.digest.matches(bytes) {
        return Err(malformed("dependency lock identity mismatch"));
    }
    let lock = SelectionLock::from_cbor(bytes, limits.dependencies).map_err(framing)?;
    let setup = match root.experiment.setup {
        StoredSetup::Legacy(legacy) => {
            if blobs.len() != 1 {
                return Err(malformed("container contains unrelated blobs"));
            }
            Setup::Legacy(legacy)
        }
        StoredSetup::Scientific(scientific) => Setup::Scientific(Arc::new(restore_scientific(
            &scientific,
            blobs,
            limits,
            BTreeSet::from([d.digest]),
        )?)),
    };
    Ok(ExperimentDocument {
        dependencies: Some(Arc::new(lock)),
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        metadata: root.metadata,
        default_view: root.default_view,
        experiment: StoredExperiment {
            counters: root.experiment.counters,
            setup,
            objects: root.experiment.objects,
            variables: root.experiment.variables,
        },
    })
}
