//! Reading a catalog directory from disk.
//!
//! This is the imperative shell: it stats, reads, and decodes bytes, and
//! hands [`crate::resolve::resolve`] a list of parsed documents. Every
//! decision about what an entry *means* lives in the pure core, so the only
//! behaviour that needs a filesystem to test is the behaviour that is
//! genuinely about files: size bounds, directory ordering, and per-file
//! failure isolation.
//!
//! Kagami must never fail to start because of catalog state. A missing or
//! unreadable directory loads as an empty catalog; an oversized or non-UTF-8
//! file is reported and skipped; a malformed document is isolated from its
//! siblings.

use std::fs;
use std::path::{Path, PathBuf};

use orishu_resource::ResourceHeader;

use crate::diagnostic::{Diagnostic, InvalidReason};
use crate::document::{self, API_VERSION, KIND, TemplateDocument};
use crate::entry::{CatalogFileError, CatalogSet};
use crate::limits::Limits;
use crate::resolve::{ParsedDocument, resolve};
use crate::schema::SchemaRegistry;
use crate::source::{DocumentOrdinal, SourceLocation, TemplateIdentity};
use crate::template::Template;

/// File extensions the loader treats as catalog files.
pub const CATALOG_EXTENSIONS: [&str; 2] = ["yaml", "yml"];

/// Load every catalog file under `root` and resolve the result against the
/// installed component schemas.
///
/// The scan is flat (no recursion) and ordered by path, so the same directory
/// contents always produce the same report. A directory that does not exist
/// or cannot be read yields an empty set rather than an error: an
/// installation with no catalog is a normal state, not a failure.
pub fn load_directory(root: &Path, registry: &SchemaRegistry, limits: &Limits) -> CatalogSet {
    let (documents, file_errors) = read_directory(root, limits);
    resolve(documents, file_errors, registry, limits)
}

/// Read and parse every catalog file under `root`, without resolving
/// availability. Useful to a caller that wants to resolve the same documents
/// against more than one schema registry.
pub fn read_directory(
    root: &Path,
    limits: &Limits,
) -> (Vec<ParsedDocument>, Vec<CatalogFileError>) {
    let mut documents = Vec::new();
    let mut errors = Vec::new();

    let Ok(read_dir) = fs::read_dir(root) else {
        return (documents, errors);
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut links: Vec<PathBuf> = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !is_catalog_file(&path) {
            continue;
        }
        // `DirEntry::file_type` does not follow the link, which is what makes
        // this a decision about the directory entry rather than about
        // whatever it points at.
        match entry.file_type() {
            Ok(file_type) if file_type.is_file() => paths.push(path),
            Ok(file_type) if file_type.is_symlink() => links.push(path),
            // A directory or another special file named `*.yaml` is not a
            // catalog file and needs no diagnostic.
            _ => {}
        }
    }
    paths.sort();
    links.sort();

    // Reported before the files are read, in path order, so the same directory
    // contents always produce the same report.
    for link in links {
        errors.push(CatalogFileError {
            file: SourceLocation::relative_to(root, &link, DocumentOrdinal::from_index(0)).file,
            reason: InvalidReason::SymbolicLink,
        });
    }

    if paths.len() > limits.max_files {
        errors.push(CatalogFileError {
            // The bound belongs to the directory, not to any one file in it.
            file: PathBuf::new(),
            reason: InvalidReason::LimitExceeded {
                what: "catalog file",
                found: paths.len(),
                limit: limits.max_files,
            },
        });
        paths.truncate(limits.max_files);
    }

    for path in paths {
        read_file(root, &path, limits, &mut documents, &mut errors);
    }
    (documents, errors)
}

/// `true` when `path` has an extension the catalog loader reads.
pub fn is_catalog_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            CATALOG_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

