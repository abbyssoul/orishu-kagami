//! Safe replacement of catalog source files.
//!
//! Three properties are non-negotiable here, because the file being edited is
//! a scientist's hand-authored data:
//!
//! - **Atomic.** Every write goes to a temporary file in the same directory
//!   and is renamed over the target, so a crash or a full disk leaves the
//!   previous file intact rather than a truncated one.
//! - **Conflict-checked.** A write names the file digest it was based on. If
//!   the file changed since it was read — another editor, another Kagami, a
//!   `git checkout` — the write is refused and nothing is touched.
//! - **Sibling-preserving.** One file may hold many templates. Editing one
//!   document splices only that document's text, so its neighbours keep their
//!   comments and formatting exactly. If the file's layout cannot be spliced
//!   safely, the writer re-emits every document canonically instead, and
//!   verifies the result parses back to the same documents before replacing
//!   anything.
//!
//! Path handling is equally deliberate, and deliberately *physical* rather
//! than only lexical. A target is always a relative path resolved under the
//! catalog root, and `..`, absolute paths, and non-catalog extensions are
//! refused — but refusing those alone would still let a symlinked parent
//! directory redirect a write outside the root.
//!
//! So [`CatalogRoot`] never hands the filesystem a path assembled from the
//! root and untrusted components as a unit. It canonicalises the configured
//! root, then walks the requested components one at a time: each must be a
//! real directory (`symlink_metadata` does not follow, so a link is refused
//! rather than traversed) whose canonical form still lies beneath the root,
//! and the walk continues from that canonical directory rather than from the
//! requested spelling. A missing directory is created only *below* a component
//! already proven, and is removed again if a later step fails. The final
//! target must be a regular file. `OpenOptions::create_new` is the
//! exclusive-creation mechanism the temporary file relies on, so a
//! pre-existing temporary path can neither be followed nor clobbered.
//!
//! **The race boundary is explicit.** These are path-based `std` operations,
//! so each check and the use that follows it are separate syscalls. That is
//! enough to contain a *pre-existing* hostile link, which is what an untrusted
//! MCP client can plant. It is not enough against a process concurrently
//! swapping a directory for a link between one step and the next: closing that
//! would need directory-handle-relative, no-follow operations (`openat` with
//! `RESOLVE_BENEATH`, or the platform's equivalent), which this crate does not
//! claim.

use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::load::is_catalog_file;
use crate::source::{DocumentOrdinal, SourceLocation, TemplateIdentity};
use crate::template::Template;

/// SHA-256 of a catalog file's complete bytes, used as the write conflict
/// authority.
///
/// Timestamps are deliberately not trusted: editors preserve them, and some
/// filesystems have a coarser resolution than the interval between two of a
/// user's edits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileDigest([u8; 32]);

impl FileDigest {
    /// Digest a file's contents.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}

impl std::fmt::Display for FileDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for FileDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "FileDigest({self})")
    }
}

/// The exact document an edit or removal targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteTarget {
    /// The catalog-root-relative file.
    pub file: PathBuf,
    /// Which document within that file's stream.
    pub document: DocumentOrdinal,
    /// The identity that document is expected to carry, verified after the
    /// digest check so two same-named documents can never alias.
    pub identity: TemplateIdentity,
}

impl WriteTarget {
    /// Build a target from a loaded entry's source location and identity.
    pub fn new(source: &SourceLocation, identity: TemplateIdentity) -> Self {
        Self {
            file: source.file.clone(),
            document: source.document,
            identity,
        }
    }
}

/// Why a catalog write was refused. A failed write never modifies the file.
#[derive(Debug, Error)]
pub enum WriteError {
    /// The filesystem refused the operation.
    #[error("catalog file operation failed: {0}")]
    Io(#[source] std::io::Error),
    /// The target path is lexically not a catalog file inside the catalog
    /// root: absolute, containing `..`, or carrying a non-catalog extension.
    #[error("`{0}` is not a relative catalog file path inside the catalog directory")]
    UnsafePath(PathBuf),
    /// The target path is lexically fine but does not *physically* resolve to
    /// a regular file inside the catalog root: a symbolic link somewhere along
    /// it leaves the root, or the target itself is a link, a directory, or
    /// another special file.
    #[error("`{0}` does not resolve to a regular file inside the catalog directory")]
    NotContained(PathBuf),
    /// The file changed since the caller read it.
    #[error("catalog file `{0}` changed since it was read")]
    SourceModified(PathBuf),
    /// The caller expected a file that does not exist.
    #[error("catalog file `{0}` does not exist")]
    SourceMissing(PathBuf),
    /// The caller expected to create a file that already exists.
    #[error("catalog file `{0}` already exists")]
    SourceAlreadyExists(PathBuf),
    /// The file is read-only.
    #[error("catalog file `{0}` is read-only")]
    ReadOnly(PathBuf),
    /// The targeted document is not there, or does not carry the expected
    /// identity.
    #[error("document {document} of `{file}` is not `{identity}`")]
    TargetMismatch {
        file: PathBuf,
        document: DocumentOrdinal,
        identity: TemplateIdentity,
    },
    /// The file could not be parsed as a YAML stream, so its documents could
    /// not be counted or rewritten.
    #[error("catalog file `{file}` is not a readable YAML stream: {message}")]
    UnreadableStream { file: PathBuf, message: String },
    /// The rewritten text did not parse back to the documents it was built
    /// from, so it was discarded rather than written.
    #[error("rewriting `{0}` would not have round-tripped, so nothing was written")]
    WouldNotRoundTrip(PathBuf),
}

/// The catalog directory an authority owns, and the only place its file
/// effects may land.
///
/// The configured path is kept as the user wrote it — it is what appears in
/// configuration and in an operator's mental model — while containment is
/// decided against its *canonical* form, resolved afresh for each operation
/// because a catalog directory may be created, replaced, or relinked while
/// Kagami is running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRoot {
    configured: PathBuf,
}

