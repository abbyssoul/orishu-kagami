use crate::field::orishu::simulation::buffers::{self, AccessError, Descriptor};
use std::sync::Arc;
use wasmtime::{
    StoreLimits, StoreLimitsBuilder,
    component::{Resource, ResourceTable},
};

/// Owned immutable buffer with an explicit portable schema and logical count.
#[derive(Clone, Debug)]
pub struct Buffer {
    /// Versioned wire/storage schema, not a Rust memory layout.
    pub schema: String,
    /// Number of schema-defined logical values represented by the bytes.
    pub value_count: u64,
    /// Committed bytes; observers and inputs may share them but never write them.
    pub bytes: Arc<[u8]>,
}

/// A mismatch between a host buffer descriptor and its standard scientific packet.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum ScientificBufferError {
    /// Descriptor names a different or unsupported standard schema.
    #[error("scientific buffer schema mismatch")]
    Schema,
    /// Descriptor's logical count differs from the fully validated packet count.
    #[error("scientific buffer value count mismatch")]
    Count,
    /// The packet itself failed bounded validation.
    #[error(transparent)]
    Packet(#[from] orishu_plugin::execution::BulkError),
}
impl Buffer {
    /// Validate both descriptor and standard packet, returning a borrowed view.
    /// Exact workload/run/boundary/provider binding remains a supervisor obligation.
    pub fn scientific_batch<R: orishu_plugin::execution::BulkRecord>(
        &self,
        limits: orishu_plugin::execution::BulkLimits,
    ) -> Result<orishu_plugin::execution::Batch<'_, R>, ScientificBufferError> {
        if self.schema != R::SCHEMA {
            return Err(ScientificBufferError::Schema);
        }
        let batch = orishu_plugin::execution::Batch::read(&self.bytes, limits)?;
        if self.value_count != batch.len() as u64 {
            return Err(ScientificBufferError::Count);
        }
        Ok(batch)
    }
}
/// Caller-owned bounds on resource grants and aggregate host-call work.
#[derive(Clone, Copy, Debug)]
pub struct GrantLimits {
    /// Maximum input plus output resources for one invocation.
    pub resources: usize,
    /// Maximum one read or logical write prefix (physical writes are 64 KiB).
    pub chunk_bytes: usize,
    /// Maximum aggregate input plus reserved candidate bytes.
    pub bytes: usize,
    /// Aggregate physical transferred bytes (repeated reads and write padding count).
    pub transfer_bytes: usize,
    /// All host calls, including failed accesses and describe calls.
    pub calls: u64,
}
impl Default for GrantLimits {
    fn default() -> Self {
        Self {
            resources: 64,
            chunk_bytes: 1024 * 1024,
            bytes: 128 * 1024 * 1024,
            transfer_bytes: 512 * 1024 * 1024,
            calls: 4096,
        }
    }
}
/// An immutable host resource; only the supervisor constructs it.
pub struct InputGrant {
    invocation: u64,
    descriptor: Descriptor,
    bytes: Arc<[u8]>,
}
/// Isolated exact/bounded output with contiguous coverage and exactly-once finish.
pub struct OutputGrant {
    invocation: u64,
    descriptor: Descriptor,
    bytes: Vec<u8>,
    finished: bool,
    exact: bool,
    finished_values: u64,
}
enum Handle {
    Input(u32),
    Output(u32),
}

