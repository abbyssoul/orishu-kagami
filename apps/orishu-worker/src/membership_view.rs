//! Bounded read projections; no IO or independent membership authority.
use orishu::model::{
    cluster::SummaryView,
    node::{Inspection, MemberState, MembershipPage, NodeAccepts},
};
use orishu_membership::{Liveness, Membership, NodeId, model::Member};

pub(crate) fn inspect(model: &Membership, member: &Member) -> Inspection {
    Inspection {
        schema_version: 1,
        formation_id: model.formation().clone(),
        source_node_id: model.local_id().clone(),
        view: SummaryView::LocalAtRequest,
        node_id: member.id.clone(),
        worker_name: member.name.clone(),
        cert_fingerprint: member.cert_fingerprint,
        liveness: match member.liveness {
            Liveness::Alive => MemberState::Alive,
            Liveness::Suspected => MemberState::Suspected,
            Liveness::Dead => MemberState::Dead,
        },
        incarnation: member.incarnation,
        version: member.version.clone(),
        accepts: NodeAccepts {
            clients: member.accepts.clients,
            peers: member.accepts.peers,
            work: member.accepts.work,
        },
        peer_endpoints: member.endpoints.peers.iter().map(|a| a.0.clone()).collect(),
        client_endpoints: member
            .endpoints
            .clients
            .iter()
            .map(|a| a.0.clone())
            .collect(),
    }
}

/// O(log members + 4 records), with no scan of preceding pages or retained state.
pub(crate) fn page(model: &Membership, after: Option<NodeId>) -> MembershipPage {
    use std::ops::Bound::{Excluded, Unbounded};
    let mut records = model
        .members()
        .range((after.map_or(Unbounded, Excluded), Unbounded));
    let members: Vec<_> = records
        .by_ref()
        .take(4)
        .map(|(_, member)| inspect(model, member))
        .collect();
    let next_after = records
        .next()
        .and_then(|_| members.last().map(|m| m.node_id.clone()));
    MembershipPage {
        schema_version: 1,
        formation_id: model.formation().clone(),
        source_node_id: model.local_id().clone(),
        members,
        next_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn owner_requires_current_formation_for_continuations() {
        let (handle, owner) =
            crate::driver::spawn_standalone(orishu_membership::testing::model_with_members(10));
        let first = handle.list(None, None).unwrap().await.unwrap().unwrap();
        assert_eq!(first.members.len(), 4);
        let next = handle
            .list(Some(first.formation_id.clone()), first.next_after.clone())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert!(next.members[0].node_id > first.members.last().unwrap().node_id);
        assert!(
            handle
                .list(None, first.next_after.clone())
                .unwrap()
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            handle
                .list(
                    Some("abandoned-formation".parse().unwrap()),
                    first.next_after
                )
                .unwrap()
                .await
                .unwrap()
                .is_none()
        );
        handle.shutdown().await.unwrap();
        assert_eq!(owner.await.unwrap(), Ok(()));
    }
    #[test]
    fn pages_are_ordered_bounded_and_complete_without_duplicate_records() {
        let model = orishu_membership::testing::model_with_members(10);
        let mut ids = Vec::new();
        let mut after = None;
        loop {
            let page = page(&model, after);
            assert!(page.members.len() <= 4);
            ids.extend(page.members.into_iter().map(|m| m.node_id));
            after = page.next_after;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(ids, model.members().keys().cloned().collect::<Vec<_>>());
        assert!(
            page(&model, Some("zzzz".parse().unwrap()))
                .members
                .is_empty()
        );
    }
}
