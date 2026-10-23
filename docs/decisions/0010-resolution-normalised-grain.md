# 0010: Resolution-normalised grain

Date: 2026-10-23
Status: accepted

## Context

Grain used fixed pixel sizes: a 3-12 px noise cell and a 0-12 px box blur,
whatever the image size. The GUI renders a 900 px proxy, so the preview read
much coarser than the full-resolution export at fit. The same softness value
also meant a different fraction of the frame at every output size.

Saved recipes need a stable look, and the preview should predict the export
without rendering at full resolution.

## Decision

Normalise grain size and softness to a 3000 px long side:

- `reference = min(3000, source_long_side)`;
- grain coordinates are `(pixel / processed_long_side) * reference / cell`;
- `cell` is a reference-pixel feature size: coarse 0.75-6, smooth 0.6-6.8,
  fine 0.3-3.4;
- softness is a Gaussian half-width of 1-8 px at the reference, multiplied by
  `processed_long_side / 3000`; values below 5 leave the field sharp;
- the grain field and blur intermediates are `Rgba16`, and 8-bit output gets
  half a code step of deterministic dither;
- `Renderer::render_with_reference` accepts the source long side. The GUI
  passes it for proxy previews; export and the CLI use the processed size.

## Alternatives considered

- **Keep fixed pixel sizes.** Rejected: preview and export disagree and the
  same recipe changes with proxy size.
- **Make grain a fixed fraction of the frame at all sizes.** Rejected: a 900 px
  image would show very coarse grain relative to a 24 MP image of the same
  scene, and small exports would look wrong.
- **Always render full resolution for the preview.** Rejected: too slow for
  interactive sliders.
- **Render the preview with supersampling.** Useful later for subpixel grain,
  but it multiplies preview cost and does not fix the size mapping.

## Consequences

- A proxy preview of a large image can generate grain below one processing
  pixel, so preview grain is stronger than the downscaled export until the
  generator band-limits itself. Measured on the calibration harness: preview
  RMS 2.01/255 versus export/3 RMS 0.98/255 for the fine recipe.
- The public `Renderer::render` signature is unchanged; callers that render
  proxies opt into `render_with_reference`.
- The 16-bit field doubles auxiliary memory and costs two words per pixel in
  the scatter, blur and apply passes.
- The reference value is a heuristic. Validate 900, 3000 and 6000 px renders
  before treating it as fixed.
- Phase 3 replaces value noise with a band-limited generator, which should
  close the preview/export amplitude gap.
