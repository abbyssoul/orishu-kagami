//! Bounded leases over committed fields and objects. No advancing guest or run borrow
//! crosses this seam. The run's commit path never acquires an observer mutex.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct SnapshotBudget {
    held: Mutex<(usize, usize)>,
    count: usize,
    bytes: usize,
}
impl SnapshotBudget {
    pub(super) fn new(count: usize, bytes: usize) -> Self {
        Self {
            held: Mutex::new((0, 0)),
            count,
            bytes,
        }
    }
    fn reserve(self: &Arc<Self>, bytes: usize) -> wasmtime::Result<Lease> {
        let mut held = self.held.try_lock().map_err(|_| RunRejection::Limit)?;
        let count = held
            .0
            .checked_add(1)
            .filter(|n| *n <= self.count)
            .ok_or(RunRejection::Limit)?;
        let total = held
            .1
            .checked_add(bytes)
            .filter(|n| *n <= self.bytes)
            .ok_or(RunRejection::Limit)?;
        *held = (count, total);
        Ok(Lease {
            budget: self.clone(),
            bytes,
        })
    }
}
struct Lease {
    budget: Arc<SnapshotBudget>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut held = self.budget.held.lock().unwrap_or_else(|p| p.into_inner());
        held.0 -= 1;
        held.1 -= self.bytes;
    }
}
struct Sampling<'a>(&'a AtomicBool);
impl Drop for Sampling<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Complete committed object packet and optional reduced forces from one
/// boundary. Immutable buffers are shared, not copied per observer. Holding this
/// lease cannot affect advancement; callers must retain it through delivery.
/// This is a host-side lease, not a wire snapshot, checkpoint or editable state.
pub struct ObjectSnapshot {
    source: SnapshotSource,
    objects: Buffer,
    forces: Option<Buffer>,
    _lease: Lease,
}
impl ObjectSnapshot {
    pub(super) fn new(
        state: &CommittedState,
        source: SnapshotSource,
        budget: Arc<SnapshotBudget>,
    ) -> wasmtime::Result<Self> {
        let bytes = state
            .objects
            .bytes
            .len()
            .checked_add(state.forces.as_ref().map_or(0, |forces| forces.bytes.len()))
            .ok_or(RunRejection::Limit)?;
        let lease = budget.reserve(bytes)?;
        Ok(Self {
            source,
            objects: state.objects.clone(),
            forces: state.forces.clone(),
            _lease: lease,
        })
    }
    /// Workload, run descriptor, epoch, accepted boundary and SI simulation time.
    pub fn source(&self) -> &SnapshotSource {
        &self.source
    }
    /// Complete schema-tagged canonical object packet, including static objects.
    pub fn objects(&self) -> &Buffer {
        &self.objects
    }
    /// Forces that produced this boundary, evaluated at the predecessor's
    /// kinematics. None at boundary zero means uncomputed, never invented zeroes.
    pub fn forces(&self) -> Option<&Buffer> {
        self.forces.as_ref()
    }
    /// Encode the shared full-object observation into reusable caller storage.
    /// Call off the scientific executor: encoding scans/copies the retained
    /// buffers and cannot participate in commit. This projection excludes fields.
    pub fn encode(
        &self,
        output: &mut Vec<u8>,
        limits: ObjectObservationLimits,
    ) -> Result<ArtifactDigest, ObjectObservationError> {
        encode_object_observation(
            &self.source,
            &self.objects.bytes,
            self.forces.as_ref().map(|forces| forces.bytes.as_ref()),
            output,
            limits,
        )
    }
}

