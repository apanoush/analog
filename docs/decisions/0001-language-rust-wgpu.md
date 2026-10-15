# 0001: Use Rust with wgpu for the engine

Date: 2026-10-12
Status: accepted

## Context

The engine must synthesize film grain, halation, print simulation and film
color shifts over images up to 16k x 16k, in 8- or 16-bit, with a live preview.
The project needs a library, a CLI and a desktop GUI.

## Decision

Build the engine and library in Rust on `wgpu`, running our own WGSL compute
shaders. `wgpu` is the reference implementation of WebGPU and targets Vulkan,
Metal and DirectX 12 from one codebase.

## Alternatives considered

- **Python.** Faster to prototype and stronger for the colour-science and
  machine-learning parts (`colour-science`, ONNX Runtime). Rejected as the
  primary language because of packaging friction, the GIL, and CPU-side cost on
  large 16-bit images. `wgpu-py` would still use WGSL, so a Python prototype
  remains possible for isolated experiments.
- **C++.** Closest to native GPU APIs but heavier tooling and slower iteration.
- **Hybrid Rust core + Python tooling.** Kept as an option for colour-science
  experiments and depth-model work, not as the primary language.

## Consequences

- One toolchain (Cargo) produces the library, CLI and GUI.
- WGSL shaders written for WebGPU port to `wgpu` with minimal change.
- The depth model will be called from Rust through the `ort` crate.
- A Python scratchpad may be used for one-off colour-science derivations; it is
  not part of the shipped workspace.

## Sources

- WebGPU / `wgpu` (Vulkan, Metal, DirectX 12 backends).