/// A path proved to name a regular file inside a [`CatalogRoot`], together
/// with the canonical directory holding it.
///
/// Carrying the directory rather than re-deriving it is what lets the atomic
/// replacement put its temporary file in a directory that has already been
/// checked, instead of trusting `path.parent()` a second time.
#[derive(Debug)]
struct ContainedPath {
    directory: PathBuf,
    path: PathBuf,
    /// Directories this resolution created, shallowest first, so a later
    /// failure can undo exactly what it added and nothing else.
    created: Vec<PathBuf>,
}

impl ContainedPath {
    /// Remove the directories this resolution created, deepest first.
    ///
    /// `remove_dir` refuses a non-empty directory, so a concurrent writer's
    /// file keeps the directory it needs; and only paths this resolution
    /// created are listed, so nothing pre-existing is ever removed.
    fn undo_created_directories(&self) {
        undo_created_directories(&self.created);
    }
}

fn undo_created_directories(created: &[PathBuf]) {
    for directory in created.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}

impl CatalogRoot {
    /// Take ownership of `path` as a catalog directory.
    ///
    /// The directory does not have to exist: an installation with no catalog
    /// is a normal state, and creating the first template creates it.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            configured: path.into(),
        }
    }

    /// The configured directory, as it was supplied.
    pub fn path(&self) -> &Path {
        &self.configured
    }

    /// Resolve `file` to a path physically inside this root, creating nothing.
    ///
    /// `Ok(None)` means nothing exists at that path yet because the root or
    /// one of the target's directories is absent — a normal answer for a read
    /// or a digest, not a failure.
    fn contained(&self, file: &Path) -> Result<Option<ContainedPath>, WriteError> {
        let mut created = Vec::new();
        let resolved = self.walk(file, false, &mut created);
        debug_assert!(created.is_empty(), "a read must not create a directory");
        resolved
    }

    /// Resolve `file`, establishing the catalog root and any missing parent
    /// directories one *proven* component at a time.
    ///
    /// Nothing is created until the component above it has been shown to be a
    /// real directory beneath the canonical root, so a symlinked component
    /// cannot carry a creation outside. If any later step fails, the
    /// directories this call added are removed again.
    fn contained_for_create(&self, file: &Path) -> Result<ContainedPath, WriteError> {
        let mut created = Vec::new();
        match self.walk(file, true, &mut created) {
            Ok(Some(mut contained)) => {
                contained.created = created;
                Ok(contained)
            }
            // Unreachable while creating: a missing component is created
            // rather than reported. Refusing beats asserting.
            Ok(None) => {
                undo_created_directories(&created);
                Err(WriteError::SourceMissing(file.to_owned()))
            }
            Err(error) => {
                undo_created_directories(&created);
                Err(error)
            }
        }
    }

    /// Resolve `file`, treating an absent root or directory as a missing file.
    fn existing(&self, file: &Path) -> Result<ContainedPath, WriteError> {
        self.contained(file)?
            .ok_or_else(|| WriteError::SourceMissing(file.to_owned()))
    }

    /// Walk `file`'s components down from the canonical root, proving each one
    /// before stepping into it.
    ///
    /// The joined path `root/<relative>` is never handed to the filesystem as
    /// a unit: every component is checked with `symlink_metadata`, which does
    /// not follow, and the walk continues from the *canonical* directory it
    /// proved rather than from the requested spelling. That is what keeps a
    /// symlinked component from redirecting either a lookup or a creation.
    fn walk(
        &self,
        file: &Path,
        creating: bool,
        created: &mut Vec<PathBuf>,
    ) -> Result<Option<ContainedPath>, WriteError> {
        let unsafe_path = || WriteError::UnsafePath(file.to_owned());
        if file.is_absolute() || !is_catalog_file(file) {
            return Err(unsafe_path());
        }
        if !file
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(unsafe_path());
        }
        let name = file.file_name().ok_or_else(unsafe_path)?;

        // The configured root is trusted configuration, so a symlink used as
        // the catalog directory is an intentional, supported setup. That
        // exception stops here: it does not extend to any component a command
        // supplied.
        let Some(root) = self.canonical_root(creating, created)? else {
            return Ok(None);
        };

        let mut directory = root.clone();
        let parent = file.parent().unwrap_or_else(|| Path::new(""));
        for component in parent.components() {
            let candidate = directory.join(component);
            match fs::symlink_metadata(&candidate) {
                // A component that exists must be a real directory. A
                // symbolic link reports as one here whatever it points at,
                // and is refused before anything below it is created.
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(WriteError::NotContained(file.to_owned()));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if !creating {
                        return Ok(None);
                    }
                    match fs::create_dir(&candidate) {
                        Ok(()) => created.push(candidate.clone()),
                        // Someone else created it between the check and the
                        // call; fall through and judge what is actually there.
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(WriteError::Io(error)),
                    }
                    let metadata = fs::symlink_metadata(&candidate).map_err(WriteError::Io)?;
                    if !metadata.is_dir() {
                        return Err(WriteError::NotContained(file.to_owned()));
                    }
                }
                Err(error) => return Err(WriteError::Io(error)),
            }
            let canonical = fs::canonicalize(&candidate).map_err(WriteError::Io)?;
            if !canonical.starts_with(&root) {
                return Err(WriteError::NotContained(file.to_owned()));
            }
            directory = canonical;
        }

        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            // A catalog file must be a regular file, even when a link would
            // resolve inside the root: the atomic replacement below renames
            // over the target, which would destroy the link rather than edit
            // what it points at.
            Ok(metadata) if !metadata.is_file() => {
                return Err(WriteError::NotContained(file.to_owned()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(WriteError::Io(error)),
        }
        Ok(Some(ContainedPath {
            directory,
            path,
            created: Vec::new(),
        }))
    }

    /// The canonical catalog root, created when a write needs it.
    fn canonical_root(
        &self,
        creating: bool,
        created: &mut Vec<PathBuf>,
    ) -> Result<Option<PathBuf>, WriteError> {
        match fs::canonicalize(&self.configured) {
            Ok(path) => Ok(Some(path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !creating {
                    return Ok(None);
                }
                // `create_dir_all` is safe on the configured path precisely
                // because it is configuration rather than command input. Any
                // directory it makes *above* the catalog root belongs to the
                // operator, so only the root itself is recorded for undo.
                fs::create_dir_all(&self.configured).map_err(WriteError::Io)?;
                let root = fs::canonicalize(&self.configured).map_err(WriteError::Io)?;
                created.push(root.clone());
                Ok(Some(root))
            }
            Err(error) => Err(WriteError::Io(error)),
        }
    }
}

/// Add a template to `file`, creating the file when `expected` is `None`.
///
/// Passing `Some(digest)` appends to an existing file whose contents still
/// hash to `digest`; passing `None` requires the file not to exist. That
/// distinction is what makes a repeated create idempotent for the authority
/// rather than silently doubling an entry.
pub fn create_entry(
    root: &CatalogRoot,
    file: &Path,
    template: &Template,
    expected: Option<FileDigest>,
) -> Result<(), WriteError> {
    match expected {
        None => {
            let contained = root.contained_for_create(file)?;
            let written = if contained.path.exists() {
                Err(WriteError::SourceAlreadyExists(file.to_owned()))
            } else {
                let document = canonical_document(template);
                replace_atomically(&contained, &format!("---\n{document}"))
            };
            // A refused create leaves no directory behind either.
            if written.is_err() {
                contained.undo_created_directories();
            }
            written
        }
        Some(expected) => {
            let contained = root.existing(file)?;
            let text = read_existing(&contained, file, expected)?;
            let mut stream = Stream::split(file, &text)?;
            stream.documents.push(canonical_document(template));
            commit(&contained, file, stream)
        }
    }
}

/// Replace exactly one document with `template`, keeping its position in the
/// file and every sibling document's text.
pub fn update_entry(
    root: &CatalogRoot,
    target: &WriteTarget,
    template: &Template,
    expected: FileDigest,
) -> Result<(), WriteError> {
    let contained = root.existing(&target.file)?;
    let text = read_existing(&contained, &target.file, expected)?;
    let mut stream = Stream::split(&target.file, &text)?;
    let index = stream.check_target(target)?;
    stream.documents[index] = canonical_document(template);
    commit(&contained, &target.file, stream)
}

/// Remove exactly one document. Removing the last document removes the file.
pub fn delete_entry(
    root: &CatalogRoot,
    target: &WriteTarget,
    expected: FileDigest,
) -> Result<(), WriteError> {
    let contained = root.existing(&target.file)?;
    let text = read_existing(&contained, &target.file, expected)?;
    let mut stream = Stream::split(&target.file, &text)?;
    let index = stream.check_target(target)?;
    stream.documents.remove(index);
    if stream.documents.is_empty() {
        check_writable(&contained.path, &target.file)?;
        return fs::remove_file(&contained.path).map_err(WriteError::Io);
    }
    commit(&contained, &target.file, stream)
}

/// The current digest of a catalog file, or `None` when it does not exist.
pub fn file_digest(root: &CatalogRoot, file: &Path) -> Result<Option<FileDigest>, WriteError> {
    let Some(contained) = root.contained(file)? else {
        return Ok(None);
    };
    match fs::read(&contained.path) {
        Ok(bytes) => Ok(Some(FileDigest::of(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(WriteError::Io(error)),
    }
}

fn read_existing(
    contained: &ContainedPath,
    file: &Path,
    expected: FileDigest,
) -> Result<String, WriteError> {
    let path = &contained.path;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteError::SourceMissing(file.to_owned()));
        }
        Err(error) => return Err(WriteError::Io(error)),
    };
    if FileDigest::of(&bytes) != expected {
        return Err(WriteError::SourceModified(file.to_owned()));
    }
    check_writable(path, file)?;
    String::from_utf8(bytes).map_err(|_| WriteError::UnreadableStream {
        file: file.to_owned(),
        message: "file is not valid UTF-8".to_owned(),
    })
}

fn check_writable(path: &Path, file: &Path) -> Result<(), WriteError> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(WriteError::ReadOnly(file.to_owned()));
    }
    Ok(())
}

