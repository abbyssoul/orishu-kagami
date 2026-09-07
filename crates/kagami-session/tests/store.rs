//! The durable write protocol, exercised at every step that can fail.
//!
//! The property being asserted is a survival one: whatever goes wrong, and
//! wherever it goes wrong, the *previous* document is still there. That cannot
//! be shown by saving successfully, so this file fails each step in turn
//! through an injectable store and checks what is left on disk afterwards.

mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use kagami_document::{Experiment, Limits};
use kagami_session::{
    DocumentMetadata, DocumentTarget, ExperimentDocument, FileStore, LoadedFrom, load, save,
};
use support::schemas;

/// An in-memory filesystem that can be told to fail one named step.
#[derive(Default)]
struct FaultyStore {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    /// Which operation to fail, and on which path suffix.
    fail: Option<Fault>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    WriteNew,
    SyncFile,
    BackupRename,
    PrimaryRename,
    SyncDir,
}

impl FaultyStore {
    fn with(fault: Fault) -> Self {
        Self {
            files: RefCell::new(BTreeMap::new()),
            fail: Some(fault),
        }
    }

    fn put(&self, path: &Path, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), bytes.to_vec());
    }

    fn get(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.borrow().get(path).cloned()
    }

    fn fails(&self, fault: Fault) -> io::Result<()> {
        if self.fail == Some(fault) {
            return Err(io::Error::other("injected failure"));
        }
        Ok(())
    }
}

impl FileStore for FaultyStore {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.fails(Fault::WriteNew)?;
        if self.files.borrow().contains_key(path) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "taken"));
        }
        self.put(path, bytes);
        Ok(())
    }

    fn sync_file(&self, _path: &Path) -> io::Result<()> {
        self.fails(Fault::SyncFile)
    }

    fn sync_dir(&self, _path: &Path) -> io::Result<()> {
        self.fails(Fault::SyncDir)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if to.extension().is_some_and(|extension| extension == "bak") {
            self.fails(Fault::BackupRename)?;
        } else {
            self.fails(Fault::PrimaryRename)?;
        }
        let bytes = self
            .files
            .borrow_mut()
            .remove(from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))?;
        self.put(to, &bytes);
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }
}

fn target() -> DocumentTarget {
    DocumentTarget::new("/experiments/orbit.kagami").expect("valid target")
}

fn metadata(saved: &str) -> DocumentMetadata {
    DocumentMetadata {
        generator: "kagami 0.1.0".to_owned(),
        created: "2026-09-07T10:00:00Z".to_owned(),
        saved: saved.to_owned(),
        saved_revision: 0,
    }
}

/// A document distinguishable by its `saved` stamp.
fn document(saved: &str) -> ExperimentDocument {
    let experiment = Experiment::new();
    ExperimentDocument::of(&experiment, &experiment.snapshot(), metadata(saved))
}

/// The `saved` stamp of whatever is at `path`, if it decodes.
fn stamp_at(store: &FaultyStore, path: &str) -> Option<String> {
    let bytes = store.get(Path::new(path))?;
    serde_json::from_slice::<ExperimentDocument>(&bytes)
        .ok()
        .map(|document| document.metadata.saved)
}

#[test]
fn a_first_save_writes_the_document_and_no_backup() {
    let store = FaultyStore::default();
    save(&store, &target(), &document("first")).expect("nothing in the way");

    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami").as_deref(),
        Some("first")
    );
    assert!(!store.exists(Path::new("/experiments/orbit.kagami.bak")));
    // The temporary is gone: it became the document.
    assert!(!store.exists(Path::new("/experiments/orbit.kagami.tmp0")));
}

#[test]
fn a_second_save_keeps_the_previous_document_as_the_backup() {
    let store = FaultyStore::default();
    save(&store, &target(), &document("first")).expect("saved");
    save(&store, &target(), &document("second")).expect("saved");

    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami").as_deref(),
        Some("second")
    );
    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami.bak").as_deref(),
        Some("first")
    );
}

#[test]
fn a_failure_at_any_step_leaves_the_previous_document_intact() {
    // The point of the whole protocol. Each of these is a real failure mode —
    // a full disk, a device error, a permission change mid-save — and none of
    // them may cost the document that was already there.
    for fault in [
        Fault::WriteNew,
        Fault::SyncFile,
        Fault::BackupRename,
        Fault::PrimaryRename,
        Fault::SyncDir,
    ] {
        let store = FaultyStore::with(fault);
        // Seed a good previous document by hand, since saving would fail.
        let previous = serde_json::to_vec_pretty(&document("previous")).expect("encodes");
        store.put(Path::new("/experiments/orbit.kagami"), &previous);

        let outcome = save(&store, &target(), &document("attempted"));

        if matches!(fault, Fault::SyncDir) {
            // The directory flush is the last step: by then the rename has
            // happened, so the *new* document is legitimately in place and
            // only its durability is in doubt. It is still reported.
            assert!(outcome.is_err(), "a failed flush is still a failure");
            assert_eq!(
                stamp_at(&store, "/experiments/orbit.kagami").as_deref(),
                Some("attempted")
            );
            assert_eq!(
                stamp_at(&store, "/experiments/orbit.kagami.bak").as_deref(),
                Some("previous"),
                "and the previous document is still recoverable"
            );
            continue;
        }

        assert!(outcome.is_err(), "the injected failure must be reported");
        // Either the primary is untouched, or it moved to the backup and is
        // still readable. What must never happen is losing it.
        let recoverable = stamp_at(&store, "/experiments/orbit.kagami")
            .or_else(|| stamp_at(&store, "/experiments/orbit.kagami.bak"));
        assert_eq!(
            recoverable.as_deref(),
            Some("previous"),
            "the previous document must survive a failure at any step"
        );
    }
}

