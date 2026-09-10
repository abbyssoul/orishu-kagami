//! Whether this client is editing initial conditions or watching a run.
//!
//! ADR 0022 gives Kagami two explicit workspace modes and one rule about them:
//! while a session is in Observation/replay, *neither* UI nor MCP authoring is
//! routed to the authority. This module is that rule.
//!
//! # Why it is here and not in the authority
//!
//! ADR 0022 is explicit in both directions. The restriction "belongs to the
//! application/session workflow, not to the document authority", and making
//! client mode "a reason for the document authority to reject an otherwise
//! valid command" is a stated non-goal — the authority stays independent of
//! run state and may own a successor draft while another client observes a
//! run. So [`Workspace`] sits *beside*
//! [`DocumentAuthority`](crate::DocumentAuthority), and a caller consults it
//! before submitting rather than the authority consulting it afterwards.
//!
//! It is nonetheless a rule, so it is not the app's. `apps/kagami` owns
//! presentation and the effects it executes; every decision lives in this
//! crate or in `kagami-document`. That is also what lets the embedded MCP
//! adapter reach the *same* gate the UI does, which ADR 0006 requires.
//!
//! # Leaving is a consequence, not an action
//!
//! This module performs nothing. [`Workspace::edit_initial_conditions`]
//! returns a [`LeaveConsequence`] the shell carries out, which is what keeps
//! the decision sans-IO and — more importantly — makes ADR 0022's asymmetry
//! structural rather than remembered: leaving a local preview stops it, and
//! leaving a remote run yields [`LeaveConsequence::Detach`], because
//! `Detach` is the only thing this type can produce for one. Stopping a
//! cluster run is a separate privileged action, and there is no value here
//! that could ask for it.
//!
//! # What is missing, and where it comes from
//!
//! Nothing in this build *enters* Observation/replay: submitting a run is
//! K-RUN's and previewing one is K-PREVIEW's, and neither exists. [`RunLabel`]
//! is a deliberate stand-in for S-OBSERVE's run identity — a bounded opaque
//! label, so replacing it is a type substitution and not a redesign.

use thiserror::Error;

use crate::command::SessionCommand;
use crate::identity::bounded_identity;

bounded_identity!(
    /// Which run a window is watching.
    ///
    /// A stand-in for the run identity S-OBSERVE will define, held opaque on
    /// purpose: this module needs to tell two attachments apart and to name one
    /// in a message, and it needs nothing else. When the real identity arrives
    /// it replaces this type without changing a transition.
    RunLabel
);

/// Where an observed run is executing.
///
/// The distinction exists for exactly one reason: it decides what leaving
/// costs. ADR 0022 makes stopping a local preview implicit in leaving it, and
/// stopping a cluster run never implicit in anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOrigin {
    /// In-process, inside this Kagami. Leaving stops it.
    LocalPreview,
    /// On an Orishu cluster. Leaving detaches this window and nothing more.
    Remote,
}

/// One run this window is attached to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunAttachment {
    origin: RunOrigin,
    run: RunLabel,
}

impl RunAttachment {
    /// Attach to a local in-process preview.
    pub const fn local_preview(run: RunLabel) -> Self {
        Self {
            origin: RunOrigin::LocalPreview,
            run,
        }
    }

    /// Attach to a run executing on a cluster.
    pub const fn remote(run: RunLabel) -> Self {
        Self {
            origin: RunOrigin::Remote,
            run,
        }
    }

    /// Where it is executing.
    pub const fn origin(&self) -> RunOrigin {
        self.origin
    }

    /// Which run it is.
    pub const fn run(&self) -> &RunLabel {
        &self.run
    }
}

/// What the shell must do as a result of leaving Observation/replay.
///
/// Returned rather than performed. There is no third variant, which is how
/// "stopping the cluster run is a separate privileged action and is never
/// implied" is enforced by the type instead of by a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaveConsequence {
    /// Stop the local preview that was being watched.
    StopPreview {
        /// The preview to stop.
        run: RunLabel,
    },
    /// Detach this window from a cluster run, which keeps executing.
    Detach {
        /// The run to stop receiving.
        run: RunLabel,
    },
}

impl LeaveConsequence {
    /// A stable identifier for this consequence.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StopPreview { .. } => "stop_preview",
            Self::Detach { .. } => "detach",
        }
    }
}

/// Which mode a window is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceMode {
    /// Editing the experiment's initial conditions. Document commands, undo
    /// and redo are available.
    Authoring,
    /// Watching one run. Playback and visualisation are available; no
    /// authoring is.
    Observing(RunAttachment),
}

