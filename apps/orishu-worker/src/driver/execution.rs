//! Owner-issued single-node scientific-admission reservation.
//!
//! This is a worker lifetime fence, not scientific validation or a run commit.
//! Heavy admission runs outside the membership owner. Final adoption/publication
//! must return to an owner that checks this fence; reading `is_current` alone is
//! not an atomic commit protocol. No workload route/capability is enabled here.
use super::*;
use orishu::model::run::{RunDescriptor, RunIdentity, WorkloadEpoch};
use orishu_runtime::Cancellation;

/// One owner-issued immutable descriptor and the matching shared runtime scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunAllocation {
    pub(super) descriptor: RunDescriptor,
    pub(super) scope: orishu_runtime::RunScope,
}

/// Admission reservation refused without changing scientific or formation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionError {
    /// The caller observed a different process-local formation generation.
    #[error("execution reservation names a stale formation")]
    Stale,
    /// Initial execution requires a locked, standalone, single-member formation
    /// without an outstanding join. Distributed execution has a different gate.
    #[error("worker is not eligible for single-node scientific admission")]
    Unavailable,
    /// One pending admission or retained execution lease already owns the slot.
    #[error("worker already has an execution reservation")]
    Busy,
    /// A non-reusable reservation or workload-epoch counter was exhausted.
    #[error("worker execution identity exhausted")]
    Exhausted,
    /// A descriptor could not be encoded under the fixed version/budget.
    #[error("worker run descriptor is invalid")]
    InvalidRun,
}

/// Read-only liveness/identity of an owner-issued reservation. Cloning a fence
/// does not retain its lease and cannot keep work alive after release/shutdown.
#[derive(Debug, Clone)]
pub struct ExecutionFence {
    generation: Generation,
    formation: orishu_membership::FormationId,
    node: NodeId,
    serial: u64,
    current: Cancellation,
}
impl ExecutionFence {
    /// A cheap cancellation hint, not authority to publish a result.
    pub fn is_current(&self) -> bool {
        !self.current.is_cancelled()
    }
    /// Monotonic revocation linked into guest operation control. Callers may
    /// cancel work but cannot restore validity or mint another reservation.
    pub fn cancellation(&self) -> Cancellation {
        self.current.clone()
    }
    /// Formation that issued the reservation, never a mutable display label.
    pub fn formation(&self) -> &orishu_membership::FormationId {
        &self.formation
    }
    /// Node identity at reservation time.
    pub fn node(&self) -> &NodeId {
        &self.node
    }
    /// Process-local generation, fenced across formation replacement/ejection.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// Monotonically allocated process-local sequence, not a workload/run digest.
    pub fn serial(&self) -> u64 {
        self.serial
    }
}

/// Exclusive reservation retained through admission and eventual unload.
/// Drop revokes all cloned fences immediately and reliably releases the owner
/// slot using a pre-reserved control permit, even if normal ingress is full.
pub struct ExecutionLease {
    fence: ExecutionFence,
    completion: Option<mpsc::OwnedPermit<Control>>,
}
impl ExecutionLease {
    /// Clone the read-only fence for off-owner work and cancellation checks.
    pub fn fence(&self) -> ExecutionFence {
        self.fence.clone()
    }
}
impl std::fmt::Debug for ExecutionLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionLease")
            .field("fence", &self.fence)
            .finish_non_exhaustive()
    }
}
impl Drop for ExecutionLease {
    fn drop(&mut self) {
        self.fence.current.cancel();
        if let Some(permit) = self.completion.take() {
            send_reserved_control(
                permit,
                Control::ReleaseExecution {
                    generation: self.fence.generation,
                    serial: self.fence.serial,
                },
            );
        }
    }
}

