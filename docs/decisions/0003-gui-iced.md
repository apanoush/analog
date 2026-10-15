# 0003: Use iced for the GUI

Date: 2026-10-12
Status: accepted

## Context

The project needs a desktop GUI: grouped parameter sliders and a live preview
of the effects on a selected photo. The GUI must stay pure Rust so it shares the
workspace toolchain and the `analog-core` data model.

## Decision

Use `iced` (retained, ELM-style) for the desktop GUI, in its own crate so its
dependencies never leak into the library or CLI.

## Alternatives considered

- **egui / eframe.** Immediate-mode; faster to assemble a slider-and-preview
  editor, but a weaker fit for the structured, message-driven UI this editor
  will grow into.
- **Tauri + React.** Would allow a WebGPU front end, but adds a web toolchain
  and a heavier dependency surface. Rejected to keep the stack pure Rust.

## Consequences

- `iced` renders through `wgpu`; the compute engine and the UI both use `wgpu`.
  How to share or bridge the device (a custom shader widget versus a separate
  compute device plus a pixel handoff) is decided during implementation and
  recorded there.
- The GUI crate starts as a stub.

## Addendum (2026-10-12)

`iced` 0.14 depends on `wgpu` 27, while the engine uses `wgpu` 30, so the two
cannot share a device without pinning both to the same version. The GUI therefore
runs the engine on its own device, reads back CPU pixels, and hands `iced` an
`image::Handle`. This is simple and robust for a downscaled preview (longest side
900 px); a shared device or an `iced` custom shader widget is a possible future
optimization if preview latency becomes a problem.