/// Why a workspace transition or submission was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WorkspaceRejection {
    /// An authoring command arrived while a run was being watched.
    ///
    /// Refused here rather than routed and refused later, so the authority
    /// never sees a command whose mode was wrong. ADR 0022: an MCP caller
    /// "must explicitly request the same transition to Authoring and receives
    /// its local-stop/remote-detach consequence; it cannot switch modes
    /// accidentally as a side effect of an edit."
    #[error("`{kind}` needs Authoring mode, and this window is observing run `{run}`")]
    NotAuthoring {
        /// The submission that was refused.
        kind: &'static str,
        /// The run being watched.
        run: RunLabel,
    },
    /// Edit initial conditions was requested with nothing to leave.
    #[error("this window is already in Authoring mode")]
    AlreadyAuthoring,
    /// A second run was requested without leaving the first.
    ///
    /// Refused rather than swapped: replacing an attachment silently would
    /// abandon a local preview without ever producing the
    /// [`LeaveConsequence::StopPreview`] that stops it.
    #[error("this window is already observing run `{run}`; leave it first")]
    AlreadyObserving {
        /// The run already attached.
        run: RunLabel,
    },
}

impl WorkspaceRejection {
    /// A stable identifier for this reason.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotAuthoring { .. } => "not_authoring",
            Self::AlreadyAuthoring => "already_authoring",
            Self::AlreadyObserving { .. } => "already_observing",
        }
    }
}

/// One window's mode, and the transitions between them.
///
/// Holds no document, no run and no camera: what it decides is whether
/// authoring is routed at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    mode: WorkspaceMode,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::authoring()
    }
}

impl Workspace {
    /// A window in Authoring mode.
    ///
    /// The only starting state there is. ADR 0022: "Opening a document always
    /// starts in Authoring and never resumes a run merely because presentation
    /// defaults were saved" — and because a saved file carries no run, there is
    /// nothing a document could say that would construct any other mode.
    pub const fn authoring() -> Self {
        Self {
            mode: WorkspaceMode::Authoring,
        }
    }

    /// Which mode this window is in.
    pub const fn mode(&self) -> &WorkspaceMode {
        &self.mode
    }

    /// `true` when authoring is available.
    pub const fn is_authoring(&self) -> bool {
        matches!(self.mode, WorkspaceMode::Authoring)
    }

    /// The run being watched, if any.
    pub const fn attachment(&self) -> Option<&RunAttachment> {
        match &self.mode {
            WorkspaceMode::Authoring => None,
            WorkspaceMode::Observing(attachment) => Some(attachment),
        }
    }

    /// Enter Observation/replay, watching `attachment`.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceRejection::AlreadyObserving`] when this window is
    /// already attached to a run.
    pub fn observe(&mut self, attachment: RunAttachment) -> Result<(), WorkspaceRejection> {
        if let WorkspaceMode::Observing(current) = &self.mode {
            return Err(WorkspaceRejection::AlreadyObserving {
                run: current.run.clone(),
            });
        }
        self.mode = WorkspaceMode::Observing(attachment);
        Ok(())
    }

    /// Return to Authoring, reporting what leaving costs.
    ///
    /// The transition happens here; the consequence is the caller's to carry
    /// out. Nothing about the observed run is adopted — ADR 0022 keeps
    /// adopting a computed state a separate, validated workflow, so this
    /// returns to the authored scene exactly as it was.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceRejection::AlreadyAuthoring`] when there is no run
    /// to leave. Refused rather than ignored, for the same reason an undo with
    /// an empty history is refused: an affordance that quietly does nothing is
    /// worse than one that says so.
    pub fn edit_initial_conditions(&mut self) -> Result<LeaveConsequence, WorkspaceRejection> {
        let WorkspaceMode::Observing(attachment) =
            std::mem::replace(&mut self.mode, WorkspaceMode::Authoring)
        else {
            return Err(WorkspaceRejection::AlreadyAuthoring);
        };
        Ok(match attachment.origin {
            RunOrigin::LocalPreview => LeaveConsequence::StopPreview {
                run: attachment.run,
            },
            RunOrigin::Remote => LeaveConsequence::Detach {
                run: attachment.run,
            },
        })
    }

    /// Decide whether `command` may be routed to the authority.
    ///
    /// Every [`SessionCommand`] is gated, not just the mutating ones. ADR 0022
    /// says authoring is not routed at all while observing, and the lifecycle
    /// commands are the reason that matters rather than being pedantic: opening
    /// or replacing the experiment under someone watching a run would change
    /// the document out from under a mode that shows no document controls.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceRejection::NotAuthoring`] while a run is being
    /// watched.
    pub fn admit(&self, command: &SessionCommand) -> Result<(), WorkspaceRejection> {
        self.admit_kind(command.kind())
    }

