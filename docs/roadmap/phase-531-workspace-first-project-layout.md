# Phase 531 — Workspace-first project layout: bringup-only `system.toml`, one build road, the leaf shape retired

**Status (2026-11-03). OPEN — not started; W1 (the measurement) first.**
Implements [RFC-0121](../design/0121-workspace-first-project-layout.md) D1 to
D8, with its amendments to RFC-0098, RFC-0116 D6, RFC-0026/0066, RFC-0100,
RFC-0118 D2 and RFC-0120.

Depends on:

- [phase-528](phase-528-run-on-host-and-emulator.md) W4, for `nros run`, whose
  target argument W4 here extends;
- [phase-529](phase-529-flash-monitor-bench.md) W3, for `nros flash`, the same;
- [phase-530](phase-530-nros-configure-single-pass.md), whose `nros configure`
  becomes the place W3's launch generation runs. W3 can land on today's `nros
  sync` first and move with phase-530;
- [phase-525](phase-525-adoption-front-doors.md) W4 (the timed first chapter):
  it scaffolds a single package today. Its chapter text and probe move with W8
  here, so the two phases agree on the scaffold before either rewrites the
  chapter.

---

## Why

A standalone package is built by its own road, and that road, not the launch
file, is what every later feature had to be written for twice: the selection
crate, the `--from-leaf` sizing producer, `write_for_leaf`, three bringup
synthesisers and the shape checks in `cmd/build.rs`. RFC-0121 makes every
project a workspace with `system.toml` only in a bringup, and keeps the
single-node experience by a scaffold and a node-name target rather than by a
second shape.

## Work items

### W1 — Measure what retires

Before anything is removed, list every function, file and gate the standalone
shape reaches, and say for each whether a workspace image also reaches it (some
`leaf_*` code serves workspace entries too). Inputs: the call graph from
`leaf_system`, `leaf_settings`, `leaf_kconfig`, `leaf_entity_env`,
`west_leaf_sizing`, `sizing_descriptor` (`write_for_leaf`, `--from-leaf`), the
three synthesisers, and the shape checks in `cmd/build.rs` and `cmd/ws.rs`.

**Acceptance:** a table in this doc with each item, its line count, whether the
workspace road reaches it, and the W7 step that deletes it. It also records how
many tracked `system.toml` sit beside code (223 on 2026-11-03), split by tree
(`examples/`, `packages/testing/`, templates).

### W2 — The rule and its gates

- `check-system-toml-in-bringup` (fast line) refuses a tracked `system.toml`
  beside `Cargo.toml` or `CMakeLists.txt`. It is a ratchet whose allowlist is
  generated from W1's list and may only shrink.
- `check-node-package-selection-free` refuses a selection crate or board crate
  named in a node package's manifest, with the same ratchet.
- `nros configure` / `nros sync` warn on a `system.toml` beside code, naming
  `nros dev migrate-bringup`. The warning becomes a refusal in W7.

**Acceptance:** both gates in `just check fast` with mutation cases (a new
`system.toml` beside a `Cargo.toml` fails; removing one allowlisted site
without deleting its row fails the ratchet).

### W3 — A missing launch file is generated, in any bringup

Move `synthesise_leaf_launch` to read bringup packages: a bringup with no
`launch/` gets `<ws>/build/nros/launch/<bringup>/system.launch.xml` from its
`[[component]]` rows. `nros plan explain` marks it generated. A bringup with
neither is refused, naming both remedies.

**Acceptance:** a fixture workspace whose bringup has no `launch/` resolves the
same SystemModel as the same workspace with the generated file copied into
`launch/` (models compared byte for byte); deleting `launch/` returns to
generation.

### W4 — `nros new` writes a workspace; `nros run <node>`

- `nros new <ws> --board <b> --template <t>` writes `<ws>/src/<node>/` (a
  library node package) and `<ws>/src/<node>_bringup/` (`package.xml`,
  `system.toml`, no `launch/`). Inside an existing workspace, `nros new <node>
  [--to <bringup>]` adds a package and optionally a `[[component]]` row.
- The `run` / `flash` / `monitor` / `build --packages-up-to` target resolver
  gains RFC-0121 D5's rungs, with `--why` naming the rung and every ambiguous
  case refused with the spellings that would resolve.

**Acceptance:** `nros new sensor_ws --board mps2-an385-baremetal --template
talker && cd sensor_ws && nros run sensor_node --expect 'Publishing: 0'`
passes on QEMU. A workspace with two bringups naming one node refuses `nros run
<node>` and lists both.

### W5 — One build road

Every image builds through the generated entry. A node package is a library;
its `main` is generated. The hooked roads of RFC-0120 D8 (west, CMake) build
the generated entry, which records its bringup.

**Acceptance:** each platform family's single-node example builds and runs
through the workspace road, with images whose section sizes match the
standalone build's within the phase-445 tolerance. `rg selection_crate_name`
finds only the code W7 deletes.

### W6 — Migrate the tree

- `nros dev migrate-bringup <dir>`, the one-shot of RFC-0121 D8. It is tested
  on a copy of each shape: Rust, C, C++, Zephyr west, a hand-written `main`, and
  a fixture row.
- Run it over `examples/`, `packages/testing/` and the templates, in batches by
  platform directory so each PR's fixture rebuild stays one lane. Each batch
  rebuilds its lane's fixtures and runs the lane (`just test <platform>`).
