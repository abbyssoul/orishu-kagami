//! Owner-local bounded retention of immutable public baselines. Current session
//! and source-network authorization must precede every call. No token is stored
//! or returned here, and a finish acknowledgement cannot establish readiness.

use super::{Descriptor, Frozen};
use crate::driver::Generation;
use orishu::model::cluster::OperationId;
use orishu_membership::{CertFingerprint, Membership, NodeId, PeerContext, SenderIdentity};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const MAX_RETAINED: usize = 4;
const LIFETIME: Duration = Duration::from_secs(30);

/// Non-payload-bearing transfer refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Caller is not a currently admitted member in this formation.
    #[error("admission baseline access denied")]
    Unauthorized,
    /// Baseline expired, was fenced, or is not owned by this requester.
    #[error("admission baseline unavailable")]
    Unavailable,
    /// Fixed live-transfer capacity is exhausted; nothing was evicted.
    #[error("admission baseline capacity exhausted")]
    Overloaded,
    /// The requested page would skip data or the completion digest is wrong.
    #[error("invalid admission baseline continuation")]
    Invalid,
    /// Complete source state cannot be represented within the wire profile.
    #[error(transparent)]
    Baseline(#[from] super::Error),
}

struct Entry {
    request: OperationId,
    requester: NodeId,
    fingerprint: CertFingerprint,
    generation: Generation,
    expires: Instant,
    served: u16,
    frozen: Frozen,
}

/// Four non-evicting, 30-second leases. Snapshot IDs increase for this store's
/// process lifetime, including across generation sweeps, and never wrap.
#[derive(Default)]
pub struct Store {
    next: u64,
    entries: BTreeMap<u64, Entry>,
}

fn requester<'a>(model: &Membership, peer: &'a PeerContext) -> Result<&'a NodeId, Error> {
    if !peer.authenticated || &peer.formation != model.formation() {
        return Err(Error::Unauthorized);
    }
    let SenderIdentity::Admitted(node) = &peer.sender else {
        return Err(Error::Unauthorized);
    };
    crate::peer::session::validate_member(model, node, peer.cert_fingerprint)
        .map_err(|_| Error::Unauthorized)?;
    Ok(node)
}

impl Store {
    /// Bound work to four leases, retiring expired/old-generation entries and
    /// requesters whose admitted identity or exclusion policy is no longer valid.
    pub fn sweep(&mut self, model: &Membership, generation: Generation, now: Instant) {
        self.entries.retain(|_, entry| {
            entry.generation == generation
                && now < entry.expires
                && &entry.frozen.descriptor().formation == model.formation()
                && &entry.frozen.descriptor().source == model.local_id()
                && crate::peer::session::validate_member(model, &entry.requester, entry.fingerprint)
                    .is_ok()
        });
    }

    /// Capture once for an identified request. Exact live retries return the
    /// same bytes and deadline, even if source policy has since advanced. The
    /// caller must additionally enforce its own source catch-up/token readiness.
    pub fn begin(
        &mut self,
        model: &Membership,
        generation: Generation,
        peer: &PeerContext,
        request: OperationId,
        now: Instant,
    ) -> Result<Descriptor, Error> {
        self.sweep(model, generation, now);
        let node = requester(model, peer)?;
        if let Some(entry) = self
            .entries
            .values()
            .find(|entry| &entry.requester == node && entry.request == request)
        {
            return Ok(entry.frozen.descriptor().clone());
        }
        if self.entries.len() == MAX_RETAINED {
            return Err(Error::Overloaded);
        }
        let snapshot = self.next.checked_add(1).ok_or(Error::Overloaded)?;
        let expires = now.checked_add(LIFETIME).ok_or(Error::Overloaded)?;
        let frozen = Frozen::capture(model, snapshot)?;
        let descriptor = frozen.descriptor().clone();
        self.next = snapshot;
        self.entries.insert(
            snapshot,
            Entry {
                request,
                requester: node.clone(),
                fingerprint: peer.cert_fingerprint,
                generation,
                expires,
                served: 0,
                frozen,
            },
        );
        Ok(descriptor)
    }

