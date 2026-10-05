---
id: 1662
title: "A standalone Zephyr Rust leaf now PROBES, and nothing on its west road reads the sidecar — its pools stay at the Kconfig / crate defaults"
status: resolved
resolved_in: 2026-10-05
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

## Resolution, 2026-10-05

Built as the 2026-10-03 ruling directed: ONE derivation, two carriers.

* **The derivation** — `nros ws west-leaf-sizing --leaf <app> --build-dir
  <build> --output-cmake <fragment>`, which the Zephyr module runs at the head
  of `nros_resolve_knobs()` (`_nros_west_leaf_sizing()` in
  `zephyr/cmake/nros_cargo_build.cmake`). It resolves the application as a
  standalone Rust leaf (`leaf_settings::resolve_west`: a `system.toml` beside a
  `[package]` manifest whose board's road is west) and prints nothing for
  anything else, so entry images and C/C++ leaves are untouched. The inventory
  is the cargo-leaf road's own: `leaf_entity_env::inventory_for_leaf`, which
  now also reads the `metadata/` of every node package the leaf path-depends on
  inside itself (the edge `Workspace::discover` already follows to probe it),
  plus `with_leaf_monitor_rows`.
* **C lane** — `inv.to_cmake()` written at
  `<build>/nros/entity_inventory.cmake`, the fragment rung 3 already loads: the
  resolver gains a source, not a ladder.
* **Rust lane** — the descriptor, written by the cargo road's writer
  (`sizing_descriptor::write_for_leaf`, now taking the directory it writes
  under) at `<build>/nros/sizing/<image>.toml`, with the configure's
  `NROS_RUST_TARGET` as its triple. Named to the C lane as a resolved knob, and
  to the Rust lane through `NROS_BOARD_FACTS_ENV`.
* Every sidecar read (and `system.toml`) is a `CMAKE_CONFIGURE_DEPENDS`.

**A defect found on the way, fixed here.** `rust_cargo_application()` runs in
the APPLICATION's scope, and `NROS_BOARD_FACTS_ENV` was a normal variable of
the module's directory scope — so the `${NROS_BOARD_FACTS_ENV}` the
cargo-features patch injects into the Rust lane expanded to nothing. Measured
on the listener before this change: the configure printed "board facts ... 3
value(s) delivered to cargo" and `build.ninja` held zero `NROS_BOARD=`. Issue
0605's delivery had been inert on every Zephyr Rust image. `zephyr/CMakeLists.txt`
now publishes the composition (board facts + a leaf's descriptor) as a CACHE
entry, which an unset name falls back to in every scope; after: 3 `NROS_BOARD=`
rows on the Rust command.

**Measured** (`just zephyr build-one rust/<leaf> zenoh`, `native_sim/native/64`,
this worktree's Zephyr 3.7 workspace). The table at the top of this issue is
stale — the listener's `.bss` was already 564,640 on `main` today (executor
backing work since), so the baseline was re-measured:

| | `.bss` | `ZPICO_MAX_SUBSCRIBERS` (C defs / Rust const) | `MAX_CBS` | descriptor on cargo |
| --- | --- | --- | --- | --- |
| listener, `main` | 564,640 | 8 / 8 | 4 | none |
| listener, this change | **431,904** (−132,736) | 1 / 1 | 1 | yes |
| listener + a 2nd subscription in `node/src/lib.rs` only | 450,112 | 2 / 2 | 2 | yes |

The second row is the acceptance's other half: an edit to the node's code
moved both lanes with no declaration touched. All six Rust leaves build
(talker 296,736; service-server 292,704; service-client 293,792; action-server
297,984; action-client 304,640 `.bss`), `examples/zephyr/c/talker` builds
unchanged (8 subscribers, no descriptor), and `check-knob-delivery` passes on
the listener's build dir. Runtime against `rmw_zenohd`: listener heard 16/16
from `ros2 topic pub` and 26 from the Zephyr talker; service client got
`Result of add_two_ints: 5`; action client got the full Fibonacci result.

Test: `leaf_entity_env::tests::a_node_package_s_probe_is_the_leaf_s_inventory`
(red with the node-package read removed; its negative control holds an
unreferenced package's sidecar out).

**Payload sizes, checked rather than assumed.** The message-bound fragment
(`nros_message_bounds.cmake`) is still written only by `nros_find_interfaces()`,
which a Rust leaf does not call. That costs these images nothing: no size knob
is a C compile definition here (the C defs carry the counts above, and the
transport geometry), and the Rust lane's build scripts read the bound tables
from the descriptor this change names. Where the type is bounded they derive:
the service server's `AddTwoInts` request prices `SERVICE_BUFFER_SIZE` and
`SERVICE_INBOX_BYTES` at 24 (this build's `.config` states 1024). Where it is not they REFUSE, correctly
— the listener's `std_msgs/msg/String` has an unbounded `data` — and
`SUBSCRIBER_BUFFER_SIZE` stays at its default, on this road as on every other.
