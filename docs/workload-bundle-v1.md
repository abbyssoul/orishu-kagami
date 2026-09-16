# Portable workload bundle v1

Implemented for the [v3 fixed scientific profile](workload-v3.md) by
`orishu_plugin::workload::bundle` and Kagami's headless `export` command.
This is a distribution encoding, not another workload identity, plugin installer,
run authorization or worker API. Worker upload/admission endpoints remain open.

## Export a captured experiment

```sh
kagami export experiment.kagami --output experiment.orishu \
  --name gravity-example --directory /path/to/plugins --json
```

The input must contain a valid scientific setup with captured initial field and
integrator state. Legacy setup requires explicit model selection/initialization
first; export does not migrate it, choose replacement models, execute guests or
initialize state. The Unix window now creates/replaces scientific setups and
resets captured fields through guarded document commands; scientific MCP parity
remains pending. This command consumes captured files from that window or the
document/session APIs. It never opens a window or MCP listener.

`--directory` defaults to `KAGAMI_PLUGIN_DIR`, then the normal user data root.
The exact selected releases/code must be installed and available. Export freezes
them under the existing inventory/release-lease authority. A concurrent inventory
revision change refuses instead of retrying or choosing different providers.
`--expected-inventory-revision N` lets a headless caller supply its own guard.
Otherwise export reads the current revision and uses that guard for preparation.
`--enable-plugin ID` / `--disable-plugin ID` are repeatable, process-only overrides;
they do not update persisted enablement. Duplicate/conflicting overrides and
uninstalled override IDs refuse. Later management changes cannot alter already
frozen bytes.

The output is a **new file only**. Existing files and final symlinks refuse;
the input is never rewritten or recovered from a backup. The current secure IO
adapter is Unix-only. Publication uses the shared descriptor-relative staging,
file sync, new-only link and directory sync path. A failure during final directory
sync can leave a published file: inspect it before retrying. User-selected parent
paths are configuration, not archive-controlled paths. Temporary inventory lock
and lease files may be created; export does not change inventory selections.

`--json` prints one `kagami.workload-export/v1` object. Success has `ok: true`
and `result` with `workload`, `inventoryRevision`, `artifacts` (distinct blobs,
excluding root) and `bytes`. Refusal has `ok: false`, `error.stage`, `error.code`
and a bounded message; selection refusals include the structured resolver outcome
and its options. Exit status is nonzero on refusal. Invalid CLI syntax is Clap's
usage error, not this JSON protocol. Normal output is pretty JSON (success on
stdout, refusal on stderr). Neither output nor successful export grants permission
to execute the workload.

## Physical layout and verification

One strict classic stored ZIP contains:

```text
workload.cbor                 canonical workload-v3 root
blobs/sha256/<64 lowercase hex digits>    each required artifact, once
```

The root precedes digest-sorted blob entries in the deterministic writer. Headers
use the shared archive codec's fixed metadata. There is no compression, ZIP64,
encryption, archive comment, symlink, directory entry, arbitrary path or external
fetch. The shared reader checks header consistency, entry counts, ranges,
non-overlap and CRC before exposing borrowed slices; it never extracts files.
Only this closed root name is accepted, so plugin and experiment containers are
not accepted as workloads. Header/layout changes require an explicit encoding
extension, not a silent change to v1.

The workload wrapper then parses the bounded canonical v3 root, requires physical
blob membership to equal its required descriptor set, and independently verifies
the entire selected scientific closure. It checks hashes, declared sizes, schemas,
selection reachability, configurations, captured state and scene/packet agreement.
Release-root evidence does not make unselected plugin code a dependency. Packing
selects only required descriptors from a caller's cache; reading rejects extras
and missing blobs. No installed inventory participates in reading or runtime
admission. Runtime admission additionally enforces sandbox/run policy, compiles
the exact guests and validates their captured state; it does not initialize them.

Archive bytes, filename, path and inventory revision are not workload identity.
Repacking the same root/closure preserves its digest. The CLI's explicit workload
name is root metadata and **does** affect identity; camera/default-view changes do
not. Original editable document metadata is not smuggled into runtime inputs.

Default archive bounds are 1 GiB total bytes, 1 MiB root bytes, 256 MiB per blob
and 1 GiB aggregate physical blob bytes. Workload/profile limits tighten these:
at most 512 artifact descriptors (plus the physical root), 128 MiB per scientific
input, 512 MiB aggregate scientific input uses, and 16 MiB scene data. Selection,
canonical-tree and numerical record limits apply independently. Caller-supplied
archive and profile policies may tighten bounds further. This initial API buffers
the complete archive and returns borrowed verified slices; it is **not** a
streaming transfer implementation or a bound on total process RSS. Pending IO
reservations outside the worker adapter below, thin transfer and large-input
streaming remain explicit work.

