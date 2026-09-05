//! The single deterministic merge path.
//!
//! Every membership fact — piggybacked gossip, an `Announce`, an anti-entropy
//! reply, a bootstrap snapshot — converges here. One path means one place
//! where staleness, conflict, and tombstone fencing are decided, rather than
//! three implementations that disagree under load.
//!
//! # Two orderings, deliberately
//!
//! A member record carries both a [`VersionTuple`] and a SWIM
//! liveness/incarnation pair, and they order different things:
//!
//! - Descriptive fields (endpoints, capability, accepts, capacity, label) are
//!   ordered by version. Their writer is whoever last described the node.
//! - Liveness is ordered by the SWIM override rules over incarnations. Its
//!   writer is the failure detector, and only the subject itself may raise its
//!   own incarnation.
//!
//! Collapsing the two would mean a node that merely re-advertised its
//! addresses could also overwrite a suspicion, or that a stale suspicion could
//! revert an address change. So a single delta may be adopted in part.

use orishu_identity::{Incarnation, MembershipTombstone, NodeId, VersionTuple};

use crate::{
    effect::{Announcement, ChangeRecord, Diagnostic},
    gossip::DeltaBody,
    model::{Liveness, Member, Membership},
};

/// What merging one delta did.
///
/// `Adopted` is much larger than the other variants because it carries the
/// merged record. Boxing it to even the variants out would trade a fixed
/// stack move for a heap allocation on every adopted delta — the common case
/// during steady-state gossip — so the size difference is the cheaper side of
/// the trade here.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeOutcome {
    /// State changed. `body` is the *locally held* result, which is what gets
    /// re-gossiped: forwarding the incoming delta verbatim would spread a
    /// partially adopted record that no node actually holds.
    Adopted {
        body: DeltaBody,
        change: Option<ChangeRecord>,
    },
    /// The delta matched what is already held, exactly. Replay is a no-op.
    Idempotent,
    /// The delta was refused, with a reason.
    Rejected(Diagnostic),
    /// The delta claims this node is suspected or dead. Only this node may
    /// speak for itself, so the caller refutes rather than adopting.
    SelfChallenged { incarnation: Incarnation },
    /// This node was removed from the formation by an operator.
    SelfRemoved { tombstone: MembershipTombstone },
}

/// Whether an incoming SWIM state supersedes the one currently held.
///
/// Implements the override table in `docs/protocol-p2p.md`:
///
/// - `Alive(n)` overrides `Alive(m)` or `Suspect(m)` when `n > m`. A
///   refutation must carry a *strictly* newer incarnation, otherwise a
///   replayed old `Alive` would clear a fresh suspicion.
/// - `Suspect(n)` overrides `Alive(m)` when `n >= m`, and another `Suspect(m)`
///   when `n > m`. Equality matters here: a probe times out against the
///   target's *current* incarnation, so requiring `n > m` would make suspicion
///   unreachable.
/// - `Dead(n)` overrides `Alive(m)` or `Suspect(m)` for any `n >= m`.
/// - Nothing overrides `Dead`. Within one formation, death is terminal:
///   readmission means a new formation-assigned identity, not a resurrected
///   record.
#[must_use]
pub(crate) fn swim_supersedes(
    incoming_state: Liveness,
    incoming_incarnation: Incarnation,
    current_state: Liveness,
    current_incarnation: Incarnation,
) -> bool {
    match (current_state, incoming_state) {
        (Liveness::Dead, _) => false,
        (_, Liveness::Dead) => incoming_incarnation >= current_incarnation,
        (Liveness::Alive, Liveness::Suspected) => incoming_incarnation >= current_incarnation,
        (Liveness::Suspected, Liveness::Suspected)
        | (Liveness::Alive, Liveness::Alive)
        | (Liveness::Suspected, Liveness::Alive) => incoming_incarnation > current_incarnation,
    }
}

