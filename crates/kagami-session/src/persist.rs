//! Naming what was written, so a completion cannot acknowledge the wrong
//! thing.
//!
//! Nothing here performs IO, and this crate deliberately does not decide the
//! experiment file's format — that is task K4's, and path selection, byte
//! writes, clocks and prompts belong to the imperative shell above it. What
//! the authority owns is the *bookkeeping*: which revision is on disk, and
//! where.
//!
//! # Why a completion has to say what it completed
//!
//! A save is asynchronous. Between capturing revision N and the bytes landing,
//! a user can keep typing, so the experiment can already be at N+1 when the
//! write reports success. An unqualified "it was saved" marks that N+1 clean
//! and the unsaved edits are lost the next time anyone trusts the dirty flag.
//! [`DocumentAuthority::acknowledge_save`](crate::DocumentAuthority::acknowledge_save)
//! therefore takes both the revision that was captured and the target it was
//! written to, and answers whether that made the session clean.

use std::fmt;
use std::path::{Path, PathBuf};

use kagami_document::ExperimentRevision;
use thiserror::Error;

/// Longest accepted document target, in bytes.
///
/// A bound rather than a filesystem limit: the target is caller-supplied, is
/// held in session state, and is reported in every view.
pub const MAX_TARGET_BYTES: usize = 4_096;

/// Why a document target was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TargetError {
    /// The path was empty.
    #[error("a document target must not be empty")]
    Empty,
    /// The path was longer than [`MAX_TARGET_BYTES`].
    #[error("document target is {found} bytes, over the limit of {limit}")]
    TooLong {
        /// The submitted length.
        found: usize,
        /// The limit it exceeded.
        limit: usize,
    },
}

/// Where an experiment is saved.
///
/// A validated path, not an open file: this crate holds it to answer "what is
/// this window editing" and to decide whether a completion is for the target
/// currently in force. It is adopted only by a *successful* acknowledgement,
/// which is what makes a failed Save As leave the previous target in place
/// instead of retargeting a document that was never written.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentTarget(PathBuf);

impl DocumentTarget {
    /// Validate and construct a target.
    ///
    /// # Errors
    ///
    /// Returns [`TargetError`] for an empty or over-long path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, TargetError> {
        let path = path.into();
        let length = path.as_os_str().len();
        if length == 0 {
            return Err(TargetError::Empty);
        }
        if length > MAX_TARGET_BYTES {
            return Err(TargetError::TooLong {
                found: length,
                limit: MAX_TARGET_BYTES,
            });
        }
        Ok(Self(path))
    }

    /// The path this target names.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for DocumentTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.display())
    }
}

/// What acknowledging a completed write did to the session.
///
/// Generic over the revision because ADR 0022 gives the file two of them: the
/// experiment revision the authority owns, and the
/// [`ViewRevision`](crate::ViewRevision) the client's authoring view owns. The
/// three outcomes are the same three for both — one write, one captured
/// revision, one question about whether it is still current — and describing
/// them twice would be two places for the out-of-order rule to drift apart.
/// The default parameter keeps the experiment spelling unqualified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAcknowledgement<R = ExperimentRevision> {
    /// The acknowledged revision is the one in force, so the session is clean.
    Clean,
    /// The write succeeded, but the session has moved on since the revision was
    /// captured. The target is adopted; the session stays dirty, because what
    /// is on disk is not what is here.
    Superseded {
        /// The revision now in force.
        current: R,
    },
    /// The acknowledgement was for a revision older than one already
    /// acknowledged — two writes completing out of order. It changed nothing
    /// at all, including the target.
    Stale {
        /// The most recent revision successfully acknowledged.
        acknowledged: R,
    },
}

impl<R> SaveAcknowledgement<R> {
    /// `true` when the session is clean as a result.
    pub const fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_must_be_present_and_bounded() {
        assert_eq!(DocumentTarget::new(""), Err(TargetError::Empty));
        assert_eq!(
            DocumentTarget::new("a".repeat(MAX_TARGET_BYTES + 1)),
            Err(TargetError::TooLong {
                found: MAX_TARGET_BYTES + 1,
                limit: MAX_TARGET_BYTES,
            })
        );
        assert_eq!(
            DocumentTarget::new("/tmp/orbit.kagami")
                .expect("valid")
                .path(),
            Path::new("/tmp/orbit.kagami")
        );
    }

    #[test]
    fn only_a_current_acknowledgement_is_clean() {
        assert!(SaveAcknowledgement::<ExperimentRevision>::Clean.is_clean());
        assert!(
            !SaveAcknowledgement::Superseded {
                current: ExperimentRevision::INITIAL.next(),
            }
            .is_clean()
        );
        assert!(
            !SaveAcknowledgement::Stale {
                acknowledged: ExperimentRevision::INITIAL,
            }
            .is_clean()
        );
    }
}
