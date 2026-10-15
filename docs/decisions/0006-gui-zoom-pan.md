# 0006: Zoom and pan in the preview

Date: 2026-10-13
Status: accepted

## Context

The GUI showed the photo as a fit-to-window `image` widget
(`ContentFit::Contain`); there was no way to inspect it at 1:1 or closer. The
project todo asks for scroll-to-zoom and drag-to-pan in the preview.

## Decision

Draw the preview with an `iced` **canvas** (`canvas::Program`) and enable
`iced`'s `canvas` feature. The view is a small, pure
`ViewTransform { zoom, pan }` in `crates/analog-gui/src/view.rs`:

- `zoom` is a multiplier on the fit-to-window scale, clamped to `[1, 8]`.
- `pan` is the image centre's offset from the viewport centre, clamped so the
  scaled image never leaves a gap (and centred on an axis where it is smaller
  than the viewport).
- Scrolling zooms anchored at the cursor (the image point under the pointer
  stays put); left-dragging pans; a "Fit" button or a double-click resets.

The canvas program only translates mouse events into new transforms and
publishes them; the application stores the transform, so there is a single
source of truth and the math is unit-testable.

## Alternatives considered

- **A fixed-size `image` inside a `scrollable`.** Reuses existing features and
  gets clipping for free, but the `scrollable` owns the wheel to pan, so it
  cannot also zoom; zoom-to-cursor and centring-when-smaller are awkward.
  Rejected.
- **An `iced` custom `shader` widget.** Could keep the preview on the GPU, but
  is a large step beyond this change and would not change the interaction.
  Deferred.

## Consequences

- `iced`'s `canvas` feature is enabled; no new crate dependency is added (the
  canvas renderer is already part of `iced`).
- The canvas clips to its bounds, which is what makes panning look correct.
- Zooming upscales the 900 px preview proxy, so the image looks soft beyond
  about 1:1. Rendering a higher-resolution preview while zoomed is future work.
- The canvas receives mouse events over its whole area, including the margin
  around a fit-to-window photo; this is harmless.

## Sources

- `iced` 0.14 `canvas::Program`
  (https://docs.rs/iced/0.14.0/iced/widget/canvas/trait.Program.html) and
  `geometry::Frame::draw_image` (iced source, `graphics/src/geometry/frame.rs`).