- `fixtures.toml` rows keep their coordinates (`row_coord()` unchanged), so
  lane membership does not move; `matrix_fixture_coverage.rs` and
  `check-west-leaf-vocabulary` stay green per batch.
- The ratchet allowlist from W2 shrinks with each batch.

**Acceptance:** the W2 allowlists are empty. `just ci` (tier 1) and `just ci
matrix` (tier 2) are green on the migrated tree, with fixture counts unchanged
from before W6.

### W7 — Retirement

Delete what W1 listed as reachable only from the standalone shape, in this
order, each step with its own PR and gate run:

1. the `[deploy.*]` manifest metadata and `synthesise_self_bringup`;
2. `synthesise_self_model` and the launchless-package detection in
   `launch_synth.rs`;
3. the `<pkg>_nros_selection` crate (`leaf_system::selection_crate_name`,
   `LEAF_SELECTION_DIR`) and the macro path that emits it;
4. `--from-leaf`, `write_for_leaf` and the leaf rows of
   `NanoRosSizingDescriptor.cmake`; RFC-0100's road table loses its leaf rows;
5. the leaf-only halves of `leaf_settings`, `leaf_system`, `leaf_kconfig`,
   `leaf_entity_env`, `west_leaf_sizing`, and the shape checks in `cmd/build.rs`
   and `cmd/ws.rs`;
6. the `[overrides]` fallback to a root `system.toml` (RFC-0119 D4 as
   amended); `nros.toml [workspace.overrides]` is the only home;
7. the gates that police the retired shape (`check-leaf-deployment-spelling`,
   the leaf arm of `check-msg-dep-is-path`, and the like), each confirmed by W1
   to guard nothing that remains;
8. W2's warning becomes a refusal.

Each deletion is listed in `book/src/reference/retired-names.md` with its
replacement.

**Acceptance:** W1's table has every row marked deleted or kept-with-reason;
`just ci gate` and `just ci matrix` green after the last step.

### W8 — Book rewrite

The book teaches one shape, the workspace, from the first page.

| Page | Change |
| --- | --- |
| `getting-started/first-project.md` | `nros new <ws>` → `nros run <node>`; shows the workspace tree with its two packages and says the reader need not open the bringup yet (RFC-0116 D6 as amended) |
| `getting-started/anatomy.md` | explains node package, bringup and the generated launch file (`nros plan explain`) |
| `getting-started/first-node-{rust,c,cpp}.md` | node packages as libraries; no hand-written `main` |
| `getting-started/workspace-from-app-node.md`, `workspace-node-pkgs.md`, `workspace-bringup.md`, `workspace-entry-pkg.md` | merged into one "Growing your workspace" path: add a node package, add a component row, add `launch/` when a launch argument is needed, add an image |
| `getting-started/workspace-cpp.md`, `workspace-mixed-language.md`, `workspace-languages.md` | updated to the bringup-only `system.toml` and the node rung |
| platform chapters (`freertos.md`, `integration-zephyr.md`, `integration-nuttx.md`, `threadx.md`, `bare-metal.md`, `arm-fvp.md`, `px4.md`) | every example path and command becomes a workspace; `nros run <node>` / `nros flash <node>` |
| `porting/*.md`, `getting-started/build-as-subdirectory.md`, `vendored-tree.md` | a custom board is a board package under `src/` (RFC-0115 D3, RFC-0087 D6); projects that own their build stay as they are (RFC-0121 D6 scope) |
| `internals/creating-examples.md` | an example is a copy-out workspace (RFC-0026 as amended) |
| `reference/cli.md`, `build-commands.md`, `retired-names.md` | the target rungs, the retired flows and names |
| `troubleshooting-first-10-min.md` | the `system.toml`-beside-code refusal and its remedy |

- The `probe=NN` blocks of every rewritten page are re-run (`just probe
  bootstrap`, `just probe installed`, `just probe <platform>`), so the book's
  commands are executed, not just edited.
- Coordinate the first chapter with phase-525 W4 (the timed chapter), so one
  rewrite lands.

**Acceptance:** `rg 'system.toml' book/src` finds no page that places it beside
code; `just book` builds; every touched page's probe track is green.

### W9 — Docs and pointers

- CLAUDE.md: the lines describing standalone leaves (RFC-0098 single-package
  shapes, the `<pkg>_nros_selection` crate, the leaf `.cargo/config.toml`
  include) are replaced by one-line pointers to RFC-0121.
- AGENTS.md, `examples/README.md` and `examples/workspaces/README-layout.md`:
  the workspace shape, and how to add an example.
- `docs/reference/canonical-build-path.md`: the leaf road leaves the map.

**Acceptance:** `just check fast` green (the doc-reference and identifier gates
cover these files).

## Acceptance for the phase

- No tracked `system.toml` sits beside code, enforced on the fast line, with no
  allowlist.
- `nros new <ws> --board <b> --template talker && nros run <node>` is the first
  chapter, and it passes its probe on QEMU.
- The standalone road's code is deleted (W1's table, all rows resolved).
- Tier 1 and tier 2 are green on the migrated tree.

## Not in scope

- Projects that own their build and have no `system.toml` (guest Zephyr
  applications, CMake `add_subdirectory` consumers, vendor-SDK projects).
- The open questions of RFC-0121 (bringup naming in the scaffold;
  rust-analyzer with the generated entry as the cargo root). W4 picks the name
  and records the choice in RFC-0121.