    fn entry(
        &mut self,
        model: &Membership,
        generation: Generation,
        peer: &PeerContext,
        snapshot: u64,
        now: Instant,
    ) -> Result<&mut Entry, Error> {
        self.sweep(model, generation, now);
        let node = requester(model, peer)?;
        self.entries
            .get_mut(&snapshot)
            .filter(|entry| &entry.requester == node)
            .ok_or(Error::Unavailable)
    }

    /// Return the next page or a previously served page for lost-response retry.
    /// Skips/out-of-range indexes fail without advancing the completion barrier.
    pub fn page(
        &mut self,
        model: &Membership,
        generation: Generation,
        peer: &PeerContext,
        snapshot: u64,
        index: u16,
        now: Instant,
    ) -> Result<&[u8], Error> {
        let entry = self.entry(model, generation, peer, snapshot, now)?;
        if index > entry.served || index >= entry.frozen.descriptor().pages {
            return Err(Error::Invalid);
        }
        if index == entry.served {
            entry.served += 1;
        }
        entry.frozen.page(index).ok_or(Error::Invalid)
    }

    /// Check the requester's acknowledgement after all pages were issued. This
    /// does not prove remote installation or authorize release by itself: the
    /// owner must recheck readiness/session and the receiver must verify content.
    pub fn confirm(
        &mut self,
        model: &Membership,
        generation: Generation,
        peer: &PeerContext,
        snapshot: u64,
        root: [u8; 32],
        now: Instant,
    ) -> Result<(), Error> {
        let entry = self.entry(model, generation, peer, snapshot, now)?;
        if entry.served != entry.frozen.descriptor().pages || root != entry.frozen.descriptor().root
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::{Command, Message, SessionId, testing, update};

    fn peer(model: &Membership, index: usize) -> PeerContext {
        let node: NodeId = format!("node-{index:04}").parse().unwrap();
        PeerContext {
            protocol: orishu_membership::ProtocolVersion::CURRENT,
            session: SessionId(1),
            authenticated: true,
            cert_fingerprint: model.member(&node).unwrap().cert_fingerprint,
            sender: SenderIdentity::Admitted(node),
            formation: model.formation().clone(),
            seq: 1,
            gossip: vec![],
        }
    }

    #[test]
    fn completion_requires_every_page_and_current_identity_authority() {
        let mut model = testing::model_with_members(1);
        for index in 0..40 {
            model = update(
                model,
                Message::Local(Command::UpdateBlocklist(testing::blocklist_entry(
                    &format!("blocked-{index:02}"),
                ))),
            )
            .model;
        }
        let caller = peer(&model, 0);
        let now = Instant::now();
        let mut store = Store::default();
        let descriptor = store
            .begin(
                &model,
                Generation(0),
                &caller,
                "pages".parse().unwrap(),
                now,
            )
            .unwrap();
        assert_eq!(descriptor.pages, 2);
        assert_eq!(
            store.page(&model, Generation(0), &caller, descriptor.snapshot, 1, now),
            Err(Error::Invalid)
        );
        store
            .page(&model, Generation(0), &caller, descriptor.snapshot, 0, now)
            .unwrap();
        assert_eq!(
            store.confirm(
                &model,
                Generation(0),
                &caller,
                descriptor.snapshot,
                descriptor.root,
                now
            ),
            Err(Error::Invalid)
        );
        store
            .page(&model, Generation(0), &caller, descriptor.snapshot, 1, now)
            .unwrap();
        assert_eq!(
            store.confirm(
                &model,
                Generation(0),
                &caller,
                descriptor.snapshot,
                [0; 32],
                now
            ),
            Err(Error::Invalid)
        );
        store
            .confirm(
                &model,
                Generation(0),
                &caller,
                descriptor.snapshot,
                descriptor.root,
                now,
            )
            .unwrap();
        let mut block = testing::blocklist_entry("unused");
        block.key = orishu_membership::model::BlocklistKey::Fingerprint(caller.cert_fingerprint);
        model = update(model, Message::Local(Command::UpdateBlocklist(block))).model;
        assert_eq!(
            store.confirm(
                &model,
                Generation(0),
                &caller,
                descriptor.snapshot,
                descriptor.root,
                now
            ),
            Err(Error::Unauthorized)
        );
        assert!(store.entries.is_empty());
    }

    #[test]
    fn retained_retry_is_immutable_and_expiry_does_not_reuse_snapshot_identity() {
        let mut store = Store::default();
        let model = testing::model_with_members(2);
        let caller = peer(&model, 0);
        let now = Instant::now();
        let request: OperationId = "baseline".parse().unwrap();
        let descriptor = store
            .begin(&model, Generation(0), &caller, request.clone(), now)
            .unwrap();
        assert_eq!(
            store.confirm(
                &model,
                Generation(0),
                &caller,
                descriptor.snapshot,
                descriptor.root,
                now
            ),
            Err(Error::Invalid)
        );
        let bytes = store
            .page(&model, Generation(0), &caller, descriptor.snapshot, 0, now)
            .unwrap()
            .to_vec();
        let changed = update(model, Message::Local(Command::SetMembershipLock(true))).model;
        assert_eq!(
            store
                .begin(&changed, Generation(0), &caller, request.clone(), now)
                .unwrap(),
            descriptor
        );
        assert_eq!(
            store
                .page(
                    &changed,
                    Generation(0),
                    &caller,
                    descriptor.snapshot,
                    0,
                    now
                )
                .unwrap(),
            bytes
        );
        store
            .confirm(
                &changed,
                Generation(0),
                &caller,
                descriptor.snapshot,
                descriptor.root,
                now,
            )
            .unwrap();
        let later = now + LIFETIME;
        assert_eq!(
            store.page(
                &changed,
                Generation(0),
                &caller,
                descriptor.snapshot,
                0,
                later
            ),
            Err(Error::Unavailable)
        );
        let next = store
            .begin(&changed, Generation(0), &caller, request, later)
            .unwrap();
        assert!(next.snapshot > descriptor.snapshot);
        assert_ne!(next.root, descriptor.root);
    }

    #[test]
    fn live_leases_are_not_evicted_and_continuations_are_requester_and_generation_bound() {
        let model = testing::model_with_members(2);
        let a = peer(&model, 0);
        let b = peer(&model, 1);
        let now = Instant::now();
        let mut store = Store::default();
        let mut descriptors = Vec::new();
        for index in 0..MAX_RETAINED {
            descriptors.push(
                store
                    .begin(
                        &model,
                        Generation(0),
                        &a,
                        format!("request-{index}").parse().unwrap(),
                        now,
                    )
                    .unwrap(),
            );
        }
        assert_eq!(
            store.begin(&model, Generation(0), &b, "extra".parse().unwrap(), now),
            Err(Error::Overloaded)
        );
        let descriptor = &descriptors[0];
        assert_eq!(
            store.page(&model, Generation(0), &b, descriptor.snapshot, 0, now),
            Err(Error::Unavailable)
        );
        assert!(
            store
                .page(&model, Generation(0), &a, descriptor.snapshot, 0, now)
                .is_ok()
        );
        assert_eq!(
            store.page(&model, Generation(1), &a, descriptor.snapshot, 0, now),
            Err(Error::Unavailable)
        );
        let mut invalid = a;
        invalid.authenticated = false;
        assert_eq!(
            store.begin(
                &model,
                Generation(1),
                &invalid,
                "invalid".parse().unwrap(),
                now
            ),
            Err(Error::Unauthorized)
        );
    }
}
