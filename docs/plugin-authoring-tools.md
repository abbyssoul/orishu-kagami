# Local plugin authoring tools

Status: **initial Unix source/package and CLI management implemented, review
pending**. This is declaration verification, not Wasm ABI admission or scientific
execution. Windows secure package IO, application UI/MCP management, document
selection adoption and workload export remain implementation work. The shared
sandbox library exists; it is not invoked by plugin management. See
[X-PLUGIN delivery](tasks/x-plugin-delivery-ledger.md).

The app-local `PluginStore::resolve_authoring` adapter returns the provider decision
and, only for a fully resolved selection, component schemas from that same verified
inventory snapshot. Those exact schemas retain original scientific roles/bindings,
defaults and property limits; the document authority and catalog enforce them.
Stale, disabled, missing or ambiguous selections return no substitute schemas.
This adapter does not mutate a document, install anything, or execute a kernel.
Unix application startup now loads this vocabulary into the document authority;
document-selection dialogs and open-document lease adoption remain open.

## Build inputs

Plugins are built outside Kagami. The initial low-level source profile uses
`plugin.json` and explicitly named regular files beneath its directory:

```json
{
  "apiVersion": "orishu.plugin-source/v1",
  "metadata": {
    "pluginId": "org.example.constants",
    "versionLabel": "development"
  },
  "contributions": [
    {
      "localId": "gravity-constants",
      "extensionPoint": "orishu.model.constants/v1",
      "path": "constants.json"
    }
  ],
  "artifacts": []
}
```

For example, `constants.json` contains a contribution payload:

```json
{
  "scientific": {
    "name": "org.example.gravity.constants",
    "version": 1,
    "requirements": [],
    "constants": [
      {
        "id": "g",
        "dimension": [3, -1, -2, 0, 0, 0, 0],
        "valueSI": 6.67430e-11,
        "meaning": "Newtonian gravitational constant"
      }
    ]
  }
}
```

Known contributions use the shared [payload schemas](../crates/orishu-plugin/schema/plugin-v1.schema.json)
as JSON. Packing validates them, emits canonical CBOR, derives root requirement
slots from their exact scientific requirements and hashes payload/artifact bytes.
Unknown well-formed extension points accept raw files as opaque contributions;
this initial source profile supplies no requirements for opaque payloads. Neither
unknown bytes nor declared Wasm artifacts execute during these operations.

Each extra kernel/input entry is `{ "path": "kernel.wasm", "mediaType":
"application/wasm" }`. Its descriptor is computed from its actual bytes.
Current source payloads carry already-exact scientific references and kernel
digests, produced by external build tooling using the shared identity helpers.
Symbolic source-local dependency/kernel aliases and automatic topological source
compilation are still pending; this input profile does not silently guess them.
Do not put mutable paths into scientific identity fields. Build leftovers not
explicitly listed in `plugin.json` are not packaged.

Directory-relative opens reject path escapes, symlink files and symlinked
descendants; source paths are never extracted from an archive or passed to a
shell. Limits apply before source acquisition, including a 1 MiB source manifest,
256 contributions, 4,096 explicit artifact entries, 256 MiB per artifact and
1 GiB aggregate artifact bytes. The archive has a separate aggregate byte limit,
so a source at an artifact limit need not fit with archive overhead. Typed list
readers refuse excess entries before deserializing them.

## Commands

Run these without opening a window or starting MCP:

```sh
kagami plugin validate ./my-plugin
kagami plugin pack ./my-plugin --output ./my-plugin.okplugin
kagami plugin inspect ./my-plugin.okplugin
kagami plugin install ./my-plugin.okplugin
kagami plugin list --all-releases
kagami plugin disable org.example.constants
kagami plugin enable org.example.constants
```

Also implemented: `update <plugin-id> <bundle>`, `set-default <plugin-id>
<release-id>`, `inspect <release-id>` and `remove <plugin-id> <release-id>
[--ack-open-references]`. Install/update accept `--expect-release <sha256:...>`.
Packing refuses an existing output atomically; it never overwrites another bundle.
The [concrete contract](plugin-contract-v1-draft.md#9-local-inventory-and-cli-r7)
defines default selection and enablement semantics.

All commands accept `--json`. Outcomes have `apiVersion:
"kagami.plugin-command/v1"`, an `ok` boolean and either `result` or bounded `error`.
Failures exit nonzero. Management commands accept `--expected-revision <integer>`;
otherwise the CLI reads a revision before submitting and still rejects a concurrent
change rather than blindly retrying. The store's nonblocking writer lock returns
`Busy` if another process owns it. Retry after inspecting current state.

Use `--directory <path>` or `KAGAMI_PLUGIN_DIR` for an explicit inventory. Otherwise
the current Unix shell uses `$XDG_DATA_HOME/kagami/plugins` or
`$HOME/.local/share/kagami/plugins`. These are local distribution paths, never
workload identity. Validate/pack and path-based inspect do not open the inventory.

## Store and reader safety

The store publishes `orishu.plugin-inventory/v1` in `index.json`: an explicit
revision and at most 256 release registrations, each with logical ID, exact
release, enablement and default flag. There is exactly one default per logical
plugin; all its releases share enablement. Logical version labels never choose a
default. The index has a 256 KiB byte ceiling. Root manifests and artifact bytes
live in separate digest-keyed directories and are independently reverified on use.

Writers take one process/cross-process lock, validate the command, stage and flush
all immutable blobs and the canonical root, then atomically replace and flush the
index and its parent. Unreferenced staging is ignored. A failed final directory
flush can mean uncertain persistence; reread the index before retrying a mutation.
Do not claim such a failure proves rollback. Fault-injection across every crash
boundary remains follow-up verification, beyond the failed-write/reopen tests.

Shared OS locks implement explicit release leases for open documents/runs. Removal
requires acknowledgement while a lease is held and only de-registers the release;
it does not purge cached data. Leases survive acknowledged removal and disappear
when their process exits. Physical garbage collection is not implemented. Local
origin metadata capture and UI warnings based on actual open-document leases are
still pending adapter work.

The authority can resolve installed providers with process-only overrides without
changing the index. Its cold snapshot acquisition has a 1 GiB aggregate artifact
read ceiling; it does not load kernels per viewport frame. Native startup supports
`--plugin-directory PATH` (or `KAGAMI_PLUGIN_DIR`), `--enable-plugin ID` and
`--disable-plugin ID`. Discovery reads each release once and independently resolves
at most 256 component declarations, preserving usable vocabulary when another
contribution is dormant. Non-default enabled releases remain available to saved
pins. Discovery does not choose a field model or integrator. Unknown/conflicting
overrides, corrupt content and IO/budget errors fail before window/MCP startup;
dependency issues are reported as unavailable contributions without substitution.
The Add-component action submits declared defaults as ordinary authored values;
the authority, not the UI, decides completeness, units and constraints.
Live registry reload, dependency-choice dialogs, open-document leases and full
management UI/MCP parity remain work, not completed X-PLUGIN integration.
