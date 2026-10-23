# 0012: Cheap grain fusion approximation

Date: 2026-10-23
Status: accepted

## Context

The plan's Phase 5 called for reimplementing Grain Fusion as a base-image
scatter pass: jittered particles, image sampling around each particle, a blur
and a coverage blend. That is the largest change in the grain plan, needs a new
pass or a much heavier blur, and mainly changes how grain sits against edges and
micro-detail. It does not improve grain size, softness, spectrum or colour,
which were the original fidelity gaps. A naive scatter also risks softening
photographs and introducing halos.

## Decision

Use two cheap approximations and rename the control to match what it does:

- **Edge-aware amplitude**: a central difference on luma reduces grain by up to
  30 percent on strong edges, so micro-detail stays readable.
- **Partial soft light**: up to 25 percent of the grain is mixed through W3C
  soft light instead of pure addition. The Clumping slider drives this fraction
  together with the low-frequency density modulation.
- **Rename**: the UI label changes from Grain Fusion to Clumping. The recipe
  attribute remains `dissolutionAmount`, so saved recipes still load.

The full structural scatter remains deferred.

## Alternatives considered

- **Full particle scatter now.** Deferred: high complexity, uncertain gain and
  a real risk of softening the image.
- **Rename only, no blend change.** Rejected: leaves no integration with the
  base image and makes the control purely a density modifier.
- **Additive grain only.** Rejected: grain stacks on strong edges and can look
  pasted on.
- **Overlay blend.** Rejected in favour of soft light, which is gentler at both
  ends of the tonal range.

## Consequences

- No new pass and no new buffer. The apply shader adds four neighbour luma
  loads per pixel.
- The effect is intentionally subtle. At Clumping 100 the soft-light mix is
  25 percent; most of the slider's visible effect is still density clumping.
- Existing recipes keep their values but the slider label and the high end of
  its response change.
- Structural scatter stays in `agent_artifacts/backlog.md` if it is ever wanted.
