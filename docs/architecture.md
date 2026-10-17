# Architecture

## Crate layout

A Cargo workspace separates the engine from its front ends so the GUI's heavy
dependencies never leak into the library or CLI.

| Crate | Role |
| --- | --- |
| `analog-core` | Library: recipe/params data model, image I/O, and (later) the `wgpu` render engine and WGSL shaders. |
| `analog-cli` | `clap`-based command line front end. |
| `analog-gui` | `iced` desktop front end. |

`analog-core` is the single source of truth for the parameter model. Both front
ends depend on it and must not duplicate parameter ranges or defaults.

## Data model

The model is data-first: plain structs with `serde` derives and documented
ranges, so a recipe can be serialised, compared and clamped without any engine
present.

- `params::{GrainParams, HalationParams, PrintParams, FilmParams}`: one struct
  per effect block, each with a `Default` that reproduces the shipped neutral
  values and a `clamped()` method.
- `params::{GrainType, PaperType, FilmStock}`: the enumerations. `FilmStock`
  exposes a stable snake_case `id()` for recipes and the CLI.
- `recipe::Recipe`: the four blocks plus `grain_seed`, `name` and
  `schema_version`. `Recipe::neutral()` is an explicit pass-through.

Ranges and defaults are documented per field.

## Render pipeline

The documented order is Film Color Shift, then Grain, then Halation, then Print
Simulation, ending in a readback. The engine will be a
multi-pass `wgpu` compute pipeline over `rgba8unorm` textures for the first
milestone:

1. `analyze_luminance`: GPU atomics produce the image min/max luma; the CPU
   derives the three adaptive-zone multipliers.
2. `analyze_hue_clusters`: four circular-hue centroids into a fixed-point buffer.
3. `film_stock_transform`: our own parametric color model (implemented).
4. Grain: a three-stage pipeline (`scatter`, `blur`, `apply`).
5. Halation: `extract`, `downsample`, `upsample_add`, `composite` (implemented).
6. `print_simulation_transform` (implemented).
7. Readback to `analog_core::Image`.

## Grain model

The grain is a three-stage GPU pipeline (`scatter`, `blur`,
`apply`), and documented:

- **Noise**: an integer hash (Wellons' lowbias32) over a lattice, bilinearly
  interpolated, two octaves. Integer hashing is what makes a `grain_seed`
  reproducible across backends.
- **Size**: `grain_amount` and `grain_type` set the noise feature size in pixels
  (coarse 3-12 px, smooth 2.5-10 px, fine 1-4 px).
- **Zones**: `luminance_shadows/mids/highlights` scale three overlapping
  smoothstep weights over luma, fading to zero at the deep shadows and near
  white.
- **Color**: per-channel noise offsets, blended toward monochrome by
  `grain_color` and directed by `color_shift`.
- **Softness**: a separable box blur (horizontal then vertical) of the grain
  field, radius from `roughness`; the composite applies a mild contrast
  compensation because the field is already low-frequency.
- **Dissolution**: clumps the field and, above 10 and 20, adds dust specks and
  scratches.
- **Focus**: with a depth map, in-focus pixels use the sharp field and defocused
  pixels use the blurred field, mixed by `depth_of_field_intensity`.

## Halation model

A Kawase-style bloom, documented:

- **Extract**: pixels above `halation_threshold` (0-255) with a soft knee,
  keeping the bright colour.
- **Downsample**: a box-filter mip chain (1/2, 1/4, ... up to 1/64).
- **Upsample**: tent (bilinear) upsample with additive accumulation.
- **Radius**: `halation_radius` selects how many mip levels contribute, so a
  larger radius produces a wider bloom.
- **Composite**: 70 percent additive plus 30 percent screen, a yellow-to-red
  tint whose hue is `clamp(color_tone/100 + (color_shift - 50)/100, 0, 1)`
  (desaturated by `halation_color_intensity`), highlight protection, and a
  dither. The tint is exposed as `analog_core::tint_rgb` so the GUI's gradient
  swatch and the render stay in sync.
- `light_spill_intensity` is folded into the composite intensity.
- **Depth**: with a depth map, `halation_background_focus` suppresses the glow on
  the in-focus subject and `halation_background_gain` boosts it in the
  defocused background.

## Print model

A per-paper tone map applied last:

- **Shadow floor**: matte 0.118, neutral 0.06, glossy 0.005, scaled by
  `print_sim_paper_intensity` (which is why the full-strength values only appear
  at paper intensity 100).
- **S-curve**: a smoothstep contrast lift, amount per paper (matte 0.10, neutral
  0.15, glossy 0.20).
- **Highlight shoulder**: identity to a knee, smoothstep to a white point
  (matte 0.45/0.72, neutral 0.55/0.78, glossy 0.65/0.84), gentle compression
  beyond. `print_sim_highlight_rolloff` lowers the knee.
- **Paper tint**: warm cream to cool cyan-blue, from `print_sim_paper_tint`.
- `print_simulation` blends the whole effect with the input.

Approximation: the highlight shoulder uses a smoothstep.

## Film model

A parametric emulation; no third-party LUTs. Each stock has a
`Profile` (in `src/film.rs`) applied in `shaders/film.wgsl`:

- **Lift/gain/gamma** per channel.
- **Saturation** around BT.709 luma.
- **Shadow/highlight tint**, where the warm shadow tint approximates the C-41
  orange mask.
- **Black-and-white** stocks use a channel mixer and a paper tone.

Deferred: per-image hue-cluster narrowing and baked 33-cubed LUTs. Our 19
profiles are hand-authored approximations.

## GPU scaffold

`gpu::GpuContext` owns one `wgpu` device and queue, requested with a
high-performance preference. `passes::GpuImage` is a pixel buffer that lives on
the GPU; `passes::ImageComputePass::run_gpu` takes `&GpuImage` inputs and returns
a `GpuImage` with no readback, and `GpuImage::read` is the single synchronisation
point. `Renderer::render` uploads the source once, chains the passes on the GPU,
and reads back once. Passes compile lazily on first use, and only the pixel
format actually used (see
[`decisions/0005-gpu-resident-pipeline.md`](decisions/0005-gpu-resident-pipeline.md)).

## Determinism

Grain must be reproducible: the same `grain_seed` has to yield byte-identical
grain on every run and every backend. This rules out float-noise generators.
Grain is produced by an integer-hash PRNG in WGSL seeded from `grain_seed` and
the pixel coordinate, and guarded by golden-image tests.

## Image I/O and bit depth

`Image` holds RGBA pixels in one of two [`PixelFormat`](image::PixelFormat)s:
`Rgba8` (4 bytes per pixel) or `Rgba16` (8 bytes per pixel). Decode preserves
16-bit PNG/TIFF; encode writes 16-bit for those formats and falls back to 8-bit
for JPEG and WebP. JPEG output composites over white because the format has no
alpha channel; WebP keeps alpha and is written lossless.

On the GPU each pixel is packed into `u32` words: one word for `Rgba8`
(`r | g<<8 | b<<16 | a<<24`), two for `Rgba16` (`r | g<<16`, `b | a<<16`).
Shaders that read or write the image declare `//@IMAGE_LOAD` / `//@IMAGE_STORE`
markers, and `shaders::image_shader` substitutes the 8- or 16-bit implementation
when the pipeline is compiled; each such pass holds both variants. Auxiliary
buffers (grain field, bloom chain, depth map) are always 8-bit.

`Renderer::render` preserves the input depth: an 8-bit source produces 8-bit
output and a 16-bit source produces 16-bit output.

PSD files are imported through the `psd` crate as their flattened 8-bit RGBA
image; the crate can panic on some variants, so the call is wrapped in
`catch_unwind`. PSB and PSD/PSB export are not supported.

Camera RAW files (CR2, NEF, ARW, DNG, ...) are developed to 16-bit sRGB RGBA
through LibRaw on the `raw` feature (decode only), so they enter the pipeline
like any other 16-bit input; RAW is never an output format. See
[`decisions/0007-camera-raw-input.md`](decisions/0007-camera-raw-input.md).

## Cross-platform

`wgpu` targets Vulkan, Metal and DirectX 12 from one codebase; `iced` renders
the UI, and user paths go through the `directories` crate (see the depth model).
The GUI hides the Windows console in release builds (via `windows_subsystem`).
Development happens on Linux/Vulkan; the release workflow also builds Windows
and Apple Silicon macOS artifacts. Platform-specific build requirements are
recorded in `.cargo/config.toml` and the README.

## Recipes

A recipe is the shared unit of the library, CLI and GUI. It serialises to JSON
(via `serde`) and to the `.analog` XMP format (via `recipe_io`), and the CLI
`preset` subcommand converts between them. The XMP attribute names mirror the
internal schema so a preset round-trips faithfully; import is lenient.

## Depth model

Focus-based grain and depth-aware halation use a monocular depth estimate from
Depth Anything V2 Small (`onnx-community/depth-anything-v2-small`, Apache-2.0)
run through ONNX Runtime (`ort`). The model is not shipped: the int8-quantised
ONNX file (~27 MB) is downloaded on demand into the platform cache directory via
the `directories` crate (`~/.cache/analog` on Linux, `~/Library/Caches/analog` on
macOS, `%LOCALAPPDATA%\analog\cache` on Windows) and loaded from there
afterwards.

`DepthModel::predict` squashes the image to the model's 518x518 input, applies
ImageNet normalisation, runs inference, normalises the output to 0..255 (higher
is nearer), and resamples it back to image resolution as a `DepthMap`. The GPU
passes read the depth from an RGBA8 buffer; when no depth map is supplied the
host forces the depth controls to zero so the shaders never read the placeholder.

## GUI

The `analog-gui` crate is a single-window `iced` app. It holds a `GpuContext`
and a `Renderer` (whose passes compile lazily), keeps the original image plus a
full preview proxy and a coarse proxy, and re-renders on parameter changes: the
coarse proxy immediately, the full proxy after a 60 ms debounce. Export renders
the original at full resolution and writes it with `Image::encode`.

The preview is an `iced` **canvas** (`canvas::Program`) rather than a plain
image widget, so it can be scaled and translated. Scrolling over the photo zooms
in and out anchored at the cursor, dragging with the left button pans while
zoomed, and a "Fit" button or a double-click resets the view. Zoom is clamped to
`[fit, 8x]`, and the pan is clamped so the photo never leaves a gap in the
viewport. The view (zoom and pan) is a small, pure `ViewTransform` in
`crates/analog-gui/src/view.rs`; the canvas program only turns mouse events into
new transforms. Zooming upscales the 900 px preview proxy, so fine detail is
limited until a higher-resolution preview on zoom is added. This is what
requires `iced`'s `canvas` feature.

A **hold-to-compare** control shows the un-effected original while held: press
the "Compare (B)" control, or hold `B` or `\`, to display the raw source
proxy instead of the render. It is built with `mouse_area` (`on_press` /
`on_release`) and a keyboard subscription, and is inert until an image is
loaded. The zoom and pan survive toggling compare.

Depth estimation runs in a background `Task` (`Message::DepthReady`): the model
is shared as `Arc<Mutex<DepthModel>>` and moved into the task, so the UI never
blocks. The panel shows a persistent `info` line (the loaded image) and a
transient `status` line (progress and errors, empty when idle).

Because `iced` 0.14 pins `wgpu` 27 while the engine uses `wgpu` 30, the two do
not share a device. The engine renders to CPU pixels and hands `iced` an
`image::Handle` (see `decisions/0003-gui-iced.md`). This is simple and robust at
preview sizes; a shared-device or custom-shader path is a possible future
optimization.
