# examples/templates/

Multi-platform copy-out templates — Pattern A workspace layouts
and similar scaffolds that don't belong to a single
`<plat>/<lang>/<rmw>/<example>` cell. Each subdirectory is a
standalone project you can copy into your own tree and customize.

## Contents

- `multi-package-workspace/` — mixed C + C++ + Rust packages
  sharing one nano-ros install via `CMAKE_PREFIX_PATH` and
  Cargo `[patch.crates-io]`. Demonstrates the Phase 123 Pattern A
  layout where each downstream workspace pins one nano-ros source
  checkout under `src/nano-ros/`.
