# Kagami

Kagami is the native Orishu client for authoring experiments, controlling
workloads, and visualizing live or recorded observations.

The current crate reuses the Iced application shell and offscreen-capable GPU
renderer from the standalone prototype. It can select and display an Orishu
endpoint, but it does not connect or stream observations yet.

The scene tree and inspector are no longer demo state: they render the read
projection of a real [`kagami-session`](../../crates/kagami-session)
`DocumentAuthority`, and every edit is a submitted command envelope — the same
one an MCP client would send (ADR 0006). The window owns presentation state and
nothing else, so a refused edit is reported and leaves the view showing the last
accepted revision. File → New, Open, Save and Save As read and write the
versioned `kagami.experiment` format through the durable write protocol.

The component schemas the inspector offers are a bundled stand-in until the
simulation-plugin inventory (X-PLUGIN) exists. They are data: the inspector
branches on no component name, so a plugin's component becomes authorable the
moment its schema is registered.

From the repository root:

```sh
make run-kagami
make run-kagami ARGS="--host cluster.example.com:6680"
make smoke-kagami
```

Use `--exit-after SECONDS` for bounded windowed smoke testing. On systems where
Vulkan presentation misbehaves, try `WGPU_BACKEND=gl` or
`ICED_PRESENT_MODE=no_vsync`.

See the [project architecture](../../docs/architecture.md) and [migration
strategy](../../docs/migration.md).
