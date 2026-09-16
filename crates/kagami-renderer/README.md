# kagami-renderer

Kagami's Iced/wgpu rendering boundary: a perspective/orthographic orbit camera
driving a wgpu grid-and-axis "stage", exposed as an
`iced::widget::shader::Program`. It owns GPU presentation mechanics and never
authoritative experiment or simulation state. See
[ADR 0022](../../docs/adr/0022-persist-default-view-outside-experiment-intent.md).

## Public contract

| Type | Role |
| --- | --- |
| `SceneProgram` | The `iced::widget::shader::Program`. It is handed a `Camera` each frame and publishes a `CameraMotion` for every gesture it recognises. Only pointer bookkeeping stays in the widget. |
| `Camera`, `Projection` | The camera pose and projection. The camera is an input, not renderer state. |
| `CameraMotion`, `PointerState` | The gesture output and pointer input the widget tracks. |
| `ScenePrimitive` | The `iced` shader primitive the program produces. |
| `GridAxisPipeline`, `Uniforms` | The wgpu pipeline and its uniform block. |

The camera pose and its bounds belong to whoever has to save them, not to this
crate. That keeps view state outside authoritative experiment intent.

## Testing and fuzzing

Contract testing: **not required.** The crate has no persisted or wire format
and parses no untrusted input; its public surface is a GPU rendering program.
Correctness is a visual and hardware concern, verified through
`make smoke-kagami` where graphics hardware is available (see the repository
`AGENTS.md`), not through serialized-contract tests. An `examples/` binary
exercises the program interactively.

Fuzzing: **not required.** No untrusted bytes reach this crate; camera and
pointer inputs come from the host widget, not from a peer, file, or plugin.