struct Active(ExecutionFence);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.current.cancel();
    }
}
#[derive(Default)]
pub(super) struct Slot {
    next: u64,
    active: Option<Active>,
    confirmed: bool,
    run: Option<super::run::RunView>,
    next_epoch: u64,
    allocation: Option<RunAllocation>,
}
impl Slot {
    pub(super) fn occupied(&self) -> bool {
        self.active.is_some()
    }
    pub(super) fn revoke(&mut self) {
        if let Some(active) = &self.active {
            active.0.current.cancel();
        }
        self.run = None;
    }
    pub(super) fn release(&mut self, generation: Generation, serial: u64) {
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.0.generation == generation && a.0.serial == serial)
        {
            self.active = None;
            self.confirmed = false;
            self.run = None;
            self.allocation = None;
        }
    }
    pub(super) fn run_view(&self) -> Option<super::run::RunView> {
        self.active.as_ref().filter(|a| a.0.is_current())?;
        self.run.clone()
    }
    fn allocate(
        &mut self,
        formation: orishu_membership::FormationId,
        workload: orishu_workload::WorkloadDigest,
    ) -> Result<RunAllocation, ExecutionError> {
        if let Some(allocation) = &self.allocation {
            if allocation.descriptor.identity().formation_id() != &formation
                || allocation.scope.workload != workload
            {
                return Err(ExecutionError::Stale);
            }
            return Ok(allocation.clone());
        }
        let epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(ExecutionError::Exhausted)?;
        let descriptor = RunDescriptor::new(RunIdentity::new(
            formation,
            workload,
            WorkloadEpoch::new(epoch),
        ));
        let allocation = RunAllocation {
            scope: orishu_runtime::RunScope {
                workload,
                run: descriptor
                    .digest()
                    .map_err(|_| ExecutionError::InvalidRun)?,
                epoch,
            },
            descriptor,
        };
        self.next_epoch = epoch;
        self.allocation = Some(allocation.clone());
        Ok(allocation)
    }
}

impl Handle {
    /// Reserve scientific admission in the serialized formation owner. This
    /// internal adapter API assumes caller authorization; no HTTP route uses it
    /// yet. Reserve before reading large input or starting compilation. Two
    /// control slots are needed initially; one remains reserved for lease drop.
    pub fn reserve_execution(
        &self,
        generation: Generation,
        formation: orishu_membership::FormationId,
    ) -> Result<oneshot::Receiver<Result<ExecutionLease, ExecutionError>>, DriverError> {
        let completion = self
            .control
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        let (reply, receive) = oneshot::channel();
        self.control
            .try_send(Control::ReserveExecution {
                generation,
                formation,
                completion,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DriverError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
            })?;
        Ok(receive)
    }
}