/// Merges one membership-owned delta into the model.
///
/// Foreign deltas never reach here; the caller hands them to their owner.
pub(crate) fn merge_delta(model: &mut Membership, body: DeltaBody) -> MergeOutcome {
    match body {
        DeltaBody::MembershipUpdate(member) => merge_member(model, member),
        DeltaBody::TombstoneUpdate(tombstone) => merge_tombstone(model, tombstone),
        DeltaBody::BlocklistUpdate(entry) => merge_blocklist(model, entry),
        // Reaching here is a caller bug: foreign deltas are split off before
        // merging so that membership never decodes another subsystem's data.
        DeltaBody::Foreign(_) => MergeOutcome::Rejected(Diagnostic::Unexpected {
            what: "foreign delta reached the membership merge path",
        }),
    }
}

/// Merges a member record.
fn merge_member(model: &mut Membership, incoming: Member) -> MergeOutcome {
    if let Err(error) = incoming.validate(model.limits()) {
        return MergeOutcome::Rejected(Diagnostic::MalformedRecord {
            entity: format!("member:{}", incoming.id),
            error,
        });
    }

    if model.is_tombstoned(&incoming.id) {
        // A tombstone outranks every liveness claim, however new. Without this
        // fence, one stale `Alive` would readmit a node an operator removed.
        return MergeOutcome::Rejected(Diagnostic::TombstoneFenced { node: incoming.id });
    }

    if &incoming.id == model.local_id() {
        return match incoming.liveness {
            Liveness::Suspected | Liveness::Dead if incoming.incarnation >= model.incarnation() => {
                MergeOutcome::SelfChallenged {
                    incarnation: incoming.incarnation,
                }
            }
            _ => MergeOutcome::Rejected(Diagnostic::SelfAnnouncementIgnored {
                announcement: Announcement::Alive,
            }),
        };
    }

    let Some(current) = model.members().get(&incoming.id).cloned() else {
        let limit = model.limits().max_members();
        if model.members().len() >= limit {
            return MergeOutcome::Rejected(Diagnostic::LimitExceeded {
                limit: "maxMembers",
                value: model.members().len() + 1,
                max: limit,
            });
        }
        let change = ChangeRecord::LivenessChanged {
            node: incoming.id.clone(),
            from: Liveness::Alive,
            to: incoming.liveness,
            incarnation: incoming.incarnation,
        };
        model
            .members_mut()
            .insert(incoming.id.clone(), incoming.clone());
        return MergeOutcome::Adopted {
            body: DeltaBody::MembershipUpdate(incoming),
            change: Some(change),
        };
    };

    // Gossip may re-describe a member; it may never rebind its certificate.
    // Rotation is a separate signed exchange, so a fingerprint change arriving
    // here is either a bug or an impersonation attempt.
    if incoming.cert_fingerprint != current.cert_fingerprint {
        return MergeOutcome::Rejected(Diagnostic::CertificateMismatch { node: incoming.id });
    }

    if incoming == current {
        return MergeOutcome::Idempotent;
    }

    if incoming.version == current.version {
        // Same version, different payload. Not a tie to break silently: one of
        // the two writers is buggy or hostile, and picking a winner by arrival
        // order would make the formation's state depend on packet timing.
        return MergeOutcome::Rejected(Diagnostic::VersionConflict {
            entity: format!("member:{}", incoming.id),
            version: incoming.version,
        });
    }

    let descriptive_wins = incoming.version > current.version;
    let liveness_wins = swim_supersedes(
        incoming.liveness,
        incoming.incarnation,
        current.liveness,
        current.incarnation,
    );

    if !descriptive_wins && !liveness_wins {
        return MergeOutcome::Rejected(Diagnostic::StaleDelta {
            entity: format!("member:{}", incoming.id),
        });
    }

    let mut change = None;
    let merged = {
        let held = model
            .members_mut()
            .get_mut(&incoming.id)
            .expect("member was present a moment ago");
        if descriptive_wins {
            held.name = incoming.name;
            held.protocol = incoming.protocol;
            held.endpoints = incoming.endpoints;
            held.accepts = incoming.accepts;
            held.capacity = incoming.capacity;
            held.capabilities = incoming.capabilities;
            held.version = incoming.version;
        }
        if liveness_wins {
            change = Some(ChangeRecord::LivenessChanged {
                node: held.id.clone(),
                from: held.liveness,
                to: incoming.liveness,
                incarnation: incoming.incarnation,
            });
            held.liveness = incoming.liveness;
            held.incarnation = incoming.incarnation;
        }
        held.clone()
    };

    if liveness_wins && merged.liveness != Liveness::Suspected {
        // A resolved liveness has no suspicion deadline to keep.
        model.suspicions_mut().remove(&merged.id);
    }

    MergeOutcome::Adopted {
        body: DeltaBody::MembershipUpdate(merged),
        change,
    }
}