fn canonical_document(template: &Template) -> String {
    String::from_utf8(template.canonical_bytes()).expect("canonical YAML is UTF-8")
}

/// One catalog file, split into a verbatim preamble and its document texts.
struct Stream {
    /// Leading comments and blank lines before the first document, kept
    /// exactly so a file header survives an edit.
    preamble: String,
    /// Each document's text, either verbatim from the file or freshly
    /// canonicalised for the one being written.
    documents: Vec<String>,
}

impl Stream {
    /// Split `text` at its `---` document markers.
    ///
    /// The split is verified against the YAML parser's own document count, so
    /// a file whose layout this simple scan would mis-segment (a `---` inside
    /// a block scalar, say) is re-emitted canonically instead of spliced.
    fn split(file: &Path, text: &str) -> Result<Self, WriteError> {
        let parsed = parse_documents(file, text)?;
        let Some(split) = Self::split_textually(text) else {
            return Ok(Self::canonicalised(&parsed));
        };
        if split.documents.len() != parsed.len() {
            return Ok(Self::canonicalised(&parsed));
        }
        Ok(split)
    }

    fn canonicalised(parsed: &[serde_yaml::Value]) -> Self {
        Self {
            preamble: String::new(),
            documents: parsed
                .iter()
                .map(|value| {
                    serde_yaml::to_string(value).expect("a parsed YAML value re-serialises")
                })
                .collect(),
        }
    }

