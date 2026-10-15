# analog

A local-first film grain editor: a Rust library, a CLI and a desktop GUI that
add film-like grain, halation, print simulation and film color shifts to an
already-developed photo.

Processing is entirely local: images are never uploaded.

## Status

The four effect blocks (Film Color Shift, Grain, Halation, Print Simulation)
run on the GPU and are exposed through a library, a CLI and an `iced` desktop
GUI. Images are 8- or 16-bit; focus-based grain uses a neural depth model;
recipes import and export as `.analog` XMP. PSD and camera RAW files can be
imported (PSD/PSB export is planned); output is PNG, JPEG, TIFF or lossless
WebP. See [`docs/index.md`](docs/index.md).

## Layout

```
crates/
  analog-core/   library: recipe/params data model, image I/O, wgpu engine, WGSL shaders
  analog-cli/    command line tool
  analog-gui/    desktop GUI (iced)
```

## Build and use

Requires a stable Rust toolchain (edition 2024, rustc >= 1.93) and a GPU with
Vulkan, Metal or DirectX 12.

```sh
cargo build --workspace
cargo test --workspace

# inspect an image
cargo run -p analog-cli -- info photo.jpg
# render with effects
cargo run -p analog-cli -- render in.jpg out.png --film kodak_portra_400 --grain 60 --halation 50
# check the GPU
cargo run -p analog-cli -- gpu-info
# launch the editor
cargo run -p analog-gui
```

Camera RAW input (CR2, NEF, ARW, DNG, ...) is compiled in by default and pulls
the vendored LibRaw library, so those builds need a C++ compiler. Pass
`--no-default-features` to the CLI or GUI for a pure-Rust build without RAW.

In the editor, hold the **Compare (B)** control (or press the `B` / `\` key) to
see the original photo without effects while pressed. Scroll over the photo to
zoom in and out around the cursor, drag with the left mouse button to pan while
zoomed in, and use the **Fit** button or a double-click to reset the view.

## Documentation

- [`docs/index.md`](docs/index.md): project overview and documentation index
- [`docs/architecture.md`](docs/architecture.md): cross-cutting design
- [`docs/decisions/`](docs/decisions): decision records