/// Merges a membership tombstone.
fn merge_tombstone(model: &mut Membership, incoming: MembershipTombstone) -> MergeOutcome {
    if let Some(reason) = &incoming.reason
        && reason.len() > model.limits().max_text_len()
    {
        return MergeOutcome::Rejected(Diagnostic::MalformedRecord {
            entity: format!("tombstone:{}", incoming.node_id),
            error: crate::model::ValidationError::TooLong {
                field: "tombstone.reason",
                len: reason.len(),
                max: model.limits().max_text_len(),
            },
        });
    }

    if let Some(current) = model.tombstones().get(&incoming.node_id) {
        if current == &incoming {
            return MergeOutcome::Idempotent;
        }
        if current.version == incoming.version {
            return MergeOutcome::Rejected(Diagnostic::VersionConflict {
                entity: format!("tombstone:{}", incoming.node_id),
                version: incoming.version,
            });
        }
        if current.version > incoming.version {
            return MergeOutcome::Rejected(Diagnostic::StaleDelta {
                entity: format!("tombstone:{}", incoming.node_id),
            });
        }
    } else {
        let limit = model.limits().max_tombstones();
        if model.tombstones().len() >= limit {
            return MergeOutcome::Rejected(Diagnostic::LimitExceeded {
                limit: "maxTombstones",
                value: model.tombstones().len() + 1,
                max: limit,
            });
        }
    }

    let node = incoming.node_id.clone();
    let mode = incoming.removal_mode;
    let is_self = &node == model.local_id();

    model
        .tombstones_mut()
        .insert(node.clone(), incoming.clone());
    model.members_mut().remove(&node);
    model.suspicions_mut().remove(&node);
    model.forget_sender(&node);
    let doomed: Vec<_> = model
        .probes()
        .iter()
        .filter(|(_, probe)| probe.target == node)
        .map(|(id, _)| *id)
        .collect();
    for probe in doomed {
        model.probes_mut().remove(&probe);
    }

    if is_self {
        return MergeOutcome::SelfRemoved {
            tombstone: incoming,
        };
    }

    MergeOutcome::Adopted {
        body: DeltaBody::TombstoneUpdate(incoming),
        change: Some(ChangeRecord::MemberRemoved { node, mode }),
    }
}

/// Merges a blocklist entry.
fn merge_blocklist(model: &mut Membership, incoming: crate::model::BlocklistEntry) -> MergeOutcome {
    if let Err(error) = incoming.validate(model.limits()) {
        return MergeOutcome::Rejected(Diagnostic::MalformedRecord {
            entity: format!("blocklist:{}", incoming.key),
            error,
        });
    }

    if let Some(current) = model.blocklist().get(&incoming.key) {
        if current == &incoming {
            return MergeOutcome::Idempotent;
        }
        if current.version == incoming.version {
            return MergeOutcome::Rejected(Diagnostic::VersionConflict {
                entity: format!("blocklist:{}", incoming.key),
                version: incoming.version,
            });
        }
        if current.version > incoming.version {
            return MergeOutcome::Rejected(Diagnostic::StaleDelta {
                entity: format!("blocklist:{}", incoming.key),
            });
        }
    } else {
        let limit = model.limits().max_blocklist_entries();
        if model.blocklist().len() >= limit {
            return MergeOutcome::Rejected(Diagnostic::LimitExceeded {
                limit: "maxBlocklistEntries",
                value: model.blocklist().len() + 1,
                max: limit,
            });
        }
    }

    model
        .blocklist_mut()
        .insert(incoming.key.clone(), incoming.clone());
    MergeOutcome::Adopted {
        body: DeltaBody::BlocklistUpdate(incoming),
        change: None,
    }
}

