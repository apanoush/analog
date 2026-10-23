# 0011: Adaptive grain response and luminance-preserving chroma

Date: 2026-10-23
Status: accepted

## Context

Grain zones used absolute luminance. A low-contrast image therefore placed all
of its grain in one zone, and the zone weights did not form a partition of unity
in the deep shadows. Chroma grain also used channel samples whose deviations did
not cancel, so changing Color altered local brightness as well as hue.

## Decision

- Compute the input image's BT.709 luminance range on the CPU before upload.
  Ignore fully transparent pixels. Fall back to `0..1` when the range is
  degenerate (below 0.01). Raw minimum and maximum are used, without percentile
  clipping.
- Map pixel luma into the adaptive range and weight zones as a partition of
  unity: shadows below 0.30, mids plateau to 0.55, highlights fading in from
  0.55 to 0.85.
- Fade the darkest 15 percent toward 0.05, relaxing the fade when the Shadows
  multiplier is above neutral.
- Build chroma from two opposite samples of the same grain field. Choose the
  green deviation so the three channel deviations sum to zero under BT.709
  luma, which preserves local brightness. `color_shift` rotates the offset
  direction and `grain_color` scales the deviations.

The range travels in the two former padding words of the existing grain
uniform; no new buffer or pass is needed.

## Alternatives considered

- **GPU luminance analysis with a readback.** Rejected: adds a synchronisation
  point and a new pass for two floats. The CPU scan is linear and cheap, and the
  image is already on the CPU before upload.
- **Percentile clipping.** Rejected for now: needs a histogram and a policy for
  small images. Raw min/max is predictable and matches the reference behaviour.
- **Keep absolute zones.** Rejected: low-contrast images respond in the wrong
  tonal region, and the old weights dipped in the deep shadows.
- **Independent per-channel noise.** Rejected: produces coloured luminance
  speckle and makes Color change brightness.

## Consequences

- Existing recipes render differently because the zone sliders now span the
  image's actual tonal range. No schema change is needed.
- A flat or near-flat image falls back to absolute luminance and behaves as
  before.
- Every grain render pays one CPU pass over the input image. Preview proxies are
  small; a full-resolution export scans the source once.
- Color 16 is very subtle by construction. Chroma at Color 80 remains visible
  without shifting mean luminance (measured shift below 0.01/255).
- The zone boundaries are constants, matching the reference. They can become
  uniforms later if the UI exposes them.