#[test]
fn an_interrupted_save_never_leaves_a_half_written_document() {
    // Killed after the previous document was moved aside but before the new
    // one landed. This is the one window where the primary path holds
    // nothing — the cost of moving the old document rather than copying it —
    // and the previous document is still whole, in the backup.
    let store = FaultyStore::with(Fault::PrimaryRename);
    let previous = serde_json::to_vec_pretty(&document("previous")).expect("encodes");
    store.put(Path::new("/experiments/orbit.kagami"), &previous);

    let _ = save(&store, &target(), &document("attempted"));

    // Whatever is at the primary path, it is never half of a document.
    assert!(
        stamp_at(&store, "/experiments/orbit.kagami").is_none_or(|stamp| stamp == "previous"),
        "the primary is either absent or the previous document, never a fragment"
    );

    let loaded = load(&store, &target()).expect("the previous document survived");
    assert_eq!(loaded.document.metadata.saved, "previous");
    assert_eq!(loaded.from, LoadedFrom::Backup);
    assert!(
        loaded.from.is_recovery(),
        "opening from anywhere but the primary is worth telling the user about"
    );
}

#[test]
fn an_interrupted_write_leaves_the_document_completely_untouched() {
    // Killed *before* anything was moved — the far more common case, since
    // writing the bytes is the slow step. Nothing was disturbed at all.
    let store = FaultyStore::with(Fault::SyncFile);
    let previous = serde_json::to_vec_pretty(&document("previous")).expect("encodes");
    store.put(Path::new("/experiments/orbit.kagami"), &previous);

    let _ = save(&store, &target(), &document("attempted"));

    let loaded = load(&store, &target()).expect("the document is still there");
    assert_eq!(loaded.from, LoadedFrom::Primary);
    assert_eq!(loaded.document.metadata.saved, "previous");
    assert!(!loaded.from.is_recovery(), "nothing to warn about");
}

#[test]
fn a_corrupted_primary_falls_back_to_the_backup_and_says_so() {
    let store = FaultyStore::default();
    save(&store, &target(), &document("first")).expect("saved");
    save(&store, &target(), &document("second")).expect("saved");

    // Something truncated the primary.
    store.put(Path::new("/experiments/orbit.kagami"), b"{\"format\":");

    let loaded = load(&store, &target()).expect("the backup answers");
    assert_eq!(loaded.from, LoadedFrom::Backup);
    assert_eq!(loaded.document.metadata.saved, "first");
    assert!(
        loaded.from.is_recovery(),
        "reading anything but the document is worth telling the user about"
    );
}

#[test]
fn a_leftover_temporary_is_the_last_resort() {
    let store = FaultyStore::default();
    // Neither a document nor a backup, only an interrupted save's temporary.
    let bytes = serde_json::to_vec_pretty(&document("interrupted")).expect("encodes");
    store.put(Path::new("/experiments/orbit.kagami.tmp0"), &bytes);

    let loaded = load(&store, &target()).expect("something survived");
    assert_eq!(loaded.from, LoadedFrom::Temporary);
    assert_eq!(loaded.document.metadata.saved, "interrupted");
}

#[test]
fn a_corrupted_previous_document_is_not_promoted_to_the_backup() {
    let store = FaultyStore::default();
    save(&store, &target(), &document("good")).expect("saved");
    // The primary is damaged, but the backup from an earlier save is fine.
    let older = serde_json::to_vec_pretty(&document("older but readable")).expect("encodes");
    store.put(Path::new("/experiments/orbit.kagami.bak"), &older);
    store.put(Path::new("/experiments/orbit.kagami"), b"not a document");

    save(&store, &target(), &document("new")).expect("saved");

    // The damaged bytes did not replace a backup that was still good: a
    // backup is a copy of the last *verified* document, not of whatever was
    // on disk.
    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami").as_deref(),
        Some("new")
    );
    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami.bak").as_deref(),
        Some("older but readable")
    );
}

#[test]
fn a_stale_temporary_does_not_block_a_save() {
    let store = FaultyStore::default();
    // Something left `.tmp0` behind. A save must not follow or clobber it.
    store.put(Path::new("/experiments/orbit.kagami.tmp0"), b"leftover");

    save(&store, &target(), &document("first")).expect("the next candidate is used");

    assert_eq!(
        stamp_at(&store, "/experiments/orbit.kagami").as_deref(),
        Some("first")
    );
    assert_eq!(
        store
            .get(Path::new("/experiments/orbit.kagami.tmp0"))
            .as_deref(),
        Some(b"leftover".as_slice()),
        "the file that was in the way is untouched"
    );
}

#[test]
fn a_document_that_was_saved_round_trips_back_into_an_experiment() {
    let store = FaultyStore::default();
    save(&store, &target(), &document("first")).expect("saved");

    let loaded = load(&store, &target()).expect("readable");
    loaded
        .document
        .into_experiment(&schemas(), &Limits::DEFAULT)
        .expect("a document this build wrote");
}

#[test]
fn nothing_readable_anywhere_is_a_typed_refusal() {
    let store = FaultyStore::default();
    let error = load(&store, &target()).expect_err("there is no document");
    assert!(error.to_string().contains("orbit.kagami"), "{error}");
}
