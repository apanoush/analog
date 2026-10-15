# 0008: Recipe preset format

Date: 2026-10-15
Status: accepted

## Context

Recipes (presets) are serialised as an XMP packet. They were previously written
using another product's namespace so presets could be exchanged with that
product in both directions. Carrying a third party's namespace and attribute
prefix in this codebase ties the format's identity to that product.

## Decision

Use our own namespace and prefix:

- namespace `urn:analog:recipe:1.0`, declared as `xmlns:analog`, with attributes
  prefixed `analog:` (for example `analog:intensity`, `analog:grainSeed`);
- conventional file extension `.analog`; a bare `.xmp` is still accepted on
  import, detected by the `analog:` prefix;
- public API `to_recipe_xmp` / `from_recipe_xmp`, constant `RECIPE_NS`.

Attribute names, ranges, defaults and the schema version are unchanged; only the
namespace URI and prefix differ from the previous format.

## Migration

The change is a pure text transform, so presets in the old format convert
losslessly: replace the old namespace URI with `urn:analog:recipe:1.0` and
rewrite the old attribute prefix to `analog:`. A helper script in the
development repository performs this for a list of files; it is not shipped,
because it necessarily contains the old namespace strings.

## Alternatives considered

- **Keep the previous namespace.** No migration, but the codebase keeps
  referencing the other product. Rejected.
- **Read both namespaces, write the new one.** Preserves import of old presets
  but keeps the old strings in the reader. Rejected for the public branch; the
  one-off migration covers conversion instead.

## Consequences

- Presets written in the old format no longer import; convert them once.
- Files we write are `.analog` and carry `xmlns:analog="urn:analog:recipe:1.0"`.
- The reader is prefix-based, so any XMP carrying `analog:` attributes loads.
