//! Loaded catalog entries and the set they form.
//!
//! Failure is isolated to the smallest scope that can hold it: a malformed
//! document never hides its siblings in the same file, and an unreadable file
//! never blocks the rest of the directory. Every entry is preserved whatever
//! its state, so a user can inspect, repair, copy, or delete a broken file and
//! so installing a missing plugin later can make an entry available without
//! the file being touched.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::binding::CatalogProjection;
use crate::diagnostic::{Diagnostic, InvalidReason, UnavailableReason};
use crate::name::CatalogName;
use crate::source::{ContentFingerprint, SourceLocation, TemplateIdentity, TemplateProvenance};
use crate::template::Template;

/// What loading one document produced.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadResult {
    /// Structurally valid, every component schema installed, and every
    /// referenced binding visible and resolvable.
    Available {
        /// The validated template.
        template: Template,
    },
    /// Structurally valid, but not usable in this installation.
    Unavailable {
        /// The validated template, preserved for inspection and editing.
        template: Template,
        /// Why it cannot be used here. Never empty.
        reasons: Vec<UnavailableReason>,
    },
    /// A defect in the file itself.
    Invalid {
        /// What is wrong and where. Never empty.
        diagnostics: Vec<Diagnostic>,
    },
}

impl LoadResult {
    /// The validated template, when the document was structurally valid.
    pub fn template(&self) -> Option<&Template> {
        match self {
            LoadResult::Available { template } | LoadResult::Unavailable { template, .. } => {
                Some(template)
            }
            LoadResult::Invalid { .. } => None,
        }
    }

    /// `true` when the entry can be instantiated.
    pub fn is_available(&self) -> bool {
        matches!(self, LoadResult::Available { .. })
    }

    /// A short state name for a listing or an MCP response.
    pub fn state(&self) -> &'static str {
        match self {
            LoadResult::Available { .. } => "available",
            LoadResult::Unavailable { .. } => "unavailable",
            LoadResult::Invalid { .. } => "invalid",
        }
    }
}

/// One catalog entry as loaded: where it came from, what it is called, what
/// its content hashes to, and what state it is in.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEntry {
    /// The catalog-root-relative file and document it was read from.
    pub source: SourceLocation,
    /// Its identity, known as soon as the metadata names validate — even for
    /// an entry whose spec later turns out invalid, so a browser can show
    /// "planets/sun: invalid" rather than an anonymous error.
    pub identity: Option<TemplateIdentity>,
    /// The canonical content fingerprint, present whenever the document was
    /// structurally valid enough to re-encode.
    pub fingerprint: Option<ContentFingerprint>,
    /// The load state.
    pub result: LoadResult,
}

impl CatalogEntry {
    /// The provenance an instantiation of this entry would record.
    ///
    /// `None` for an entry with no identity or no canonical content; those
    /// are never instantiable in the first place.
    pub fn provenance(&self) -> Option<TemplateProvenance> {
        Some(TemplateProvenance {
            api_version: crate::document::API_VERSION.to_owned(),
            identity: self.identity.clone()?,
            source: self.source.clone(),
            fingerprint: self.fingerprint?,
        })
    }
}

/// A failure that never got as far as identifying any document inside a file:
/// unreadable, non-UTF-8, or over the size bound. No document ordinal
/// applies, because the file was never parsed far enough to count documents.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogFileError {
    /// The catalog-root-relative file.
    pub file: PathBuf,
    /// Why it could not be read.
    pub reason: InvalidReason,
}

/// Everything one catalog directory currently publishes: its entries, the
/// files that could not be read at all, and the variable projection its
/// available and unavailable templates form.
///
/// This is a snapshot. Instantiation and workload compilation resolve against
/// one immutable `CatalogSet`, so a concurrent reload cannot change a
/// decision half-way through.
#[derive(Debug, Default)]
pub struct CatalogSet {
    entries: Vec<CatalogEntry>,
    file_errors: Vec<CatalogFileError>,
    projection: CatalogProjection,
    by_identity: BTreeMap<TemplateIdentity, usize>,
}

