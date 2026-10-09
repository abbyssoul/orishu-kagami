# kagami-renderer

Kagami's Iced/wgpu rendering boundary: a perspective/orthographic orbit camera
driving a wgpu grid-and-axis stage with optional depth-tested position markers, exposed as an
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
| `Marker`, `MarkerBatch`, `MAX_MARKERS` | Finite render-unit positions and RGB colors, bounded to 65,536 fixed-size glyphs. No scientific identity or physical radius is implied. |
| `Arrow`, `ArrowBatch`, `MAX_ARROWS` | Finite nondegenerate render-unit endpoints and RGB colors, bounded to 4096 direction glyphs sharing depth with markers. The caller owns vector meaning, normalization and spatial conversion. |

The camera pose and its bounds belong to whoever has to save them, not to this
crate. That keeps view state outside authoritative experiment intent.

## Testing and fuzzing

There is no persisted/wire parser. Unit tests enforce finite marker/color and
batch capacity bounds. `make smoke-kagami` uses the actual program/primitive and
GPU readback to check both projections, marker depth, behind-camera clipping,
unchanged-batch reuse, replacement and removal, plus arrow rendering/shared depth
and camera-only reuse, then runs the 120-frame camera
sweep. Manual native-window inspection remains necessary for interaction/layout.

The app prepares scale-converted batches outside its draw loop. The single-scene
pipeline caches their Arc identity, reuses an instance buffer (at most 1.5 MiB)
and resizes its depth target with the window. Geometry uploads are O(markers)
only on a changed batch; unchanged preparation is O(1), and GPU draw work remains
O(markers). The grid/axes are contextual, not physical depth surfaces. This is
not a multi-viewport cache or a physical mesh renderer. Arrows have a separate
reused instance buffer capped at 144 KiB and the same O(arrows) changed-upload /
O(1) unchanged-preparation behavior. Their shaft/head width is screen-space;
endpoints remain 3D. Arrows crossing near/far clip planes are omitted as a whole,
as are subpixel/view-axis directions, rather than dividing behind-camera points.

Fuzzing: **not required.** No untrusted bytes reach this crate; camera and
pointer inputs come from the host widget, not from a peer, file, or plugin.