impl Owner {
    fn execution_eligible(&self) -> bool {
        let model = self.model.as_ref().expect("installed model");
        self.view.summary.participation == Participation::Standalone
            && model.members().len() == 1
            && model.membership_locked()
            && model.join_attempt().is_none()
            && !self.join_operations.has_active()
            && self.outbound_join.is_none()
    }
    pub(super) fn reserve_execution(
        &mut self,
        generation: Generation,
        formation: orishu_membership::FormationId,
        completion: mpsc::OwnedPermit<Control>,
    ) -> Result<ExecutionLease, ExecutionError> {
        if generation != self.view.generation || formation != self.view.summary.formation_id {
            return Err(ExecutionError::Stale);
        }
        if self.execution.occupied() {
            return Err(ExecutionError::Busy);
        }
        if !self.execution_eligible() {
            return Err(ExecutionError::Unavailable);
        }
        let serial = self
            .execution
            .next
            .checked_add(1)
            .ok_or(ExecutionError::Exhausted)?;
        self.execution.next = serial;
        let fence = ExecutionFence {
            generation,
            formation,
            node: self.view.summary.source_node_id.clone(),
            serial,
            current: Cancellation::default(),
        };
        self.execution.active = Some(Active(fence.clone()));
        Ok(ExecutionLease {
            fence,
            completion: Some(completion),
        })
    }
    /// Revoke if any trusted internal/core transition bypassed an outer adapter
    /// guard. Future scientific publication must additionally recheck the fence
    /// inside the publication owner's serialized transition.
    pub(super) fn reconcile_execution(&mut self) {
        if self.execution.occupied() && !self.execution_eligible() {
            self.execution.revoke();
        }
    }
    pub(super) fn confirm_admission(
        &mut self,
        fence: &ExecutionFence,
        scope: &orishu_runtime::RunScope,
    ) -> Result<(), ExecutionError> {
        self.check_execution_fence(fence)?;
        if self.execution.allocation.as_ref().map(|a| &a.scope) != Some(scope) {
            return Err(ExecutionError::Stale);
        }
        if self.execution.confirmed {
            return Err(ExecutionError::Busy);
        }
        self.execution.confirmed = true;
        Ok(())
    }
    pub(super) fn allocate_run(
        &mut self,
        fence: &ExecutionFence,
        workload: orishu_workload::WorkloadDigest,
    ) -> Result<RunAllocation, ExecutionError> {
        self.check_execution_fence(fence)?;
        self.execution.allocate(fence.formation.clone(), workload)
    }
    fn check_execution_fence(&self, fence: &ExecutionFence) -> Result<(), ExecutionError> {
        if !self.execution_eligible() || !fence.is_current() {
            return Err(ExecutionError::Unavailable);
        }
        let Some(active) = &self.execution.active else {
            return Err(ExecutionError::Stale);
        };
        if active.0.generation != fence.generation
            || active.0.serial != fence.serial
            || self.view.generation != fence.generation
            || self.view.summary.formation_id != fence.formation
            || self.view.summary.source_node_id != fence.node
        {
            return Err(ExecutionError::Stale);
        }
        Ok(())
    }
    pub(super) fn publish_scientific(&mut self) {
        self.view.scientific = self.execution.run_view();
        self.published.send_replace(self.view.clone());
    }
    pub(super) fn publish_run_boundary(
        &mut self,
        fence: &ExecutionFence,
        next: super::run::RunView,
    ) -> Result<(), ExecutionError> {
        self.check_execution_fence(fence)?;
        if !self.execution.confirmed
            || self.execution.allocation.as_ref().map(|a| &a.scope) != Some(next.scope())
        {
            return Err(ExecutionError::Stale);
        }
        match &self.execution.run {
            None if next.boundary() == 0 && next.time_seconds().get() == 0.0 && !next.stopped() => {
            }
            Some(previous) if previous.scope() == next.scope() && !previous.stopped() => {
                let stop = next.stopped()
                    && next.boundary() == previous.boundary()
                    && next.time_seconds() == previous.time_seconds();
                let step = !next.stopped()
                    && previous.boundary().checked_add(1) == Some(next.boundary())
                    && next.time_seconds().get() > previous.time_seconds().get();
                if !stop && !step {
                    return Err(ExecutionError::Stale);
                }
            }
            _ => return Err(ExecutionError::Stale),
        }
        self.execution.run = Some(next);
        self.publish_scientific();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orishu_membership::testing;

    fn root(byte: &str) -> orishu_workload::WorkloadDigest {
        format!("sha256:{}", byte.repeat(32)).parse().unwrap()
    }
    async fn allocate(
        handle: &Handle,
        fence: ExecutionFence,
        workload: orishu_workload::WorkloadDigest,
    ) -> Result<RunAllocation, ExecutionError> {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        handle
            .control
            .try_send(Control::AllocateRun {
                fence,
                workload,
                control: orishu_runtime::OperationControl::new(Duration::from_secs(5)).unwrap(),
                reply,
            })
            .unwrap();
        tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_secs(2)).unwrap())
            .await
            .unwrap()
    }
    async fn confirm(
        handle: &Handle,
        fence: ExecutionFence,
        scope: orishu_runtime::RunScope,
    ) -> Result<(), ExecutionError> {
        let (reply, receive) = oneshot::channel();
        handle
            .control
            .try_send(Control::ConfirmScientificAdmission {
                fence,
                scope,
                control: orishu_runtime::OperationControl::new(Duration::from_secs(5)).unwrap(),
                reply,
            })
            .unwrap();
        receive.await.unwrap()
    }
    #[tokio::test]
    async fn run_allocation_is_lease_bound_idempotent_and_never_reused_after_release() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let first = allocate(&handle, lease.fence(), root("00")).await.unwrap();
        assert_eq!(first.scope.epoch, 1);
        assert_eq!(first.scope.run, first.descriptor.digest().unwrap());
        assert_eq!(
            first.descriptor.identity().formation_id(),
            lease.fence().formation()
        );
        assert_eq!(
            allocate(&handle, lease.fence(), root("00")).await.unwrap(),
            first
        );
        assert_eq!(
            allocate(&handle, lease.fence(), root("01")).await,
            Err(ExecutionError::Stale)
        );
        let mut bad = lease.fence();
        bad.serial += 1;
        assert_eq!(
            allocate(&handle, bad, root("00")).await,
            Err(ExecutionError::Stale)
        );
        for changed in [
            orishu_runtime::RunScope {
                epoch: 2,
                ..first.scope.clone()
            },
            orishu_runtime::RunScope {
                workload: root("01"),
                ..first.scope.clone()
            },
            orishu_runtime::RunScope {
                run: orishu_workload::ArtifactDigest::sha256_of(b"another run"),
                ..first.scope.clone()
            },
        ] {
            assert_eq!(
                confirm(&handle, lease.fence(), changed).await,
                Err(ExecutionError::Stale)
            );
        }
        assert_eq!(
            confirm(&handle, lease.fence(), first.scope.clone()).await,
            Ok(())
        );
        drop(lease);
        handle.model_snapshot().await;
        // A reservation abandoned before allocation does not issue an epoch.
        drop(reserve(&handle).await.unwrap());
        handle.model_snapshot().await;
        let next = reserve(&handle).await.unwrap();
        assert_eq!(next.fence().serial(), 3);
        let second = allocate(&handle, next.fence(), root("00")).await.unwrap();
        assert_eq!(second.scope.epoch, 2);
        assert_ne!(second.scope.run, first.scope.run);
        drop(next);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn lost_allocation_reply_and_full_mailbox_do_not_lose_or_reissue_the_epoch() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let permit = handle.control.clone().try_reserve_owned().unwrap();
        let mut held = vec![];
        while let Ok(p) = handle.control.clone().try_reserve_owned() {
            held.push(p);
        }
        assert_eq!(held.len(), CONTROL_CAPACITY - 2);
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        drop(receive);
        send_reserved_control(
            permit,
            Control::AllocateRun {
                fence: lease.fence(),
                workload: root("00"),
                control: orishu_runtime::OperationControl::new(Duration::from_secs(5)).unwrap(),
                reply,
            },
        );
        drop(held);
        handle.model_snapshot().await;
        assert_eq!(
            allocate(&handle, lease.fence(), root("00"))
                .await
                .unwrap()
                .scope
                .epoch,
            1
        );
        let old = lease.fence();
        drop(lease);
        handle.model_snapshot().await;
        let next = reserve(&handle).await.unwrap();
        assert!(allocate(&handle, old, root("00")).await.is_err());
        assert_eq!(
            allocate(&handle, next.fence(), root("00"))
                .await
                .unwrap()
                .scope
                .epoch,
            2
        );
        drop(next);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[test]
    fn epoch_exhaustion_refuses_without_wrapping_or_rebinding() {
        let mut slot = Slot {
            next_epoch: u64::MAX - 1,
            ..Slot::default()
        };
        let formation = testing::model_with_members(0).formation().clone();
        let final_run = slot.allocate(formation.clone(), root("00")).unwrap();
        assert_eq!(final_run.scope.epoch, u64::MAX);
        assert_eq!(
            slot.allocate(formation.clone(), root("00")).unwrap(),
            final_run
        );
        slot.allocation = None;
        assert_eq!(
            slot.allocate(formation, root("00")),
            Err(ExecutionError::Exhausted)
        );
        assert!(slot.allocation.is_none());
        assert_eq!(slot.next_epoch, u64::MAX);
    }

    async fn reserve(handle: &Handle) -> Result<ExecutionLease, ExecutionError> {
        let view = handle.view().unwrap();
        handle
            .reserve_execution(view.generation, view.summary.formation_id)
            .unwrap()
            .await
            .unwrap()
    }
    async fn lock(handle: &Handle) {
        let view = handle.view().unwrap();
        handle
            .set_membership_lock(view.generation, view.summary.formation_id, true)
            .unwrap()
            .await
            .unwrap()
            .unwrap();
    }
    fn join_request(handle: &Handle) -> orishu::model::cluster::JoinRequest {
        use orishu::model::cluster::{JoinMaterial, JoinRequest};
        JoinRequest {
            schema_version: 1,
            operation_id: "execution-race".parse().unwrap(),
            formation_id: handle.view().unwrap().summary.formation_id,
            material: JoinMaterial {
                schema_version: 1,
                formation_id: "other-formation".parse().unwrap(),
                introducer_node_id: "introducer".parse().unwrap(),
                introducer_fingerprint: testing::fingerprint(91),
                peer_endpoints: vec!["127.0.0.1:6655".into()],
                introducer_ready: true,
                token: "a".repeat(64).try_into().unwrap(),
            },
        }
    }

    #[tokio::test]
    async fn lease_is_exclusive_identity_fenced_and_released_without_a_scheduler() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        assert_eq!(
            reserve(&handle).await.unwrap_err(),
            ExecutionError::Unavailable
        );
        lock(&handle).await;
        let view = handle.view().unwrap();
        for (generation, formation) in [
            (
                Generation(view.generation.0 + 1),
                view.summary.formation_id.clone(),
            ),
            (view.generation, "another".parse().unwrap()),
        ] {
            assert_eq!(
                handle
                    .reserve_execution(generation, formation)
                    .unwrap()
                    .await
                    .unwrap()
                    .unwrap_err(),
                ExecutionError::Stale
            );
        }
        let first = reserve(&handle).await.unwrap();
        let fence = first.fence();
        assert!(fence.is_current());
        assert_eq!(fence.formation(), &view.summary.formation_id);
        assert_eq!(fence.node(), &view.summary.source_node_id);
        assert_eq!(reserve(&handle).await.unwrap_err(), ExecutionError::Busy);
        assert!(matches!(
            handle
                .prepare_join(join_request(&handle))
                .unwrap()
                .await
                .unwrap(),
            Err(crate::join_operations::OperationError::Busy)
        ));
        assert_eq!(
            handle
                .set_membership_lock(view.generation, view.summary.formation_id.clone(), false)
                .unwrap()
                .await
                .unwrap()
                .unwrap_err(),
            LockError::Unavailable
        );
        assert!(matches!(
            handle
                .leave_operation(LeaveRequest {
                    schema_version: 1,
                    operation_id: "while-reserved".parse().unwrap(),
                    formation_id: view.summary.formation_id,
                })
                .unwrap()
                .await
                .unwrap(),
            Err(LeaveError::Unavailable)
        ));
        drop(first);
        assert!(!fence.is_current());
        let second = reserve(&handle).await.unwrap();
        assert!(second.fence().serial() > fence.serial());
        handle
            .control
            .try_send(Control::ReleaseExecution {
                generation: fence.generation(),
                serial: fence.serial(),
            })
            .unwrap();
        assert_eq!(reserve(&handle).await.unwrap_err(), ExecutionError::Busy);
        assert!(
            second.fence().is_current(),
            "late release cannot release a newer use"
        );
        drop(second);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn pending_join_and_multiple_members_refuse_execution() {
        let (handle, task) = spawn_standalone(testing::model_with_members(1));
        lock(&handle).await;
        assert_eq!(
            reserve(&handle).await.unwrap_err(),
            ExecutionError::Unavailable
        );
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();

        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let PreparedJoin::Start(join) = handle
            .prepare_join(join_request(&handle))
            .unwrap()
            .await
            .unwrap()
            .unwrap()
        else {
            panic!("new pending join")
        };
        assert_eq!(
            handle.view().unwrap().summary.participation,
            Participation::Standalone
        );
        assert_eq!(
            reserve(&handle).await.unwrap_err(),
            ExecutionError::Unavailable,
            "pending IO must fence admission before participation changes"
        );
        drop(join);
        let lease = reserve(&handle).await.unwrap();
        drop(lease);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn abandoned_receiver_releases_instead_of_stranding_admission() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let view = handle.view().unwrap();
        drop(
            handle
                .reserve_execution(view.generation, view.summary.formation_id)
                .unwrap(),
        );
        // First owner turn issues/drops the reply and queues its reliable release.
        // A second barrier establishes that the queued release was consumed.
        handle.model_snapshot().await;
        handle.model_snapshot().await;
        let lease = reserve(&handle).await.unwrap();
        drop(lease);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn lease_drop_uses_reserved_capacity_even_when_ingress_is_full() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let fence = lease.fence();
        let mut held = Vec::new();
        while let Ok(permit) = handle.control.clone().try_reserve_owned() {
            held.push(permit);
        }
        assert_eq!(held.len(), CONTROL_CAPACITY - 1);
        let view = handle.view().unwrap();
        assert!(matches!(
            handle.reserve_execution(view.generation, view.summary.formation_id),
            Err(DriverError::Overloaded)
        ));
        assert_eq!(handle.control.capacity(), 0);
        drop(lease);
        assert!(!fence.is_current());
        tokio::time::timeout(Duration::from_secs(2), handle.model_snapshot())
            .await
            .unwrap();
        drop(held);
        let next = reserve(&handle).await.unwrap();
        drop(next);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn shutdown_revokes_before_acknowledgement_even_while_owner_cleanup_is_held() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let fence = lease.fence();
        let (release, held) = oneshot::channel();
        handle.hold_shutdown(held).await;
        handle.shutdown().await.unwrap();
        assert!(!fence.is_current());
        assert!(!task.is_finished());
        assert_eq!(
            handle.view().unwrap().summary.participation,
            Participation::Stopping
        );
        release.send(()).unwrap();
        task.await.unwrap().unwrap();
        drop(lease); // A held completion permit must not leak after receiver death.
    }

    #[tokio::test]
    async fn owner_abort_and_internal_topology_change_revoke_retained_fences() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let fence = lease.fence();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!fence.is_current());
        drop(lease);

        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let fence = lease.fence();
        handle
            .try_submit(
                fence.generation(),
                Message::Local(Command::SetMembershipLock(false)),
            )
            .unwrap();
        handle.model_snapshot().await;
        assert!(!fence.is_current());
        lock(&handle).await;
        assert_eq!(
            reserve(&handle).await.unwrap_err(),
            ExecutionError::Busy,
            "revocation does not release memory/IO ownership of abandoned work"
        );
        drop(lease);
        let next = reserve(&handle).await.unwrap();
        assert!(next.fence().is_current());
        drop(next);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn formation_replacement_revokes_before_new_identity_can_be_used() {
        let (handle, task) = spawn_standalone(testing::model_with_members(0));
        lock(&handle).await;
        let lease = reserve(&handle).await.unwrap();
        let fence = lease.fence();
        handle
            .try_submit(
                fence.generation(),
                Message::Local(Command::Leave {
                    replacement_formation: "replacement".parse().unwrap(),
                    replacement_node_id: "replacement-node".parse().unwrap(),
                    replacement_cluster_name: "replacement-cluster".parse().unwrap(),
                }),
            )
            .unwrap();
        handle.model_snapshot().await;
        assert!(!fence.is_current());
        assert_ne!(handle.view().unwrap().generation, fence.generation());
        assert_eq!(
            handle
                .reserve_execution(fence.generation(), fence.formation().clone())
                .unwrap()
                .await
                .unwrap()
                .unwrap_err(),
            ExecutionError::Stale
        );
        lock(&handle).await;
        assert_eq!(reserve(&handle).await.unwrap_err(), ExecutionError::Busy);
        drop(lease);
        let next = reserve(&handle).await.unwrap();
        assert_eq!(next.fence().formation().as_str(), "replacement");
        assert!(next.fence().serial() > fence.serial());
        drop(next);
        handle.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
    }
}
