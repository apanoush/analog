# 0007: Camera RAW input

Date: 2026-10-15
Status: accepted

## Context

The editor's premise is an already-developed photo, so for a long time the only
inputs were JPEG/PNG/TIFF (plus PSD). Users asked to open camera RAW files
directly instead of exporting a developed image first.

RAW is a much larger problem than adding a codec. A RAW file is single-channel
Color Filter Array (CFA) sensor data plus metadata; turning it into a viewable
image needs demosaicing, per-camera white balance and a colour matrix, a tone
curve and gamma, and highlight handling. The per-camera colour matrix and levels
are the hard part: without them, saturated colours and skin tones are wrong.

## Decision

Decode RAW through **LibRaw**, a developer-grade raw engine, via the
Rust wrapper [`libraw-rs`](https://crates.io/crates/libraw-rs) (MIT; it vendors
LibRaw 0.20.1). LibRaw ships per-camera profiles, so the result is a fully
developed 16-bit sRGB image with no colour science of our own.

- RAW files are recognised by extension (`crates/analog-core/src/raw.rs`) and
  developed to `Rgba16`, entering the pipeline like any other 16-bit input.
- RAW is **input only**: `Image::encode` rejects RAW extensions.
- Decoding is behind the `raw` feature of `analog-core` (on by default in the
  CLI and GUI; off for the library) so the default library build stays pure
  Rust and does not need a C++ compiler.

`libraw-rs` was chosen over `rsraw` because `rsraw-sys` always runs `bindgen`,
which needs libclang; `libraw-rs-sys` ships pre-generated bindings and only
needs the `cc` crate plus a C++ compiler. Both wrap the same LibRaw.

## Alternatives considered

- **`rsraw` (MIT wrapper, LibRaw).** Newest and best documented, but its build
  always runs `bindgen` for native targets, requiring libclang, which is not
  available in this environment. Rejected here; a fine choice where libclang is
  present.
- **Pure Rust: `rawler`/`rawloader` + `demosaic`.** No C toolchain, and
  `demosaic` (MIT/Apache) offers professional demosaicing (AHD/VNG/PPG,
  Markesteijn). But the wrappers give CFA data and metadata only: we would have
  to implement white balance, the camera colour matrix, the tone curve and
  gamma, and maintain a per-camera profile database. `rawler` is also LGPL-2.1.
  Rejected as disproportionate scope with lower fidelity outside DNG.
- **DNG only, pure Rust.** DNG carries its own `ColorMatrix`/`AsShotNeutral`, so
  a pure-Rust path is good for DNG. Rejected because it would not cover
  CR2/NEF/ARW, which users asked for.
- **Embedded JPEG preview extraction.** Cheap, but 8-bit and already processed,
  so it is not real RAW. Rejected.

## Consequences

- RAW builds need a C++ compiler; the vendored LibRaw compiles in roughly 25 s
  on 8 cores and is cached. `--no-default-features` gives a pure-Rust build.
- LibRaw is LGPL-2.1 (CDDL dual). The `libraw-rs` wrapper is MIT, but the linked
  LibRaw library keeps its own licence; downstream distributors must honour it.
- RAW input is 16-bit, so it exercises the existing 16-bit pipeline (slower
  passes, larger buffers) rather than the 8-bit path.
- Camera support is LibRaw's (400+ models, 0.20.1); unsupported or corrupt files
  surface as a clear decode error.

## Sources

- `libraw-rs` 0.0.4 and `libraw-rs-sys` 0.0.4 build scripts
  (https://docs.rs/crate/libraw-rs-sys/0.0.4+libraw-0.20.1/source/build.rs):
  `bindgen` is an optional feature, otherwise a checked-in `bindings.rs` is used.
- `rsraw-sys` build script (`https://github.com/hexilee/rsraw`): `bindgen` runs
  for host builds.
- LibRaw (https://www.libraw.org/) for the camera database and processing.
