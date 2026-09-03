# Kagami

Kagami is the native Orishu client for authoring experiments, controlling
workloads, and visualizing live or recorded observations.

The current crate reuses the Iced application shell and offscreen-capable GPU
renderer from the standalone prototype. It can select and display an Orishu
endpoint, but it does not connect or stream observations yet. The displayed
scene tree is explicitly demo state, not the authoritative experiment model.

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