### Internal worker input adapter

`PreparedAdmission::receive` reads a complete portable body under an existing
exclusive formation-owner execution lease. The caller must authenticate and
authorize before preparing that lease. It supplies a positive exact announced
body length and the expected workload root; `DeliveryLimits` is host policy,
not a caller-controlled relaxation. The default is 128 MiB and an absolute
30-second window, additionally bounded by archive and operation policy. Progress
does not renew the deadline. EOF must delimit this body, not the reusable client
connection; protocol adapters must provide the appropriate body-bounded reader.

Length/policy checks precede allocation or polling input. The receiver reserves
one fallible payload allocation, reads at most 64 KiB per quantum and checks one
additional byte for overrun. Missing/excess bytes, transport failure, deadline,
cancellation and revoked formation refuse without publication. Waiting for EOF
also consumes the deadline and exclusive slot. Ready readers yield between
quanta so they cannot monopolize the async owner thread. Input storage is moved
into blocking closure verification without a whole-buffer Arc conversion/copy.
Logical payload length/allocation policy is not an aggregate RSS guarantee.

Closure verification, including all scientific identities, precedes comparison
with the requested root. A mismatch refuses **before** epoch allocation and JIT;
correct identity still requires independent deny-policy and runtime admission.
No field initialization or plugin inventory participates. The existing owner
confirmation and commit gate still decide whether a run becomes visible.
Dropping delivery closes its owned reader; after native work starts, cancellation
retains the execution lease until actual disposal. A Unix-socket fixture exercises
this path through real Component admission and an accepted fixed step.

The [opt-in scientific HTTP profile](protocol-scientific-load-v1.md) now wraps this
receiver with authenticated framing and durable identified receipts. It does not
provide a cache, missing-blob negotiation or resumable transfer. Public run control
and client adapters remain open; successful byte delivery or admission alone must
not be reported as an accepted/published run.

## Encoding choice and evidence

[ADR 0010](adr/0010-content-addressed-workload-closure-and-portable-bundles.md)
required comparing a minimal layout with OCI Image Layout before selecting the
first encoding. The actual two-object Newtonian/Euler export test now includes a
reproducible framing comparison. Both layouts use identical scientific bytes and
the same uncompressed ZIP framing. The OCI-shaped alternative adds `oci-layout`
and an `index.json` descriptor for the custom CBOR root, placing that root under
its digest. Those control files follow the
[OCI layout](https://github.com/opencontainers/image-spec/blob/main/image-layout.md)
and [index](https://github.com/opencontainers/image-spec/blob/main/image-index.md)
specifications; this is not a container-image or OCI runtime compatibility claim.

Measured on the checked-in reference fixture (24 blobs, 5,204-byte root):

| Layout | Total bytes |
| --- | ---: |
| Actual minimal stored-ZIP export | 1,328,386 |
| OCI-shaped layout with identical stored-ZIP framing | 1,328,974 |
| Difference | 588 |

The test checks the framing formula against the actual exported bytes. This is a
layout-size comparison, not a throughput, memory or complete OCI integration
benchmark. The small difference is not a significant efficiency claim. Choose
the minimal layout initially because it reuses the existing strict codec, has one
unambiguous root, and enforces a complete exact closure without auxiliary index
semantics. OCI reuse remains a possible additional distribution adapter; identity
and the scientific contracts do not need to change for it.

Reproduce with:

```sh
cargo test --locked -p kagami --test scientific_initialization \
  real_captures_enter_document_authority_atomically_and_undo_restores_bytes -- --nocapture
```

That test invokes the actual binary, reads its published file, then admits and
advances it with a fresh real Newtonian/Euler runtime after dropping the test
inventory. It also covers deterministic packing, unused-code exclusion, extra or
missing blobs, wrong root type, truncation/corruption, bounds, stale/disabled
selections, process-only overrides, source preservation and new-only/symlink
refusals. General traversal/duplicate/overlap cases are shared archive-codec tests.

Emitter blueprints/dynamic membership, GUI export controls, worker delivery/run
endpoints and the other [delivery gates](tasks/x-plugin-delivery-ledger.md) are
not implied complete by this fixed-membership headless path.