/// Read one file's document stream, appending its documents and any
/// whole-file failure to the given buffers.
pub fn read_file(
    root: &Path,
    path: &Path,
    limits: &Limits,
    documents: &mut Vec<ParsedDocument>,
    errors: &mut Vec<CatalogFileError>,
) {
    let relative = SourceLocation::relative_to(root, path, DocumentOrdinal::from_index(0)).file;
    let bytes = match read_capped(path, limits) {
        Ok(bytes) => bytes,
        Err(reason) => {
            errors.push(CatalogFileError {
                file: relative,
                reason,
            });
            return;
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(_) => {
            errors.push(CatalogFileError {
                file: relative,
                reason: InvalidReason::NotUtf8,
            });
            return;
        }
    };

    let stream = parse_stream(&relative, text, limits);
    if stream.documents.len() >= limits.max_documents_per_file && stream.truncated {
        errors.push(CatalogFileError {
            file: relative,
            reason: InvalidReason::LimitExceeded {
                what: "document",
                found: stream.documents.len() + 1,
                limit: limits.max_documents_per_file,
            },
        });
    }
    documents.extend(stream.documents);
}

/// The documents one file's YAML stream yielded.
#[derive(Debug, Default)]
pub struct DocumentStream {
    /// Every document parsed, in stream order.
    pub documents: Vec<ParsedDocument>,
    /// `true` when reading stopped early: either the per-file document bound
    /// was reached, or a lexical error left the YAML parser unable to
    /// resynchronise to the next `---` boundary.
    pub truncated: bool,
}

/// Parse a `---`-separated YAML stream into documents, isolating each
/// document's failure from its siblings.
///
/// Pure: `file` is only recorded in each document's [`SourceLocation`], never
/// opened. This is the entry point the catalog authority's `Validate` command
/// uses to check authored text that is not on disk yet.
pub fn parse_stream(file: &Path, text: &str, limits: &Limits) -> DocumentStream {
    let mut stream = DocumentStream::default();
    for (index, document) in serde_yaml::Deserializer::from_str(text).enumerate() {
        if index >= limits.max_documents_per_file {
            stream.truncated = true;
            break;
        }
        let source = SourceLocation::new(file, DocumentOrdinal::from_index(index));
        let value: serde_yaml::Value = match serde::Deserialize::deserialize(document) {
            Ok(value) => value,
            Err(error) => {
                // Failing at this first step — decoding straight off the YAML
                // event stream into an untyped value — is a lexical error,
                // not a semantic one. The shared parser is left in an
                // indeterminate state, so stop rather than risk re-reporting
                // the same error forever. Every document already recorded
                // stays valid and isolated.
                stream.documents.push(ParsedDocument::invalid(
                    source,
                    None,
                    vec![Diagnostic::whole_file(InvalidReason::MalformedYaml {
                        message: error.to_string(),
                    })],
                ));
                stream.truncated = true;
                break;
            }
        };
        stream.documents.push(parse_value(source, &value, limits));
    }
    stream
}

/// Decode and structurally validate one already-parsed YAML value.
pub fn parse_value(
    source: SourceLocation,
    value: &serde_yaml::Value,
    limits: &Limits,
) -> ParsedDocument {
    if let Err(diagnostic) = check_envelope(value) {
        return ParsedDocument::invalid(source, None, vec![*diagnostic]);
    }
    let document: TemplateDocument = match serde_path_to_error::deserialize(value) {
        Ok(document) => document,
        Err(error) => {
            let field_path = error.path().to_string();
            let message = error.into_inner().to_string();
            return ParsedDocument::invalid(
                source,
                None,
                vec![Diagnostic::at(
                    field_path,
                    InvalidReason::SchemaMismatch { message },
                )],
            );
        }
    };
    let identity = TemplateIdentity::new(
        document.metadata.catalog.clone(),
        document.metadata.name.clone(),
    );
    match Template::from_document(&document, limits) {
        Ok(template) => ParsedDocument::valid(source, template),
        Err(diagnostics) => ParsedDocument::invalid(source, Some(identity), diagnostics),
    }
}

/// Decode only `{apiVersion, kind}` and reject before the rest of the
/// document is trusted, so a document from another format version is refused
/// for its version rather than for whichever unrecognised field is read
/// first.
///
/// The header type is the shared [`ResourceHeader`], which permits unknown
/// fields for exactly that reason and bounds both discriminators before
/// copying them.
///
/// `Box`ed because `Diagnostic` is far larger than the `()` success case,
/// and this returns `Ok` for every well-formed document.
fn check_envelope(value: &serde_yaml::Value) -> Result<(), Box<Diagnostic>> {
    let header: ResourceHeader = serde_path_to_error::deserialize(value).map_err(|error| {
        Box::new(Diagnostic {
            field_path: Some(error.path().to_string()),
            span: None,
            reason: InvalidReason::SchemaMismatch {
                message: error.into_inner().to_string(),
            },
        })
    })?;
    // Reported per field rather than as one combined mismatch: a catalog
    // browser distinguishes "written for a newer Kagami" from "not a template
    // at all", and they are different things for a user to do something about.
    if header.api_version() != &document::api_version() {
        return Err(Box::new(Diagnostic::at(
            "apiVersion",
            InvalidReason::UnsupportedApiVersion {
                found: header.api_version().to_string(),
                expected: API_VERSION.to_owned(),
            },
        )));
    }
    if header.kind() != &document::kind() {
        return Err(Box::new(Diagnostic::at(
            "kind",
            InvalidReason::UnsupportedKind {
                found: header.kind().to_string(),
                expected: KIND.to_owned(),
            },
        )));
    }
    Ok(())
}

fn read_capped(path: &Path, limits: &Limits) -> Result<Vec<u8>, InvalidReason> {
    let metadata = fs::metadata(path).map_err(|error| InvalidReason::Io {
        message: error.to_string(),
    })?;
    if metadata.len() > limits.max_file_bytes {
        return Err(InvalidReason::FileTooLarge {
            max_bytes: limits.max_file_bytes,
            actual_bytes: metadata.len(),
        });
    }
    let bytes = fs::read(path).map_err(|error| InvalidReason::Io {
        message: error.to_string(),
    })?;
    // Defends the race where the file grew between the stat above and this
    // read, so the bound holds against what was actually read.
    if bytes.len() as u64 > limits.max_file_bytes {
        return Err(InvalidReason::FileTooLarge {
            max_bytes: limits.max_file_bytes,
            actual_bytes: bytes.len() as u64,
        });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use crate::entry::LoadResult;
    use crate::name::{ComponentName, ComponentTypeId, PluginId, PropertyName};
    use crate::quantity::Dimension;
    use crate::schema::{ComponentSchema, PropertyKind, PropertySchema, SchemaVersion};

    const SUN: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: sun}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: {expression: "1.989e30", unit: kg}}
"#;

    const EARTH: &str = r#"apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata: {catalog: planets, name: earth}
spec:
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "5.97e24"}
"#;

    fn registry() -> SchemaRegistry {
        SchemaRegistry::new().with(
            ComponentSchema::new(
                ComponentTypeId::new(
                    PluginId::new("kagami.mass_sources").unwrap(),
                    ComponentName::new("inertial_mass").unwrap(),
                ),
                SchemaVersion(1),
            )
            .with_property(
                PropertyName::new("mass").unwrap(),
                PropertySchema::required(PropertyKind::Quantity {
                    dimension: Dimension::MASS,
                }),
            ),
        )
    }

    fn write(directory: &Path, name: &str, contents: &str) {
        let mut file = fs::File::create(directory.join(name)).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn a_missing_directory_loads_as_an_empty_catalog() {
        let set = load_directory(
            Path::new("/nonexistent/kagami/catalogs"),
            &registry(),
            &Limits::DEFAULT,
        );
        assert!(set.entries().is_empty());
        assert!(set.file_errors().is_empty());
    }

    #[test]
    fn two_valid_entries_load_deterministically() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "planets.yaml",
            &format!("{SUN}---\n{EARTH}"),
        );
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        assert_eq!(set.summary().available, 2);
        assert_eq!(set.entries()[0].source.to_string(), "planets.yaml#1");
        assert_eq!(set.entries()[1].source.to_string(), "planets.yaml#2");
    }

    #[test]
    fn files_are_read_in_path_order_whatever_the_directory_returns() {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "zzz.yaml", EARTH);
        write(directory.path(), "aaa.yaml", SUN);
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        let files: Vec<_> = set
            .entries()
            .iter()
            .map(|entry| entry.source.file.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, vec!["aaa.yaml", "zzz.yaml"]);
    }

    #[test]
    fn a_malformed_document_is_isolated_from_its_valid_siblings() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "planets.yaml",
            &format!("{SUN}---\napiVersion: kagami.catalog/v1\nkind: Nonsense\n---\n{EARTH}"),
        );
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        assert_eq!(set.summary().available, 2);
        assert_eq!(set.summary().invalid, 1);
        let LoadResult::Invalid { diagnostics } = &set.entries()[1].result else {
            panic!("expected the middle document to be invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::UnsupportedKind { .. }
        ));
    }

    #[test]
    fn a_document_from_another_format_version_is_rejected_for_its_version() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "future.yaml",
            "apiVersion: kagami.catalog/v2\nkind: ObjectTemplate\nwhatever: [1, 2]\n",
        );
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(matches!(
            diagnostics[0].reason,
            InvalidReason::UnsupportedApiVersion { .. }
        ));
    }

    #[test]
    fn a_field_typo_is_reported_at_its_document_path() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "typo.yaml",
            &SUN.replace("properties:", "propertiez:"),
        );
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        let LoadResult::Invalid { diagnostics } = &set.entries()[0].result else {
            panic!("expected invalid");
        };
        assert!(
            diagnostics[0]
                .field_path
                .as_deref()
                .is_some_and(|path| path.starts_with("spec.components")),
            "{:?}",
            diagnostics[0]
        );
    }

    #[test]
    fn an_oversized_file_is_refused_and_never_blocks_its_neighbours() {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "big.yaml", SUN);
        write(directory.path(), "small.yaml", EARTH);
        let limits = Limits {
            max_file_bytes: SUN.len() as u64 - 1,
            ..Limits::DEFAULT
        };
        let set = load_directory(directory.path(), &registry(), &limits);
        assert_eq!(set.file_errors().len(), 1);
        assert_eq!(set.file_errors()[0].file, Path::new("big.yaml"));
        assert!(matches!(
            set.file_errors()[0].reason,
            InvalidReason::FileTooLarge { .. }
        ));
        assert_eq!(set.summary().available, 1);
    }

    #[test]
    fn a_non_utf8_file_is_reported_rather_than_panicking() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("bad.yaml"), [0xff, 0xfe, 0x00]).unwrap();
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        assert!(matches!(
            set.file_errors()[0].reason,
            InvalidReason::NotUtf8
        ));
    }

    #[test]
    fn non_catalog_extensions_are_ignored() {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "planets.yaml", SUN);
        write(directory.path(), "notes.txt", "not a catalog");
        write(directory.path(), "README.md", "# notes");
        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        assert_eq!(set.entries().len(), 1);
        assert!(set.file_errors().is_empty());
    }

    #[test]
    fn the_file_count_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "a.yaml", SUN);
        write(directory.path(), "b.yaml", EARTH);
        let limits = Limits {
            max_files: 1,
            ..Limits::DEFAULT
        };
        let set = load_directory(directory.path(), &registry(), &limits);
        assert_eq!(set.entries().len(), 1);
        assert!(matches!(
            set.file_errors()[0].reason,
            InvalidReason::LimitExceeded {
                what: "catalog file",
                ..
            }
        ));
    }

    #[test]
    fn the_document_count_per_file_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "planets.yaml",
            &format!("{SUN}---\n{EARTH}"),
        );
        let limits = Limits {
            max_documents_per_file: 1,
            ..Limits::DEFAULT
        };
        let set = load_directory(directory.path(), &registry(), &limits);
        assert_eq!(set.entries().len(), 1);
        assert!(matches!(
            set.file_errors()[0].reason,
            InvalidReason::LimitExceeded {
                what: "document",
                ..
            }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_entry_is_diagnosed_rather_than_loaded_as_catalog_data() {
        let elsewhere = tempfile::tempdir().unwrap();
        write(elsewhere.path(), "private.yaml", EARTH);
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "planets.yaml", SUN);
        std::os::unix::fs::symlink(
            elsewhere.path().join("private.yaml"),
            directory.path().join("linked.yaml"),
        )
        .unwrap();

        let set = load_directory(directory.path(), &registry(), &Limits::DEFAULT);
        // The bytes behind the link are reported, never adopted.
        assert_eq!(set.entries().len(), 1);
        assert_eq!(set.entries()[0].source.file, Path::new("planets.yaml"));
        assert_eq!(set.file_errors().len(), 1);
        assert_eq!(set.file_errors()[0].file, Path::new("linked.yaml"));
        assert!(matches!(
            set.file_errors()[0].reason,
            InvalidReason::SymbolicLink
        ));
    }

    #[test]
    fn parse_stream_needs_no_filesystem() {
        let stream = parse_stream(Path::new("in-memory.yaml"), SUN, &Limits::DEFAULT);
        assert_eq!(stream.documents.len(), 1);
        assert!(stream.documents[0].outcome.is_ok());
        assert!(!stream.truncated);
    }

    #[test]
    fn a_lexical_yaml_error_stops_the_stream_instead_of_looping() {
        let stream = parse_stream(
            Path::new("broken.yaml"),
            &format!("{SUN}---\n\tthis is a tab, which YAML forbids\n---\n{EARTH}"),
            &Limits::DEFAULT,
        );
        assert!(stream.truncated);
        assert!(stream.documents[0].outcome.is_ok());
        assert!(stream.documents.last().unwrap().outcome.is_err());
    }
}