impl CatalogSet {
    /// Build a set from already-resolved parts. Callers normally go through
    /// [`crate::resolve::resolve()`] or [`crate::load::load_directory()`].
    pub fn new(
        entries: Vec<CatalogEntry>,
        file_errors: Vec<CatalogFileError>,
        projection: CatalogProjection,
    ) -> Self {
        let by_identity = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((entry.identity.clone()?, index)))
            // A later duplicate is recorded as `Invalid` and must not
            // displace the entry that legitimately owns the identity.
            .fold(BTreeMap::new(), |mut map, (identity, index)| {
                map.entry(identity).or_insert(index);
                map
            });
        Self {
            entries,
            file_errors,
            projection,
            by_identity,
        }
    }

    /// Every entry, in deterministic load order.
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Files that could not be read at all.
    pub fn file_errors(&self) -> &[CatalogFileError] {
        &self.file_errors
    }

    /// The variable projection these entries publish.
    pub fn projection(&self) -> &CatalogProjection {
        &self.projection
    }

    /// The entry that owns `identity`.
    pub fn get(&self, identity: &TemplateIdentity) -> Option<&CatalogEntry> {
        self.by_identity
            .get(identity)
            .map(|index| &self.entries[*index])
    }

    /// Every entry that can be instantiated.
    pub fn available(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.result.is_available())
    }

    /// Every catalog name with at least one entry, sorted and deduplicated.
    pub fn catalogs(&self) -> impl Iterator<Item = &CatalogName> {
        self.by_identity.keys().map(|identity| &identity.catalog)
    }

    /// Counts of entries by state, for a status line.
    pub fn summary(&self) -> CatalogSummary {
        self.entries
            .iter()
            .fold(CatalogSummary::default(), |mut summary, entry| {
                match entry.result {
                    LoadResult::Available { .. } => summary.available += 1,
                    LoadResult::Unavailable { .. } => summary.unavailable += 1,
                    LoadResult::Invalid { .. } => summary.invalid += 1,
                }
                summary
            })
    }
}

/// How many entries are in each load state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CatalogSummary {
    /// Entries that can be instantiated.
    pub available: usize,
    /// Entries preserved but not usable in this installation.
    pub unavailable: usize,
    /// Entries with a defect in the file.
    pub invalid: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::DocumentOrdinal;

    fn identity(catalog: &str, name: &str) -> TemplateIdentity {
        TemplateIdentity::new(catalog.try_into().unwrap(), name.try_into().unwrap())
    }

    fn invalid_entry(catalog: &str, name: &str, ordinal: usize) -> CatalogEntry {
        CatalogEntry {
            source: SourceLocation::new("a.yaml", DocumentOrdinal::from_index(ordinal)),
            identity: Some(identity(catalog, name)),
            fingerprint: None,
            result: LoadResult::Invalid {
                diagnostics: vec![Diagnostic::whole_file(InvalidReason::NotUtf8)],
            },
        }
    }

    #[test]
    fn an_invalid_entry_still_reports_its_identity_and_state() {
        let entry = invalid_entry("planets", "sun", 0);
        assert_eq!(entry.identity, Some(identity("planets", "sun")));
        assert_eq!(entry.result.state(), "invalid");
        assert!(entry.result.template().is_none());
        assert!(entry.provenance().is_none());
    }

    #[test]
    fn lookup_prefers_the_first_claimant_of_an_identity() {
        let first = invalid_entry("planets", "sun", 0);
        let second = invalid_entry("planets", "sun", 1);
        let set = CatalogSet::new(vec![first.clone(), second], Vec::new(), Default::default());
        assert_eq!(set.get(&identity("planets", "sun")), Some(&first));
    }

    #[test]
    fn summary_counts_every_state() {
        let set = CatalogSet::new(
            vec![invalid_entry("planets", "sun", 0)],
            Vec::new(),
            Default::default(),
        );
        assert_eq!(
            set.summary(),
            CatalogSummary {
                available: 0,
                unavailable: 0,
                invalid: 1,
            }
        );
    }

    #[test]
    fn catalogs_lists_each_catalog_that_has_an_entry() {
        let set = CatalogSet::new(
            vec![
                invalid_entry("planets", "sun", 0),
                invalid_entry("planets", "earth", 1),
                invalid_entry("particles", "electron", 2),
            ],
            Vec::new(),
            Default::default(),
        );
        let names: Vec<&str> = set.catalogs().map(CatalogName::as_str).collect();
        assert_eq!(names, vec!["particles", "planets", "planets"]);
    }
}