    /// The same decision, for a caller that cannot build the command yet.
    ///
    /// [`SessionCommand::Open`] carries a whole decoded experiment, so asking
    /// permission requires reading and decoding a file first — which is work
    /// worth skipping when the answer is already no. `kind` should be the
    /// [`SessionCommand::kind`] of the submission being contemplated; it names
    /// the operation in the refusal and decides nothing.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceRejection::NotAuthoring`] while a run is being
    /// watched.
    pub fn admit_kind(&self, kind: &'static str) -> Result<(), WorkspaceRejection> {
        match &self.mode {
            WorkspaceMode::Authoring => Ok(()),
            WorkspaceMode::Observing(attachment) => Err(WorkspaceRejection::NotAuthoring {
                kind,
                run: attachment.run.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(label: &str) -> RunLabel {
        RunLabel::new(label).expect("a constant label is valid")
    }

    #[test]
    fn a_window_starts_in_authoring() {
        let workspace = Workspace::default();
        assert!(workspace.is_authoring());
        assert_eq!(workspace.attachment(), None);
        assert_eq!(workspace.admit(&SessionCommand::Undo), Ok(()));
    }

    #[test]
    fn observing_refuses_every_authoring_submission() {
        let mut workspace = Workspace::authoring();
        workspace
            .observe(RunAttachment::remote(run("run-7")))
            .expect("nothing attached yet");

        for command in [
            SessionCommand::Undo,
            SessionCommand::Redo,
            SessionCommand::Edit(Vec::new()),
            SessionCommand::BeginInteractiveEdit,
            SessionCommand::New {
                discard_unsaved: true,
            },
        ] {
            let refusal = workspace
                .admit(&command)
                .expect_err("observing routes no authoring");
            assert_eq!(refusal.code(), "not_authoring");
            assert_eq!(
                refusal,
                WorkspaceRejection::NotAuthoring {
                    kind: command.kind(),
                    run: run("run-7"),
                }
            );
        }
    }

    #[test]
    fn leaving_a_local_preview_stops_it_and_leaving_a_remote_run_only_detaches() {
        let mut workspace = Workspace::authoring();
        workspace
            .observe(RunAttachment::local_preview(run("preview-1")))
            .expect("nothing attached");
        assert_eq!(
            workspace.edit_initial_conditions(),
            Ok(LeaveConsequence::StopPreview {
                run: run("preview-1")
            })
        );
        assert!(workspace.is_authoring());

        workspace
            .observe(RunAttachment::remote(run("cluster-9")))
            .expect("back in authoring, so nothing attached");
        let consequence = workspace.edit_initial_conditions().expect("a run to leave");
        // There is no variant that stops a cluster run, which is the point.
        assert_eq!(
            consequence,
            LeaveConsequence::Detach {
                run: run("cluster-9")
            }
        );
        assert!(workspace.is_authoring());
    }

    #[test]
    fn leaving_authoring_mode_is_refused_rather_than_ignored() {
        let mut workspace = Workspace::authoring();
        assert_eq!(
            workspace.edit_initial_conditions(),
            Err(WorkspaceRejection::AlreadyAuthoring)
        );
        assert!(workspace.is_authoring());
    }

    #[test]
    fn a_second_run_cannot_silently_replace_the_first() {
        let mut workspace = Workspace::authoring();
        workspace
            .observe(RunAttachment::local_preview(run("preview-1")))
            .expect("nothing attached");

        // Swapping would abandon the preview without ever producing the
        // StopPreview that stops it.
        assert_eq!(
            workspace.observe(RunAttachment::remote(run("cluster-9"))),
            Err(WorkspaceRejection::AlreadyObserving {
                run: run("preview-1")
            })
        );
        assert_eq!(
            workspace.attachment().map(RunAttachment::origin),
            Some(RunOrigin::LocalPreview)
        );
    }

    #[test]
    fn admitting_a_command_never_changes_mode() {
        // The gate is a read. A refused edit must not be a way to switch modes
        // as a side effect (ADR 0022).
        let mut workspace = Workspace::authoring();
        workspace
            .observe(RunAttachment::remote(run("run-7")))
            .expect("nothing attached");
        let before = workspace.clone();

        let _ = workspace.admit(&SessionCommand::Undo);
        let _ = workspace.admit(&SessionCommand::Edit(Vec::new()));

        assert_eq!(workspace, before);
    }

    #[test]
    fn admitting_by_kind_agrees_with_admitting_the_command() {
        // `admit_kind` exists so a caller that has not decoded a file yet can
        // ask. It must be the same decision, and the kinds a caller spells by
        // hand must be the ones `SessionCommand::kind` produces — the app
        // passes "open" for a submission it cannot construct yet.
        let mut workspace = Workspace::authoring();
        workspace
            .observe(RunAttachment::remote(run("run-7")))
            .expect("nothing attached");

        for command in [
            SessionCommand::Undo,
            SessionCommand::New {
                discard_unsaved: true,
            },
        ] {
            assert_eq!(
                workspace.admit(&command),
                workspace.admit_kind(command.kind())
            );
        }

        assert_eq!(
            workspace.admit_kind("open"),
            Err(WorkspaceRejection::NotAuthoring {
                kind: "open",
                run: run("run-7"),
            })
        );
        assert_eq!(Workspace::authoring().admit_kind("open"), Ok(()));
    }

    #[test]
    fn a_run_label_is_bounded_like_any_other_identity() {
        assert!(RunLabel::new("").is_err());
        assert!(RunLabel::new("a".repeat(crate::MAX_IDENTITY_BYTES + 1)).is_err());
        assert_eq!(run("run-7").as_str(), "run-7");
    }
}