/// Applies a SWIM announcement about `target` to the model.
///
/// Announcements are the same liveness decision as a gossiped member record,
/// expressed compactly, so they route through [`swim_supersedes`] too rather
/// than growing a second set of override rules.
pub(crate) fn apply_announcement(
    model: &mut Membership,
    sender: &NodeId,
    announcement: Announcement,
    target: &NodeId,
    incarnation: Incarnation,
) -> MergeOutcome {
    if announcement == Announcement::Leave && sender != target {
        // Only a node may announce its own departure. Accepting a third-party
        // `Leave` would give any member a one-datagram eviction primitive.
        return MergeOutcome::Rejected(Diagnostic::SpoofedLeave {
            sender: sender.clone(),
            subject: target.clone(),
        });
    }

    if model.is_tombstoned(target) {
        return MergeOutcome::Rejected(Diagnostic::TombstoneFenced {
            node: target.clone(),
        });
    }

    if target == model.local_id() {
        return match announcement {
            Announcement::Suspect | Announcement::Dead if incarnation >= model.incarnation() => {
                MergeOutcome::SelfChallenged { incarnation }
            }
            _ => MergeOutcome::Rejected(Diagnostic::SelfAnnouncementIgnored { announcement }),
        };
    }

    let Some(current) = model.members().get(target).cloned() else {
        return MergeOutcome::Rejected(Diagnostic::UnknownSender {
            claimed: target.clone(),
        });
    };

    // A voluntary departure is a self-declared death: it must outrank the
    // subject's own latest `Alive`, and it must not be undone by a replay of
    // one, so it lands at the next incarnation.
    let (state, effective) = match announcement {
        Announcement::Alive => (Liveness::Alive, incarnation),
        Announcement::Suspect => (Liveness::Suspected, incarnation),
        Announcement::Dead => (Liveness::Dead, incarnation),
        Announcement::Leave => (
            Liveness::Dead,
            incarnation.checked_next().unwrap_or(incarnation),
        ),
    };

    if !swim_supersedes(state, effective, current.liveness, current.incarnation) {
        return MergeOutcome::Rejected(Diagnostic::StaleDelta {
            entity: format!("member:{target}"),
        });
    }

    let merged = {
        let held = model
            .members_mut()
            .get_mut(target)
            .expect("member was present a moment ago");
        held.liveness = state;
        held.incarnation = effective;
        held.clone()
    };
    if state != Liveness::Suspected {
        model.suspicions_mut().remove(target);
    }

    let change = if announcement == Announcement::Leave {
        ChangeRecord::MemberLeft {
            node: target.clone(),
        }
    } else {
        ChangeRecord::LivenessChanged {
            node: target.clone(),
            from: current.liveness,
            to: state,
            incarnation: effective,
        }
    };

    MergeOutcome::Adopted {
        body: DeltaBody::MembershipUpdate(merged),
        change: Some(change),
    }
}

/// Bumps a member's version when this node is the one describing it.
///
/// Used by locally originated changes so that what this node gossips outranks
/// what it previously said.
pub(crate) fn local_successor(
    model: &mut Membership,
    current: &VersionTuple,
) -> Option<VersionTuple> {
    current.checked_successor(model.local_id().clone())
}
