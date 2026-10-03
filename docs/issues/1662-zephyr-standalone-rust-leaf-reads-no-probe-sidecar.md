---
id: 1662
title: "A standalone Zephyr Rust leaf now PROBES, and nothing on its west road reads the sidecar — its pools stay at the Kconfig / crate defaults"
status: open
type: tech-debt
area: [build, tooling, examples]
severity: low
found: 2026-10-03
related: [1603, 1407, 1653, 1265, 1061, 1288, rfc-0100, phase-470]
---

## What

Issue 1603's Zephyr half split each `examples/zephyr/rust/*` leaf into a
host-buildable NODE package (`<leaf>/node/`) and the thin west image half, so
the metadata probe answers for all six (`node/metadata/<component>.json` --
measured, e.g. the listener's sidecar holds its one subscription). That closes
the "no producer" half of 1603's acceptance. It does not close the other half,
"pools derived": nothing on a standalone Zephyr Rust leaf's road reads that
sidecar.

## Evidence

`examples/zephyr/rust/listener`, `native_sim/native/64`, zenoh, west-built
through the repo's own `build-one` recipe before and after the split:

| | `.bss` of `zephyr.exe` | `ZPICO_MAX_SUBSCRIBERS` in the build |
| --- | --- | --- |
| before (no sidecar) | 857,144 | 8 (Kconfig) |
| after (sidecar: 1 subscription) | 857,144 | 8 (Kconfig) |

The image is byte-for-byte the same size: the probe's facts reach no knob.

## Why

The road is `rust_cargo_application()` (zephyr-lang-rust) over the leaf's own
`Cargo.toml`, with the Zephyr module's knob resolver supplying the C-side
values from Kconfig. A Rust Zephyr leaf calls no `nano_ros_entry()`, so
neither the cmake entity-facts fold (`nros_record_entity_facts`, which reads a
MODEL) nor the sizing-descriptor naming that issue 1407 gave a west ENTRY
reaches it, and the cargo-leaf road that DOES read a leaf's probe
(`leaf_entity_env`, written into `build/<image>/nros-cargo.toml`) is not the
road west runs. CLAUDE.md already records the standalone Zephyr leaf as
reaching no descriptor producer; this is the same gap one input earlier, now
that the input exists.

## Direction (not decided)

Feed the leaf's probe-derived facts to the west road the way issue 1407 fed a
west entry's descriptor: `nros sync` already writes the leaf's facts for the
cargo road, so the question is the carrier into `rust_cargo_application()`'s
cargo invocation (which inherits no `set(ENV{})`, issue 0460) and into the C
lane's knob resolver. Acceptance: the listener above built with its sidecar
sizes `ZPICO_MAX_SUBSCRIBERS` (and the executor tables) from the one
subscription, and an edit adding a subscription to `node/src/lib.rs` moves
them with no declaration touched.

## Ruling, 2026-10-03 — what delivering it takes, and why it is not a one-carrier fix

Investigated beside issue 1653's west half (which put the board heap and the
Cyclone `[types]` facts on the west road for an ENTRY image). The standalone
Zephyr Rust leaf is a different road, and closing it needs two carriers, not
one, because the pools it sizes live on both sides of the build:

1. **The C lane.** zenoh-pico and the session tables are compiled by the
   Zephyr module (C), sized by the module's knob resolver
   (`zephyr/cmake/nros_cargo_build.cmake`, `nros_resolve_knobs()`). Its
   derived rung (rung 3) reads ONE input: the entity-inventory fragment
   `${CMAKE_BINARY_DIR}/nros/entity_inventory.cmake`
   (`_nros_load_derived_entity_inventory`), which only `nano_ros_entry()`
   writes. A Rust leaf calls no `nano_ros_entry()`, so rung 3 is empty and
   Kconfig answers (`ZPICO_MAX_SUBSCRIBERS` 8 in the issue's table).
2. **The Rust lane.** `rust_cargo_application()` (the patched
   `modules/lang/rust/CMakeLists.txt`) runs cargo with `cmake -E env
   ${NROS_BOARD_FACTS_ENV} cargo ...` plus `EXTRA_CARGO_ARGS`; it inherits
   no `set(ENV{})` (issue 0460), and the cargo-leaf road's carrier -- the
   `[env]` table `nros sync` renders into `build/<image>/nros-cargo.toml` and
   hands cargo with `--config` -- is never named on this command (the west
   lane has no `--config` seam, issue 1288).

**Direction (decided, not yet built):**

* ONE derivation: the leaf's probe sidecars (`node/metadata/*.json`) through
  the same reader the cargo-leaf road uses (`leaf_entity_env`), rendered by a
  CLI verb into the fragment shape rung 3 already reads -- the
  `NROS_DERIVED_*` names `nros_entity_inventory_knobs_file()` defines -- so
  the resolver gains a SOURCE, not a second ladder. Written by `nros sync` for
  a leaf whose `[image.*] board` is a Zephyr board, and loaded by
  `nros_resolve_knobs()` when no entry fragment exists (an entry's fragment
  still wins, by construction: an entry image is not a standalone leaf).
* The Rust lane gets the same facts through the descriptor the leaf road
  already writes (`write_for_leaf`): `NROS_SIZING_DESCRIPTOR` added to
  `NROS_BOARD_FACTS_ENV`'s composition for a leaf, so every descriptor-first
  build script (nros-node, nros-rmw-zenoh, nros-zpico-build, nros) reads it --
  the same path issue 1407 opened for a west ENTRY.
* Acceptance stays the issue's: the listener sizes `ZPICO_MAX_SUBSCRIBERS`
  and the executor tables from its one subscription, and adding a
  subscription to `node/src/lib.rs` moves them with no declaration touched.

**Why not in this round.** The files it needs are `examples/zephyr/rust/*`
(owned by the concurrent probe/census work) and a new leaf-side writer in
`nros sync`; and the measurement needs the Zephyr Rust toolchain lane
(`build-one`). The west ENTRY road's carriers that 1653 completed are the
template, and nothing here changes their shape.
