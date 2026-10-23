# 0009: Grain type codes and defaults

Date: 2026-10-23
Status: accepted

## Context

The `.analog` reader and writer used the internal `GrainType` order for the
`analog:grainType` attribute: 0 Coarse, 1 Smooth, 2 Fine. The original settings
schema, which `.analog` recipes mirror, uses 0 Fine, 1 Coarse, 2 Smooth. A
preset from that schema therefore loaded with the wrong generator, and a fresh
image started on the coarsest generator with size 50 and softness 50, which
produced blobs rather than fine grain.

## Decision

Bump `SCHEMA_VERSION` to 2 and align the wire codes with the settings schema:

- 0 Fine (default), 1 Coarse, 2 Smooth;
- the internal enum order is unchanged;
- version 1 files are migrated while parsing, using `analog:schemaVersion`;
  a missing version is treated as 1;
- fresh defaults become Fine, size 30, softness 15, colour 16, highlights 25,
  mids 38, shadows 64;
- The slider now called Clumping no longer adds dust specks and scratches;
  those were a separate effect.

## Migration

Version 1 codes map as 0 Coarse to 1, 1 Smooth to 2, and 2 Fine to 0. No file
rewrite is needed; the reader converts on load and the next export writes
version 2. Saved recipes carry every field, so changing defaults does not alter
them.

## Alternatives considered

- **Keep the internal order and accept only our own presets.** Rejected: the
  project mirrors the settings schema, and importing its presets is expected to
  produce the same generator.
- **Change the internal enum order instead of the wire codes.** Rejected: it
  changes serde output and every match site for no user-visible benefit.
- **Keep the coarse defaults and only change them in the GUI.** Rejected: the
  CLI and library would still start on the worst-looking combination.

## Consequences

- New `.analog` files carry `analog:schemaVersion="2"` and the new codes.
- Version 1 `.analog` files load correctly.
- External tools that read `analog:grainType` must apply the version mapping.
- The schema version is now independent of the namespace URI, which stays
  `urn:analog:recipe:1.0`.
