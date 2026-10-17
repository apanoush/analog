# 0004: Depth model for focus-based grain

Date: 2026-10-12
Status: accepted

## Context

Focus-based grain and depth-aware halation need a per-image depth (focus) proxy,
provided by a neural depth model rather than a classical focus estimate.

## Decision

Use Depth Anything V2 Small (`onnx-community/depth-anything-v2-small`,
Apache-2.0) through ONNX Runtime via the `ort` crate. Ship no model; download the
int8-quantised ONNX file (~27 MB) on demand into the user cache and load it from
there afterwards.

## Alternatives considered

- **A classical focus/sharpness estimate.** Cheaper and dependency-free, but much
  weaker on the subject/background separation the effect needs. Rejected.
- **Ship the fp32 model (~99 MB) or fp16 (~50 MB).** Better numerical fidelity,
  but a heavy download. The int8 model is a good size/quality trade-off for a
  depth proxy that only modulates grain and glow.
- **Bundle the model in the repository or installer.** Avoids a runtime download
  but bloats the repo and the binary; also complicates licensing review. The
  on-demand download keeps the repo small and the model attribution clear.
- **`ort` with a system ONNX Runtime.** Avoids the build-time binary download but
  requires the user to install ONNX Runtime. The `download-binaries` default is
  simpler for users.

## Consequences

- The core crate depends on `ort` and `ureq`; the build fetches ONNX Runtime
  binaries the first time.
- The prebuilt ONNX Runtime objects for Apple Silicon are compiled with
  `MACOSX_DEPLOYMENT_TARGET=13.4`, so `.cargo/config.toml` matches that target
  and the macOS arm64 binary requires macOS 13.4 or newer.
- Those objects use `@available`, which lowers to a call to
  `__isPlatformVersionAtLeast` in Clang's `libclang_rt.osx.a`. Rust final-links
  with `-nodefaultlibs` and therefore does not add compiler-rt automatically;
  `crates/analog-core/build.rs` locates the active toolchain's archive with
  `clang -print-resource-dir` and links it explicitly.
- The prebuilt ONNX Runtime for Windows uses the dynamic MSVC runtime (/MD).
  Release builds therefore set `msvc-crt-static = false` in
  `dist-workspace.toml`; the alternative static CRT (/MT) fails with LNK2038
  and unresolved `__imp_*` UCRT symbols. The Windows installer requires the
  Microsoft Visual C++ 2015-2022 Redistributable (x64) on the target machine.
- The first focus-based render downloads ~27 MB and can block briefly; the GUI
  loads the model lazily and reports progress in its status line.
- The image is squashed to the model's 518x518 input, an approximation
  documented in `docs/architecture.md`.
- Offline use works after the model is cached.

## Sources

- Depth Anything V2 Small model card (Apache-2.0).