/// One independently retained committed field. Its lifetime is bounded by the
/// run owner's count/byte policy; only one concurrent guest query uses this lease.
/// Query failure or guest mutation cannot change the scientific run or snapshot.
pub struct FieldSnapshot {
    sandbox: Arc<Sandbox>,
    kernel: Arc<CompiledKernel>,
    context: InstanceContext,
    context_digest: ArtifactDigest,
    configuration: Buffer,
    state: Buffer,
    snapshot: SampleSnapshot,
    _lease: Lease,
    sampling: AtomicBool,
}
impl FieldSnapshot {
    pub(super) fn new(
        sandbox: Arc<Sandbox>,
        binding: &RunKernel,
        state: Buffer,
        identity: InputIdentity,
        source: SnapshotSource,
        budget: Arc<SnapshotBudget>,
    ) -> wasmtime::Result<Self> {
        let lease = budget.reserve(state.bytes.len())?;
        Ok(Self {
            sandbox,
            kernel: binding.kernel.clone(),
            context_digest: ArtifactDigest::sha256_of(&binding.context.to_cbor()?),
            context: binding.context.clone(),
            configuration: binding.configuration.clone(),
            state,
            snapshot: SampleSnapshot {
                source,
                state: identity,
            },
            _lease: lease,
            sampling: AtomicBool::new(false),
        })
    }
    /// Exact retained source and field-state identity, unchanged by later steps.
    pub fn snapshot(&self) -> &SampleSnapshot {
        &self.snapshot
    }
    /// Selected field/provider/configuration/observable metadata.
    pub fn context(&self) -> &InstanceContext {
        &self.context
    }
    /// Construct cold query metadata for this lease; the observer generates points.
    pub fn request(&self, request_id: u64, channels: Vec<SampleChannel>) -> SampleMetadata {
        SampleMetadata {
            api_version: SAMPLE_METADATA_SCHEMA.parse().expect("static schema"),
            request_id,
            snapshot: self.snapshot.clone(),
            field: self.context.instance.clone(),
            context: self.context_digest,
            channels,
        }
    }
    /// Sample this exact retained committed state in a disposable isolated guest.
    /// A stale/mismatched source or exhausted observer query fails independently.
    pub fn sample(&self, query: Buffer, control: OperationControl) -> wasmtime::Result<Buffer> {
        self.sampling
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| RunRejection::Limit)?;
        let _sampling = Sampling(&self.sampling);
        control.check()?;
        let policy = SampleLimits::default();
        let limits = SampleLimits {
            points: policy
                .points
                .min(self.context.bounds.sample_points as usize),
            channels: policy
                .channels
                .min(self.context.bounds.sample_channels as usize),
            ..policy
        };
        let request = SampleRequest::read(&query.bytes, &mut SampleScratch::default(), limits)?;
        request.check_context(&self.context)?;
        if query.schema != SAMPLE_REQUEST_SCHEMA
            || query.value_count != request.len() as u64
            || request.metadata().snapshot != self.snapshot
        {
            return Err(SampleError::Context.into());
        }
        let layout = request.layout(limits)?;
        let values = request.len() as u64;
        let result = self.sandbox.invoke_field_bound(
            &self.kernel,
            &self.context,
            self.configuration.clone(),
            FieldOperation::Sample {
                snapshot: self.state.clone(),
                query,
                output: OutputExtent {
                    schema: SAMPLE_RESPONSE_SCHEMA,
                    bytes: layout.bytes,
                    values,
                },
            },
            control,
        )?;
        match result {
            FieldResult::Samples(buffer) => Ok(buffer),
            _ => Err(RunRejection::Definition.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_lease_reserves_combined_bytes_before_sharing_and_releases_quota() {
        // Byte-budget unit fixture, not a scientific packet validation test.
        let buffer = |size| Buffer {
            schema: "fixture".into(),
            value_count: 0,
            bytes: vec![0; size].into(),
        };
        let state = CommittedState {
            boundary: 1,
            time: FiniteF64::new(0.5).unwrap(),
            objects: buffer(16),
            forces: Some(buffer(8)),
            fields: vec![],
            field_identities: vec![],
            history: buffer(0),
        };
        let source = SnapshotSource::Committed {
            workload: ArtifactDigest::sha256_of(b"workload")
                .to_string()
                .parse()
                .unwrap(),
            run: ArtifactDigest::sha256_of(b"descriptor"),
            epoch: 1,
            boundary: 1,
            time_seconds: state.time,
        };
        let too_small = Arc::new(SnapshotBudget::new(8, 23));
        assert!(ObjectSnapshot::new(&state, source.clone(), too_small.clone()).is_err());
        assert_eq!(*too_small.held.lock().unwrap(), (0, 0));
        let budget = Arc::new(SnapshotBudget::new(2, 24));
        let snapshot = ObjectSnapshot::new(&state, source.clone(), budget.clone()).unwrap();
        assert_eq!(*budget.held.lock().unwrap(), (1, 24));
        assert!(Arc::ptr_eq(&state.objects.bytes, &snapshot.objects().bytes));
        assert!(ObjectSnapshot::new(&state, source.clone(), budget.clone()).is_err());
        drop(snapshot);
        assert_eq!(*budget.held.lock().unwrap(), (0, 0));
        let guard = budget.held.lock().unwrap();
        assert!(
            ObjectSnapshot::new(&state, source.clone(), budget.clone()).is_err(),
            "acquisition never waits on another observer"
        );
        drop(guard);
        let snapshot = ObjectSnapshot::new(&state, source.clone(), budget.clone()).unwrap();
        assert_eq!(snapshot.source(), &source);
        drop(snapshot);
        assert_eq!(*budget.held.lock().unwrap(), (0, 0));
    }
}
