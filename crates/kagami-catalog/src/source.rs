//! Where a catalog entry came from, and what its content hashes to.
//!
//! Provenance is deliberately *source-qualified but path-portable*: a
//! [`SourceLocation`] holds the path relative to the catalog root and the
//! document's position within that file's YAML stream, never an absolute
//! machine-local path. A [`ContentFingerprint`] covers the entry's canonical
//! content alone, so moving a file between installations does not change the
//! fingerprint an instantiated object recorded.

use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::name::{CatalogName, TemplateName};

/// A document's 1-based position within a `---`-separated YAML stream.
///
/// 1-based because it is shown to a user ("document 2 of planets.yaml"),
/// not used as a raw index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentOrdinal(NonZeroUsize);

impl DocumentOrdinal {
    /// Build an ordinal from a document's 0-based position in the stream.
    pub fn from_index(index: usize) -> Self {
        Self(NonZeroUsize::MIN.saturating_add(index))
    }

    /// The 1-based ordinal.
    pub fn get(self) -> usize {
        self.0.get()
    }

    /// The 0-based index into the stream.
    pub fn index(self) -> usize {
        self.0.get() - 1
    }
}

impl fmt::Display for DocumentOrdinal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The catalog-root-relative file and document a catalog entry was read from.
///
/// One catalog identity does not imply one file: several files may publish
/// into the same catalog, and one file may publish several templates.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Path relative to the catalog root directory.
    pub file: PathBuf,
    /// Which document within that file's stream.
    pub document: DocumentOrdinal,
}

impl SourceLocation {
    /// Build a location from an already-relative path.
    pub fn new(file: impl Into<PathBuf>, document: DocumentOrdinal) -> Self {
        Self {
            file: file.into(),
            document,
        }
    }

    /// Build a location by making `file` relative to `root`.
    ///
    /// A path outside `root` is not expected from the directory loader; its
    /// file name is used rather than serialising an absolute machine-local
    /// path into provenance.
    pub fn relative_to(root: &Path, file: &Path, document: DocumentOrdinal) -> Self {
        let relative = file
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| {
                file.file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| file.to_path_buf())
            });
        Self::new(relative, document)
    }

    /// Resolve this location back to an absolute path under `root`.
    pub fn resolve(&self, root: &Path) -> PathBuf {
        root.join(&self.file)
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}#{}", self.file.display(), self.document)
    }
}

/// The catalog/template name pair that names a template.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TemplateIdentity {
    /// The catalog the template belongs to.
    pub catalog: CatalogName,
    /// The template's name within that catalog.
    pub template: TemplateName,
}

impl TemplateIdentity {
    /// Build a template identity from its parts.
    pub fn new(catalog: CatalogName, template: TemplateName) -> Self {
        Self { catalog, template }
    }
}

impl fmt::Display for TemplateIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.catalog, self.template)
    }
}

/// SHA-256 over an entry's canonical serialised content.
///
/// Computed from the canonical document bytes the writer would emit, so it
/// is stable across whitespace, key order, and file moves, and identical for
/// two byte-different files that mean the same thing. It deliberately covers
/// no path, so an object's recorded fingerprint stays meaningful on a machine
/// whose catalog is laid out differently.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentFingerprint([u8; 32]);

impl ContentFingerprint {
    /// Fingerprint the canonical content bytes of one entry.
    pub fn of(canonical_bytes: &[u8]) -> Self {
        Self(Sha256::digest(canonical_bytes).into())
    }

    /// The raw digest.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContentFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ContentFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContentFingerprint({self})")
    }
}

/// Everything an instantiated object records about where it came from.
///
/// This is historical evidence, not a live pointer: ADR 0008 forbids a
/// tracking link, so nothing in the product resolves a
/// [`TemplateProvenance`] back to a catalog entry in order to update an
/// object. Renaming, editing, or deleting the source template leaves an
/// existing object untouched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateProvenance {
    /// The document format version the template was authored in.
    pub api_version: String,
    /// The template identity at the time of instantiation.
    pub identity: TemplateIdentity,
    /// Where the template was read from, relative to that catalog root.
    pub source: SourceLocation,
    /// The template content used, as a canonical fingerprint.
    pub fingerprint: ContentFingerprint,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinal(index: usize) -> DocumentOrdinal {
        DocumentOrdinal::from_index(index)
    }

    #[test]
    fn document_ordinal_is_one_based_for_display_and_zero_based_for_indexing() {
        assert_eq!(ordinal(0).get(), 1);
        assert_eq!(ordinal(0).index(), 0);
        assert_eq!(ordinal(4).to_string(), "5");
    }

    #[test]
    fn source_location_is_relative_to_the_catalog_root() {
        let location = SourceLocation::relative_to(
            Path::new("/home/user/.config/kagami/catalogs"),
            Path::new("/home/user/.config/kagami/catalogs/planets.yaml"),
            ordinal(1),
        );
        assert_eq!(location.file, Path::new("planets.yaml"));
        assert_eq!(location.to_string(), "planets.yaml#2");
    }

    #[test]
    fn source_location_outside_the_root_keeps_only_the_file_name() {
        let location = SourceLocation::relative_to(
            Path::new("/catalogs"),
            Path::new("/elsewhere/private/planets.yaml"),
            ordinal(0),
        );
        assert_eq!(location.file, Path::new("planets.yaml"));
        assert!(!location.file.is_absolute());
    }

    #[test]
    fn source_location_round_trips_through_the_catalog_root() {
        let location = SourceLocation::new("nested/planets.yaml", ordinal(0));
        assert_eq!(
            location.resolve(Path::new("/catalogs")),
            Path::new("/catalogs/nested/planets.yaml")
        );
    }

    #[test]
    fn fingerprint_depends_only_on_content() {
        let one = ContentFingerprint::of(b"name: earth");
        let same = ContentFingerprint::of(b"name: earth");
        let other = ContentFingerprint::of(b"name: mars");
        assert_eq!(one, same);
        assert_ne!(one, other);
        assert_eq!(one.to_string().len(), 64);
    }

    #[test]
    fn template_identity_displays_catalog_qualified() {
        let identity = TemplateIdentity::new(
            CatalogName::new("planets").unwrap(),
            TemplateName::new("earth").unwrap(),
        );
        assert_eq!(identity.to_string(), "planets/earth");
    }
}
