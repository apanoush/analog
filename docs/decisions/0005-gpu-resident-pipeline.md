# 0005: GPU-resident render pipeline

Date: 2026-10-13
Status: accepted

## Context

The interactive preview became slow after the depth, grain-fidelity and 16-bit
milestones. The engine read back to the CPU after every compute pass
(`ImageComputePass` copied to a staging buffer and blocked on
`device.poll(wait_indefinitely)`), and repacked every input with `as_words()`.
A full render is around sixteen passes, so a single preview did roughly sixteen
synchronisation stalls and sixteen CPU repacks.

## Decision

Keep the whole chain on the GPU. Introduce `GpuImage` (a GPU buffer plus its
dimensions and format). Passes take `&GpuImage` inputs and return a `GpuImage`
with no readback; `GpuImage::read` is the single synchronisation point, called
once by `Renderer::render`. Compile passes lazily, and only the pixel-format
variant actually used.

## Alternatives considered

- **Pool CPU readbacks / pipeline the passes on the CPU.** Keeps the CPU-mediated
  design and its sync stalls. Rejected.
- **Render to a texture and share the wgpu device with iced.** Would avoid the
  final readback too, but iced 0.14 pins a different `wgpu` version (see
  `0003-gui-iced.md`), so the device cannot be shared. The single readback is
  cheap relative to the sixteen it replaces.
- **Debounce only.** Cheap, but the underlying render would still take ~0.6 s, so
  the preview would stay visibly laggy. Debounce was kept as a complement, not a
  substitute.

## Consequences

- The effect chain no longer touches the CPU between passes; measured cost at
  900x600 fell from about 0.58 s to about 0.15 s.
- Startup no longer compiles unused pipelines (lazy, per-format compilation), so
  launch is fast.
- `GpuImage` buffers are not pooled yet; each pass allocates a fresh output.
- `Renderer::new` no longer needs a `GpuContext`; the context is passed to
  `render`.