    /// Segment `text` on lines that are exactly `---`. Returns `None` when a
    /// marker line carries trailing content, which this scan cannot place.
    fn split_textually(text: &str) -> Option<Self> {
        let mut markers = Vec::new();
        let mut offset = 0usize;
        for line in text.split_inclusive('\n') {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.starts_with("---") {
                if trimmed.trim_end() != "---" {
                    return None;
                }
                markers.push((offset, offset + line.len()));
            }
            offset += line.len();
        }

        if markers.is_empty() {
            return Some(Self {
                preamble: String::new(),
                documents: vec![text.to_owned()],
            });
        }

        let leading = &text[..markers[0].0];
        let mut documents: Vec<String> = Vec::new();
        let preamble = if has_content(leading) {
            documents.push(leading.to_owned());
            String::new()
        } else {
            leading.to_owned()
        };
        for (index, (_, body_start)) in markers.iter().enumerate() {
            let end = markers
                .get(index + 1)
                .map_or(text.len(), |(next_start, _)| *next_start);
            documents.push(text[*body_start..end].to_owned());
        }
        Some(Self {
            preamble,
            documents,
        })
    }

    /// Check that `target` names a document that is present and carries the
    /// identity the caller read, returning its index.
    fn check_target(&self, target: &WriteTarget) -> Result<usize, WriteError> {
        let mismatch = || WriteError::TargetMismatch {
            file: target.file.clone(),
            document: target.document,
            identity: target.identity.clone(),
        };
        let index = target.document.index();
        let document = self.documents.get(index).ok_or_else(mismatch)?;
        let value: serde_yaml::Value = serde_yaml::from_str(document).map_err(|_| mismatch())?;
        if identity_of(&value).as_ref() != Some(&target.identity) {
            return Err(mismatch());
        }
        Ok(index)
    }