/// Per-instance store data. Grants are revoked together after each invocation;
/// guest errors never turn candidate bytes into authoritative state.
pub struct HostState {
    /// Wasmtime enforces these bounds on every memory/table growth.
    pub(crate) memory: StoreLimits,
    table: ResourceTable,
    invocation: u64,
    handles: Vec<Handle>,
    limits: GrantLimits,
    bytes: usize,
    calls: u64,
    transferred: usize,
    invalid: bool,
    control: Option<crate::OperationControl>,
}
fn error(message: &str) -> wasmtime::Error {
    wasmtime::Error::msg(message.to_owned())
}
impl HostState {
    /// Construct an isolated resource table and explicit guest memory/table bounds.
    pub fn new(memory_bytes: usize, table_elements: usize, limits: GrantLimits) -> Self {
        Self {
            memory: StoreLimitsBuilder::new()
                .memory_size(memory_bytes)
                .table_elements(table_elements)
                .instances(16)
                .memories(1)
                .tables(8)
                .trap_on_grow_failure(true)
                .build(),
            table: ResourceTable::new(),
            invocation: 0,
            handles: vec![],
            limits,
            bytes: 0,
            calls: 0,
            transferred: 0,
            invalid: false,
            control: None,
        }
    }
    /// Revoke every old resource and start a fresh monotonically identified call.
    pub fn begin(&mut self) -> wasmtime::Result<u64> {
        self.check_control()?;
        self.revoke()?;
        self.invocation = self
            .invocation
            .checked_add(1)
            .ok_or_else(|| error("invocation counter exhausted"))?;
        self.calls = 0;
        self.transferred = 0;
        self.bytes = 0;
        self.invalid = false;
        Ok(self.invocation)
    }
    /// Revoke even failed/incomplete output resources. No implicit commit.
    pub fn revoke(&mut self) -> wasmtime::Result<()> {
        for handle in self.handles.drain(..) {
            match handle {
                Handle::Input(id) => {
                    self.table.delete(Resource::<InputGrant>::new_own(id))?;
                }
                Handle::Output(id) => {
                    self.table.delete(Resource::<OutputGrant>::new_own(id))?;
                }
            }
        }
        Ok(())
    }
    fn reserve(&mut self, schema: &str, bytes: usize) -> wasmtime::Result<()> {
        self.check_control()?;
        if self.invocation == 0
            || schema.is_empty()
            || schema.len() > 256
            || self.handles.len() >= self.limits.resources
        {
            return Err(error("invalid or excessive grant"));
        }
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.bytes)
            .ok_or_else(|| error("grant byte budget exceeded"))?;
        Ok(())
    }
    /// Grant immutable caller-owned bytes; this does not copy the underlying data.
    pub fn input(&mut self, buffer: Buffer) -> wasmtime::Result<Resource<InputGrant>> {
        self.reserve(&buffer.schema, buffer.bytes.len())?;
        let descriptor = Descriptor {
            schema: buffer.schema,
            byte_length: buffer.bytes.len() as u64,
            value_count: buffer.value_count,
        };
        let resource = self.table.push(InputGrant {
            invocation: self.invocation,
            descriptor,
            bytes: buffer.bytes,
        })?;
        self.handles.push(Handle::Input(resource.rep()));
        Ok(resource)
    }
    /// Reserve an isolated candidate with an exact byte/count extent.
    pub fn output(
        &mut self,
        schema: &str,
        bytes: usize,
        values: u64,
    ) -> wasmtime::Result<Resource<OutputGrant>> {
        self.output_with_policy(schema, bytes, values, true)
    }
    /// Reserve byte/value ceilings for opaque output whose actual extent is
    /// chosen by the kernel. All capacity is charged before guest execution;
    /// backing storage grows only as validated contiguous writes arrive.
    pub fn output_bounded(
        &mut self,
        schema: &str,
        bytes: usize,
        values: u64,
    ) -> wasmtime::Result<Resource<OutputGrant>> {
        self.output_with_policy(schema, bytes, values, false)
    }
    fn output_with_policy(
        &mut self,
        schema: &str,
        bytes: usize,
        values: u64,
        exact: bool,
    ) -> wasmtime::Result<Resource<OutputGrant>> {
        self.reserve(schema, bytes)?;
        let descriptor = Descriptor {
            schema: schema.into(),
            byte_length: bytes as u64,
            value_count: values,
        };
        let mut bytes = Vec::new();
        if exact {
            bytes
                .try_reserve_exact(descriptor.byte_length as usize)
                .map_err(|_| error("candidate allocation refused"))?;
        }
        let resource = self.table.push(OutputGrant {
            invocation: self.invocation,
            descriptor,
            bytes,
            finished: false,
            exact,
            finished_values: 0,
        })?;
        self.handles.push(Handle::Output(resource.rep()));
        Ok(resource)
    }
    /// Extract a completed candidate. Scientific validation and atomic publication
    /// belong to the supervisor, not the buffer's finish operation.
    pub fn take_output(&mut self, resource: Resource<OutputGrant>) -> wasmtime::Result<Buffer> {
        self.check_control()?;
        if self.invalid {
            return Err(error("invocation made an invalid grant access"));
        }
        let grant = self.table.get_mut(&resource)?;
        if grant.invocation != self.invocation || !grant.finished {
            return Err(error("candidate output is incomplete or stale"));
        }
        let bytes = std::mem::take(&mut grant.bytes).into();
        // Taking twice cannot manufacture an empty successful candidate.
        grant.finished = false;
        Ok(Buffer {
            schema: grant.descriptor.schema.clone(),
            value_count: grant.finished_values,
            bytes,
        })
    }
    fn meter(&mut self, bytes: usize) -> wasmtime::Result<()> {
        self.check_control()?;
        self.calls = self
            .calls
            .checked_add(1)
            .filter(|n| *n <= self.limits.calls)
            .ok_or_else(|| error("host-call budget exceeded"))?;
        self.transferred = self
            .transferred
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.transfer_bytes)
            .ok_or_else(|| error("host-transfer budget exceeded"))?;
        Ok(())
    }
    pub(crate) fn set_control(&mut self, control: crate::OperationControl) {
        self.control = Some(control);
    }
    fn check_control(&self) -> wasmtime::Result<()> {
        if let Some(control) = &self.control {
            control.check()?;
        }
        Ok(())
    }
}

