# Documentation index

`analog` is a local-first film grain editor built as a Rust workspace (library,
CLI, GUI).

## Overview

The engine reproduces four effect blocks, applied in a fixed order:

1. Film Color Shift
2. Grain
3. Halation
4. Print Simulation

A **recipe** is the complete, reproducible set of parameters for a render,
including a deterministic grain seed. Recipes are the unit shared by the
library, the CLI and the GUI, and serialise to the `.analog` XMP format.

## Documentation map

- [`architecture.md`](architecture.md): crate layout, the render pipeline, the
  data model, determinism and cross-platform strategy.
- [`decisions/`](decisions): records of significant decisions, each with
  context, rationale, alternatives and consequences.

## Package documentation

Each crate documents its own responsibilities in its `src/lib.rs` (or
`src/main.rs`) header. Module-level docs describe the individual passes once
they exist.

## Build and verify

```sh
cargo build --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```