    fn render(&self) -> String {
        let mut out = self.preamble.clone();
        for document in &self.documents {
            out.push_str("---\n");
            out.push_str(document);
            if !document.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }
}

/// Render, verify, and atomically replace.
///
/// Verification is what makes the textual splice safe: the rendered text must
/// parse back to exactly the documents it was built from, or nothing is
/// written at all.
fn commit(contained: &ContainedPath, file: &Path, stream: Stream) -> Result<(), WriteError> {
    let rendered = stream.render();
    let parsed = parse_documents(file, &rendered)
        .map_err(|_| WriteError::WouldNotRoundTrip(file.to_owned()))?;
    if parsed.len() != stream.documents.len() {
        return Err(WriteError::WouldNotRoundTrip(file.to_owned()));
    }
    replace_atomically(contained, &rendered)
}

fn parse_documents(file: &Path, text: &str) -> Result<Vec<serde_yaml::Value>, WriteError> {
    serde_yaml::Deserializer::from_str(text)
        .map(serde::Deserialize::deserialize)
        .collect::<Result<Vec<serde_yaml::Value>, _>>()
        .map_err(|error| WriteError::UnreadableStream {
            file: file.to_owned(),
            message: error.to_string(),
        })
}

fn identity_of(value: &serde_yaml::Value) -> Option<TemplateIdentity> {
    let metadata = value.get("metadata")?;
    Some(TemplateIdentity::new(
        metadata.get("catalog")?.as_str()?.try_into().ok()?,
        metadata.get("name")?.as_str()?.try_into().ok()?,
    ))
}

fn has_content(text: &str) -> bool {
    text.lines()
        .any(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
}

/// How many temporary names are tried before giving up on finding a free one.
const TEMPORARY_ATTEMPTS: u32 = 32;

/// Write `contents` to a sibling temporary file and rename it over the target,
/// so the previous file survives any failure before the rename.
fn replace_atomically(contained: &ContainedPath, contents: &str) -> Result<(), WriteError> {
    let stem = contained
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let (temporary, mut file) =
        create_temporary(&contained.directory, &stem).map_err(WriteError::Io)?;

    let mut write = || -> std::io::Result<()> {
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    };
    if let Err(error) = write() {
        let _ = fs::remove_file(&temporary);
        return Err(WriteError::Io(error));
    }
    if let Err(error) = fs::rename(&temporary, &contained.path) {
        let _ = fs::remove_file(&temporary);
        return Err(WriteError::Io(error));
    }
    Ok(())
}

/// Create a fresh temporary file beside the target, without following or
/// overwriting anything already at the chosen name.
///
/// `create_new` is the point: it fails rather than opening an existing file or
/// following a symbolic link planted at a predictable name, so the worst a
/// squatter can do is cost an attempt. The name is unpredictable rather than
/// merely unique so that a squatter cannot plant one in the first place, and
/// it is dot-prefixed with a `.tmp` extension so [`is_catalog_file`] keeps a
/// concurrent load from ever seeing it.
fn create_temporary(directory: &Path, stem: &str) -> std::io::Result<(PathBuf, fs::File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    for _ in 0..TEMPORARY_ATTEMPTS {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let candidate = directory.join(format!(
            ".{stem}.{}.{}.{nanos}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free temporary file name beside the catalog file",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::TemplateDocument;
    use crate::limits::Limits;

    const HEADER: &str = "# Reference catalog of Solar System bodies.\n";

    fn template_text(name: &str, mass: &str) -> String {
        format!(
            "apiVersion: kagami.catalog/v1\nkind: ObjectTemplate\nmetadata: {{catalog: planets, \
             name: {name}}}\nspec:\n  components:\n  - type: {{plugin: kagami.mass_sources, name: \
             inertial_mass}}\n    properties:\n      mass: {{quantity: \"{mass}\"}}\n"
        )
    }

    fn template(name: &str, mass: &str) -> Template {
        let document: TemplateDocument = serde_yaml::from_str(&template_text(name, mass)).unwrap();
        Template::from_document(&document, &Limits::DEFAULT).unwrap()
    }

    fn identity(name: &str) -> TemplateIdentity {
        TemplateIdentity::new("planets".try_into().unwrap(), name.try_into().unwrap())
    }

    fn target(name: &str, index: usize) -> WriteTarget {
        WriteTarget {
            file: PathBuf::from("planets.yaml"),
            document: DocumentOrdinal::from_index(index),
            identity: identity(name),
        }
    }

    /// A temporary catalog directory and the root that owns it. The
    /// [`tempfile::TempDir`] is returned so the caller keeps it alive.
    fn workspace() -> (tempfile::TempDir, CatalogRoot) {
        let directory = tempfile::tempdir().unwrap();
        let root = CatalogRoot::new(directory.path());
        (directory, root)
    }

    fn seed(root: &CatalogRoot, contents: &str) -> FileDigest {
        fs::write(root.path().join("planets.yaml"), contents).unwrap();
        file_digest(root, Path::new("planets.yaml"))
            .unwrap()
            .unwrap()
    }

    fn read(root: &CatalogRoot) -> String {
        fs::read_to_string(root.path().join("planets.yaml")).unwrap()
    }

    #[test]
    fn creating_a_new_file_writes_one_document() {
        let (_directory, root) = workspace();
        create_entry(
            &root,
            Path::new("planets.yaml"),
            &template("sun", "1.989e30"),
            None,
        )
        .unwrap();
        let text = read(&root);
        assert!(text.starts_with("---\n"), "{text}");
        assert!(text.contains("name: sun"), "{text}");
    }

    #[test]
    fn creating_over_an_existing_file_is_refused() {
        let (_directory, root) = workspace();
        seed(&root, &template_text("sun", "1.989e30"));
        let error = create_entry(
            &root,
            Path::new("planets.yaml"),
            &template("earth", "5.97e24"),
            None,
        )
        .unwrap_err();
        assert!(matches!(error, WriteError::SourceAlreadyExists(_)));
    }

    #[test]
    fn creating_with_a_digest_appends_to_the_existing_stream() {
        let (_directory, root) = workspace();
        let digest = seed(&root, &template_text("sun", "1.989e30"));
        create_entry(
            &root,
            Path::new("planets.yaml"),
            &template("earth", "5.97e24"),
            Some(digest),
        )
        .unwrap();
        let text = read(&root);
        assert!(text.contains("name: sun"));
        assert!(text.contains("name: earth"));
    }

    #[test]
    fn updating_one_document_preserves_its_siblings_verbatim() {
        let (_directory, root) = workspace();
        let contents = format!(
            "{HEADER}---\n{}---\n# earth is hand-annotated\n{}",
            template_text("sun", "1.989e30"),
            template_text("earth", "5.97e24")
        );
        let digest = seed(&root, &contents);
        update_entry(
            &root,
            &target("sun", 0),
            &template("sun", "1.9885e30"),
            digest,
        )
        .unwrap();
        let text = read(&root);
        assert!(text.starts_with(HEADER), "the file header survives: {text}");
        assert!(
            text.contains("# earth is hand-annotated"),
            "a sibling document keeps its comments: {text}"
        );
        assert!(text.contains("1.9885e30"));
        assert!(!text.contains("1.989e30\n"));
    }

    #[test]
    fn a_write_based_on_a_stale_digest_is_refused_and_changes_nothing() {
        let (directory, root) = workspace();
        let stale = seed(&root, &template_text("sun", "1.989e30"));
        let edited = format!("{}\n# edited elsewhere\n", template_text("sun", "1.0"));
        fs::write(directory.path().join("planets.yaml"), &edited).unwrap();

        let error =
            update_entry(&root, &target("sun", 0), &template("sun", "2.0"), stale).unwrap_err();
        assert!(matches!(error, WriteError::SourceModified(_)));
        assert_eq!(read(&root), edited);
    }

    #[test]
    fn a_target_whose_identity_moved_is_refused() {
        let (_directory, root) = workspace();
        let digest = seed(
            &root,
            &format!(
                "---\n{}---\n{}",
                template_text("sun", "1.989e30"),
                template_text("earth", "5.97e24")
            ),
        );
        let error = update_entry(
            &root,
            // `earth` is document 2, not document 1.
            &target("earth", 0),
            &template("earth", "1.0"),
            digest,
        )
        .unwrap_err();
        assert!(matches!(error, WriteError::TargetMismatch { .. }));
    }

    #[test]
    fn deleting_one_document_leaves_the_others() {
        let (_directory, root) = workspace();
        let digest = seed(
            &root,
            &format!(
                "---\n{}---\n{}",
                template_text("sun", "1.989e30"),
                template_text("earth", "5.97e24")
            ),
        );
        delete_entry(&root, &target("sun", 0), digest).unwrap();
        let text = read(&root);
        assert!(!text.contains("name: sun"));
        assert!(text.contains("name: earth"));
    }

    #[test]
    fn deleting_the_last_document_removes_the_file() {
        let (directory, root) = workspace();
        let digest = seed(&root, &template_text("sun", "1.989e30"));
        delete_entry(&root, &target("sun", 0), digest).unwrap();
        assert!(!directory.path().join("planets.yaml").exists());
    }

    #[test]
    fn a_path_that_escapes_the_catalog_root_is_refused() {
        let (_directory, root) = workspace();
        for escape in ["../outside.yaml", "/etc/passwd.yaml", "sub/../../x.yaml"] {
            let error =
                create_entry(&root, Path::new(escape), &template("sun", "1.0"), None).unwrap_err();
            assert!(matches!(error, WriteError::UnsafePath(_)), "{escape}");
        }
    }

    #[test]
    fn a_non_catalog_extension_is_refused() {
        let (_directory, root) = workspace();
        let error = create_entry(
            &root,
            Path::new("planets.txt"),
            &template("sun", "1.0"),
            None,
        )
        .unwrap_err();
        assert!(matches!(error, WriteError::UnsafePath(_)));
    }

    #[test]
    fn a_read_only_file_is_refused_before_anything_is_written() {
        let (directory, root) = workspace();
        let digest = seed(&root, &template_text("sun", "1.989e30"));
        let path = directory.path().join("planets.yaml");
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();

        let error =
            update_entry(&root, &target("sun", 0), &template("sun", "2.0"), digest).unwrap_err();
        assert!(matches!(error, WriteError::ReadOnly(_)));
    }

    #[test]
    fn a_written_file_reloads_to_the_template_that_was_written() {
        let (_directory, root) = workspace();
        let written = template("sun", "1.989e30");
        create_entry(&root, Path::new("planets.yaml"), &written, None).unwrap();
        let stream =
            crate::load::parse_stream(Path::new("planets.yaml"), &read(&root), &Limits::DEFAULT);
        assert_eq!(stream.documents[0].outcome.as_ref().unwrap(), &written);
    }

    #[test]
    fn a_file_whose_layout_cannot_be_spliced_is_re_emitted_canonically() {
        let (_directory, root) = workspace();
        // A `---` line inside a block scalar belongs to the document, not to
        // the stream. The textual scan would see two segments where YAML sees
        // one, so the mismatch makes the writer re-emit canonically rather
        // than guess at boundaries.
        let contents = r#"---
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: planets
  name: sun
  description: |
    first line
    ---
    second line
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "1.989e30"}
"#;
        let digest = seed(&root, contents);
        update_entry(&root, &target("sun", 0), &template("sun", "2.0"), digest).unwrap();
        let text = read(&root);
        assert!(text.contains("'2.0'") || text.contains("\"2.0\""), "{text}");
    }

    #[test]
    fn creating_the_first_template_creates_the_catalog_directory_it_needs() {
        // An installation with no catalog is a normal state, so containment
        // has to establish the root rather than require it — and every level
        // below it, one proven component at a time.
        let parent = tempfile::tempdir().unwrap();
        let root = CatalogRoot::new(parent.path().join("catalogs"));
        create_entry(
            &root,
            Path::new("nested/deeper/planets.yaml"),
            &template("sun", "1.989e30"),
            None,
        )
        .unwrap();
        let written = fs::read_to_string(root.path().join("nested/deeper/planets.yaml")).unwrap();
        assert!(written.contains("name: sun"), "{written}");
    }

    #[test]
    fn undoing_a_resolution_removes_only_the_empty_directories_it_created() {
        let (directory, _root) = workspace();
        let one = directory.path().join("one");
        let two = one.join("two");
        fs::create_dir_all(&two).unwrap();

        // A directory something else has since put a file in stays, and so
        // does its parent, because removal stops at the first refusal.
        fs::write(two.join("planets.yaml"), "kept").unwrap();
        undo_created_directories(&[one.clone(), two.clone()]);
        assert!(two.is_dir(), "a non-empty directory is never removed");
        assert!(one.is_dir());

        fs::remove_file(two.join("planets.yaml")).unwrap();
        undo_created_directories(&[one.clone(), two.clone()]);
        assert!(!two.exists(), "an empty directory it created is removed");
        assert!(!one.exists(), "and so is its parent, deepest first");
    }

    #[test]
    fn a_digest_of_a_file_under_a_missing_root_is_absent_rather_than_an_error() {
        let parent = tempfile::tempdir().unwrap();
        let root = CatalogRoot::new(parent.path().join("catalogs"));
        assert_eq!(file_digest(&root, Path::new("planets.yaml")).unwrap(), None);
        // And the query did not bring the directory into existence.
        assert!(!root.path().exists());
    }

    #[test]
    fn two_temporary_files_beside_the_same_target_never_collide() {
        let (directory, _root) = workspace();
        let (first, _) = create_temporary(directory.path(), "planets.yaml").unwrap();
        let (second, _) = create_temporary(directory.path(), "planets.yaml").unwrap();
        assert_ne!(first, second);
        assert!(first.is_file() && second.is_file());
        // A concurrent load must never mistake one for catalog data.
        assert!(!is_catalog_file(&first), "{}", first.display());
    }

    /// Containment is a claim about the filesystem, not about path text, so
    /// the tests that prove it need real symbolic links.
    #[cfg(unix)]
    mod containment {
        use super::*;
        use std::os::unix::fs::symlink;

        const SENTINEL: &str = "# a file the catalog does not own\n";

        /// An outside directory holding `secret.yaml`, which no catalog
        /// operation may read or write.
        fn outside() -> (tempfile::TempDir, PathBuf) {
            let directory = tempfile::tempdir().unwrap();
            let sentinel = directory.path().join("secret.yaml");
            fs::write(&sentinel, SENTINEL).unwrap();
            (directory, sentinel)
        }

        /// Every file effect, refused the same way and for the same reason.
        fn assert_every_effect_is_refused(root: &CatalogRoot, file: &Path) {
            let errors: Vec<WriteError> = vec![
                create_entry(root, file, &template("sun", "1.0"), None).unwrap_err(),
                create_entry(
                    root,
                    file,
                    &template("sun", "1.0"),
                    Some(FileDigest::of(SENTINEL.as_bytes())),
                )
                .unwrap_err(),
                update_entry(
                    root,
                    &WriteTarget {
                        file: file.to_owned(),
                        document: DocumentOrdinal::from_index(0),
                        identity: identity("sun"),
                    },
                    &template("sun", "1.0"),
                    FileDigest::of(SENTINEL.as_bytes()),
                )
                .unwrap_err(),
                delete_entry(
                    root,
                    &WriteTarget {
                        file: file.to_owned(),
                        document: DocumentOrdinal::from_index(0),
                        identity: identity("sun"),
                    },
                    FileDigest::of(SENTINEL.as_bytes()),
                )
                .unwrap_err(),
                file_digest(root, file).unwrap_err(),
            ];
            for error in errors {
                assert!(
                    matches!(error, WriteError::NotContained(_)),
                    "expected containment refusal, got {error:?}"
                );
            }
        }

        #[test]
        fn a_symlinked_parent_directory_cannot_redirect_a_write_outside_the_root() {
            let (_directory, root) = workspace();
            let (_elsewhere, sentinel) = outside();
            symlink(sentinel.parent().unwrap(), root.path().join("sub")).unwrap();

            assert_every_effect_is_refused(&root, Path::new("sub/secret.yaml"));
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), SENTINEL);
        }

        #[test]
        fn a_symlinked_target_file_cannot_redirect_a_write_outside_the_root() {
            let (_directory, root) = workspace();
            let (_elsewhere, sentinel) = outside();
            symlink(&sentinel, root.path().join("planets.yaml")).unwrap();

            assert_every_effect_is_refused(&root, Path::new("planets.yaml"));
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), SENTINEL);
        }

        #[test]
        fn a_missing_descendant_below_a_symlinked_parent_creates_nothing_outside() {
            // The escape the first correction still had: resolving the whole
            // requested parent at once let `create_dir_all` follow `linked`
            // and make a directory outside before containment was judged.
            let (_directory, root) = workspace();
            let (elsewhere, sentinel) = outside();
            symlink(sentinel.parent().unwrap(), root.path().join("linked")).unwrap();

            let error = create_entry(
                &root,
                Path::new("linked/created-outside/planets.yaml"),
                &template("sun", "1.0"),
                None,
            )
            .unwrap_err();

            assert!(matches!(error, WriteError::NotContained(_)), "{error:?}");
            assert!(
                !elsewhere.path().join("created-outside").exists(),
                "a rejected command created a directory outside the root"
            );
            assert!(!elsewhere.path().join("planets.yaml").exists());
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), SENTINEL);
        }

        #[test]
        fn several_missing_levels_below_a_symlinked_parent_create_nothing_outside() {
            let (_directory, root) = workspace();
            let (elsewhere, sentinel) = outside();
            symlink(sentinel.parent().unwrap(), root.path().join("linked")).unwrap();

            let error = create_entry(
                &root,
                Path::new("linked/one/two/three/planets.yaml"),
                &template("sun", "1.0"),
                None,
            )
            .unwrap_err();

            assert!(matches!(error, WriteError::NotContained(_)), "{error:?}");
            assert!(
                !elsewhere.path().join("one").exists(),
                "a rejected command created a directory outside the root"
            );
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), SENTINEL);
        }

        #[test]
        fn a_symlinked_target_inside_the_root_is_refused_too() {
            let (_directory, root) = workspace();
            // Renaming over the link would destroy it rather than edit what
            // it points at, so a link is refused wherever it leads.
            seed(&root, &template_text("sun", "1.989e30"));
            symlink(
                root.path().join("planets.yaml"),
                root.path().join("aliased.yaml"),
            )
            .unwrap();

            let error = file_digest(&root, Path::new("aliased.yaml")).unwrap_err();
            assert!(matches!(error, WriteError::NotContained(_)), "{error:?}");
        }

        #[test]
        fn a_symlinked_catalog_root_is_still_writable() {
            // Containment must refuse an escape, not a deliberately relinked
            // catalog directory.
            let real = tempfile::tempdir().unwrap();
            let parent = tempfile::tempdir().unwrap();
            let link = parent.path().join("catalogs");
            symlink(real.path(), &link).unwrap();
            let root = CatalogRoot::new(&link);

            create_entry(
                &root,
                Path::new("planets.yaml"),
                &template("sun", "1.989e30"),
                None,
            )
            .unwrap();
            assert!(
                fs::read_to_string(real.path().join("planets.yaml"))
                    .unwrap()
                    .contains("name: sun")
            );
        }

        #[test]
        fn a_symlink_planted_at_a_predictable_temporary_path_is_not_followed() {
            let (_directory, root) = workspace();
            let (_elsewhere, sentinel) = outside();
            let digest = seed(&root, &template_text("sun", "1.989e30"));
            // The name the writer used to choose deterministically. Nothing
            // may be written through it, and it must not be deleted either:
            // it is not this process's file to remove.
            let planted = root
                .path()
                .join(format!(".planets.yaml.{}.0.tmp", std::process::id()));
            symlink(&sentinel, &planted).unwrap();

            update_entry(&root, &target("sun", 0), &template("sun", "2.0"), digest).unwrap();

            assert!(read(&root).contains("2.0"), "the update still applied");
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), SENTINEL);
            assert!(fs::symlink_metadata(&planted).unwrap().is_symlink());
        }
    }
}
