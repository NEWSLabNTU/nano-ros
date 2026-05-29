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

- `zephyr-byo/` — Zephyr "bring-your-own west workspace" starter
  (Phase 205.A): a `west.yml` pinning a tested Zephyr + the nano-ros
  module import, plus a zenoh `std_msgs/Int32` talker app skeleton.
  Source for the standalone `nano-ros-zephyr-example` repo; see its
  `README.md` for the `west init → nros setup → west patch → west
  build → run` quickstart (e2e-verified, Phase 202).