impl buffers::Host for HostState {}
impl crate::field::orishu::simulation::types::Host for HostState {}
impl buffers::HostInput for HostState {
    fn describe(&mut self, resource: Resource<InputGrant>) -> wasmtime::Result<Descriptor> {
        self.meter(0)?;
        let g = self.table.get(&resource)?;
        if g.invocation != self.invocation {
            return Err(error("stale input grant"));
        }
        Ok(g.descriptor.clone())
    }
    fn read_chunk(
        &mut self,
        resource: Resource<InputGrant>,
        offset: u64,
        length: u32,
    ) -> wasmtime::Result<Result<Vec<u8>, AccessError>> {
        self.meter(length as usize)?;
        if length as usize > self.limits.chunk_bytes {
            return Ok(Err(AccessError::LimitExceeded));
        }
        let g = self.table.get(&resource)?;
        if g.invocation != self.invocation {
            return Ok(Err(AccessError::InvalidHandle));
        }
        let Some(end) = offset
            .checked_add(u64::from(length))
            .filter(|n| *n <= g.bytes.len() as u64)
        else {
            return Ok(Err(AccessError::OutOfBounds));
        };
        Ok(Ok(g.bytes[offset as usize..end as usize].to_vec()))
    }
    fn drop(&mut self, _: Resource<InputGrant>) -> wasmtime::Result<()> {
        Err(error("guest cannot own/drop borrowed input grants"))
    }
}
impl buffers::HostOutput for HostState {
    fn is_exact(&mut self, resource: Resource<OutputGrant>) -> wasmtime::Result<bool> {
        self.meter(0)?;
        let grant = self.table.get(&resource)?;
        if grant.invocation != self.invocation {
            return Err(error("stale output grant"));
        }
        Ok(grant.exact)
    }
    fn describe(&mut self, resource: Resource<OutputGrant>) -> wasmtime::Result<Descriptor> {
        self.meter(0)?;
        let g = self.table.get(&resource)?;
        if g.invocation != self.invocation {
            return Err(error("stale output grant"));
        }
        Ok(g.descriptor.clone())
    }
    fn write_chunk(
        &mut self,
        resource: Resource<OutputGrant>,
        offset: u64,
        length: u32,
        bytes: buffers::WriteFrame,
    ) -> wasmtime::Result<Result<(), AccessError>> {
        self.meter(bytes.len())?;
        if length as usize > self.limits.chunk_bytes || length as usize > bytes.len() {
            self.invalid = true;
            return Ok(Err(AccessError::LimitExceeded));
        }
        let bytes = &bytes[..length as usize];
        let g = self.table.get_mut(&resource)?;
        if g.invocation != self.invocation {
            self.invalid = true;
            return Ok(Err(AccessError::InvalidHandle));
        }
        if g.finished {
            self.invalid = true;
            return Ok(Err(AccessError::Finished));
        }
        if offset != g.bytes.len() as u64
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > g.descriptor.byte_length)
        {
            self.invalid = true;
            return Ok(Err(AccessError::OutOfBounds));
        }
        let needed = g.bytes.len() + bytes.len(); // checked against ceiling above
        if needed > g.bytes.capacity() {
            let capacity = needed
                .max(g.bytes.capacity().saturating_mul(2))
                .min(g.descriptor.byte_length as usize);
            g.bytes
                .try_reserve_exact(capacity - g.bytes.len())
                .map_err(|_| error("candidate allocation refused"))?;
        }
        g.bytes.extend_from_slice(bytes);
        Ok(Ok(()))
    }
    fn finish(
        &mut self,
        resource: Resource<OutputGrant>,
        byte_length: u64,
        value_count: u64,
    ) -> wasmtime::Result<Result<(), AccessError>> {
        self.meter(0)?;
        let g = self.table.get_mut(&resource)?;
        if g.invocation != self.invocation {
            self.invalid = true;
            return Ok(Err(AccessError::InvalidHandle));
        }
        if g.finished {
            self.invalid = true;
            return Ok(Err(AccessError::Finished));
        }
        if byte_length > g.descriptor.byte_length
            || value_count > g.descriptor.value_count
            || (g.exact
                && (byte_length != g.descriptor.byte_length
                    || value_count != g.descriptor.value_count))
            || g.bytes.len() as u64 != byte_length
        {
            self.invalid = true;
            return Ok(Err(AccessError::Incomplete));
        }
        g.finished = true;
        g.finished_values = value_count;
        Ok(Ok(()))
    }
    fn drop(&mut self, _: Resource<OutputGrant>) -> wasmtime::Result<()> {
        Err(error("guest cannot own/drop borrowed output grants"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffers::{HostInput, HostOutput};

    fn frame(bytes: &[u8]) -> buffers::WriteFrame {
        let mut frame = [0; 65536];
        frame[..bytes.len()].copy_from_slice(bytes);
        frame
    }

    #[test]
    fn grants_enforce_bounds_completion_revocation_and_host_work() {
        let limits = GrantLimits {
            resources: 2,
            bytes: 8,
            chunk_bytes: 4,
            transfer_bytes: 65536 + 16,
            calls: 8,
        };
        let mut state = HostState::new(65536, 10, limits);
        state.begin().unwrap();
        let input = state
            .input(Buffer {
                schema: "test/v1".into(),
                value_count: 1,
                bytes: Arc::from([1, 2, 3, 4]),
            })
            .unwrap();
        assert_eq!(
            state
                .read_chunk(Resource::new_borrow(input.rep()), 1, 2)
                .unwrap()
                .unwrap(),
            vec![2, 3]
        );
        assert!(
            state
                .read_chunk(Resource::new_borrow(input.rep()), u64::MAX, 1)
                .unwrap()
                .is_err()
        );
        let output = state.output("test/v1", 4, 1).unwrap();
        assert!(state.output("extra/v1", 1, 1).is_err());
        state
            .write_chunk(
                Resource::new_borrow(output.rep()),
                0,
                4,
                frame(&[8, 7, 6, 5]),
            )
            .unwrap()
            .unwrap();
        state
            .finish(Resource::new_borrow(output.rep()), 4, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            &*state
                .take_output(Resource::new_borrow(output.rep()))
                .unwrap()
                .bytes,
            &[8, 7, 6, 5]
        );
        assert!(
            state
                .take_output(Resource::new_borrow(output.rep()))
                .is_err()
        );
        state.revoke().unwrap();
        assert!(HostInput::describe(&mut state, Resource::new_borrow(input.rep())).is_err());
        state.begin().unwrap();
        let input = state
            .input(Buffer {
                schema: "test/v1".into(),
                value_count: 0,
                bytes: Arc::from([]),
            })
            .unwrap();
        for _ in 0..8 {
            HostInput::describe(&mut state, Resource::new_borrow(input.rep())).unwrap();
        }
        assert!(HostInput::describe(&mut state, Resource::new_borrow(input.rep())).is_err());
    }

    #[test]
    fn partial_or_duplicate_finish_poison_candidates_even_when_guest_ignores_error() {
        for partial in [false, true] {
            let mut state = HostState::new(65536, 10, GrantLimits::default());
            state.begin().unwrap();
            let output = state.output("test/v1", 1, 1).unwrap();
            if partial {
                assert!(
                    state
                        .finish(Resource::new_borrow(output.rep()), 1, 1)
                        .unwrap()
                        .is_err()
                );
            }
            state
                .write_chunk(Resource::new_borrow(output.rep()), 0, 1, frame(&[1]))
                .unwrap()
                .unwrap();
            state
                .finish(Resource::new_borrow(output.rep()), 1, 1)
                .unwrap()
                .unwrap();
            if !partial {
                assert!(
                    state
                        .finish(Resource::new_borrow(output.rep()), 1, 1)
                        .unwrap()
                        .is_err()
                );
            }
            assert!(
                state
                    .take_output(Resource::new_borrow(output.rep()))
                    .is_err()
            );
        }
    }

    #[test]
    fn write_frames_bound_lifting_ignore_padding_and_meter_physical_work() {
        let mut state = HostState::new(65536, 10, GrantLimits::default());
        state.begin().unwrap();
        let output = state.output("test/v1", 1, 1).unwrap();
        let mut padded = [99; 65536];
        padded[0] = 7;
        state
            .write_chunk(Resource::new_borrow(output.rep()), 0, 1, padded)
            .unwrap()
            .unwrap();
        state
            .finish(Resource::new_borrow(output.rep()), 1, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            &*state
                .take_output(Resource::new_borrow(output.rep()))
                .unwrap()
                .bytes,
            &[7]
        );
        assert_eq!(state.transferred, 65536);

        state.begin().unwrap();
        let output = state.output("test/v1", 1, 1).unwrap();
        assert!(matches!(
            state
                .write_chunk(Resource::new_borrow(output.rep()), 0, 65537, [0; 65536])
                .unwrap(),
            Err(AccessError::LimitExceeded)
        ));
        assert!(
            state
                .take_output(Resource::new_borrow(output.rep()))
                .is_err()
        );

        let mut state = HostState::new(
            65536,
            10,
            GrantLimits {
                transfer_bytes: 65535,
                ..GrantLimits::default()
            },
        );
        state.begin().unwrap();
        let output = state.output("test/v1", 0, 0).unwrap();
        // Even an empty logical write pays for the physical frame lifted by the ABI.
        assert!(
            state
                .write_chunk(Resource::new_borrow(output.rep()), 0, 0, [0; 65536])
                .is_err()
        );
    }

    #[test]
    fn bounded_grants_charge_ceilings_but_capture_actual_extent_without_padding() {
        let mut state = HostState::new(
            65536,
            10,
            GrantLimits {
                bytes: 8,
                ..GrantLimits::default()
            },
        );
        state.begin().unwrap();
        let out = state.output_bounded("opaque/v1", 8, 5).unwrap();
        assert!(!state.is_exact(Resource::new_borrow(out.rep())).unwrap());
        assert_eq!(
            HostOutput::describe(&mut state, Resource::new_borrow(out.rep()))
                .unwrap()
                .byte_length,
            8
        );
        assert_eq!(state.table.get(&out).unwrap().bytes.capacity(), 0);
        assert!(state.output_bounded("extra/v1", 1, 0).is_err());
        state
            .write_chunk(Resource::new_borrow(out.rep()), 0, 2, frame(&[1, 2]))
            .unwrap()
            .unwrap();
        state
            .write_chunk(Resource::new_borrow(out.rep()), 2, 1, frame(&[3]))
            .unwrap()
            .unwrap();
        state
            .finish(Resource::new_borrow(out.rep()), 3, 2)
            .unwrap()
            .unwrap();
        let result = state.take_output(Resource::new_borrow(out.rep())).unwrap();
        assert_eq!(result.bytes.as_ref(), &[1, 2, 3]);
        assert_eq!(result.value_count, 2);
        assert_eq!(result.schema, "opaque/v1");
        assert!(state.take_output(Resource::new_borrow(out.rep())).is_err());
        state.begin().unwrap();
        let empty = state.output_bounded("empty/v1", 8, 5).unwrap();
        state
            .finish(Resource::new_borrow(empty.rep()), 0, 0)
            .unwrap()
            .unwrap();
        let result = state
            .take_output(Resource::new_borrow(empty.rep()))
            .unwrap();
        assert!(result.bytes.is_empty());
        assert_eq!(result.value_count, 0);
    }

    #[test]
    fn bounded_output_cannot_exceed_ceiling_hide_bytes_or_recover_from_bad_completion() {
        for case in 0..5 {
            let mut state = HostState::new(65536, 10, GrantLimits::default());
            state.begin().unwrap();
            let out = state.output_bounded("opaque/v1", 4, 2).unwrap();
            if case == 0 {
                assert!(
                    state
                        .write_chunk(Resource::new_borrow(out.rep()), 0, 5, frame(&[1; 5]))
                        .unwrap()
                        .is_err()
                );
            }
            state
                .write_chunk(Resource::new_borrow(out.rep()), 0, 3, frame(&[1; 3]))
                .unwrap()
                .unwrap();
            match case {
                1 => assert!(
                    state
                        .finish(Resource::new_borrow(out.rep()), 3, 3)
                        .unwrap()
                        .is_err()
                ),
                2 => assert!(
                    state
                        .finish(Resource::new_borrow(out.rep()), 2, 2)
                        .unwrap()
                        .is_err()
                ),
                3 => assert!(
                    state
                        .finish(Resource::new_borrow(out.rep()), 4, 2)
                        .unwrap()
                        .is_err()
                ),
                _ => {}
            }
            state
                .finish(Resource::new_borrow(out.rep()), 3, 2)
                .unwrap()
                .unwrap();
            if case == 4 {
                assert!(
                    state
                        .finish(Resource::new_borrow(out.rep()), 3, 2)
                        .unwrap()
                        .is_err()
                );
            }
            assert!(
                state.take_output(Resource::new_borrow(out.rep())).is_err(),
                "case {case}"
            );
        }
        // Existing exact behavior is not weakened by adding bounded grants.
        let mut state = HostState::new(65536, 10, GrantLimits::default());
        state.begin().unwrap();
        let out = state.output("exact/v1", 4, 2).unwrap();
        assert!(state.is_exact(Resource::new_borrow(out.rep())).unwrap());
        state
            .write_chunk(Resource::new_borrow(out.rep()), 0, 3, frame(&[1; 3]))
            .unwrap()
            .unwrap();
        assert!(
            state
                .finish(Resource::new_borrow(out.rep()), 3, 2)
                .unwrap()
                .is_err()
        );
        assert!(state.take_output(Resource::new_borrow(out.rep())).is_err());
    }
}
