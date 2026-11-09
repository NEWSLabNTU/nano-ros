---
id: 2308
title: "A sparse Zephyr source tree reads as a provisioned west workspace"
status: resolved
type: bug
area: [ci, zephyr, testing]
severity: medium
found: 2026-11-06
resolved: 2026-11-09
related: [1387, phase-543]
---

**Home: [phase-543](../../roadmap/phase-543-nextest-gates.md).**

## Measured

CI's fast job provisions Zephyr's C++ minimal-libc headers with a sparse clone
(`gate.yml`, "Provision the sources the fast-line source gates read"):

    git clone --depth 1 --filter=blob:none --sparse --branch "$rev" \
      https://github.com/zephyrproject-rtos/zephyr.git zephyr-workspace/zephyr
    git -C zephyr-workspace/zephyr sparse-checkout set lib/cpp/minimal/include

`check-cxx-compat-shim-facilities` reads exactly that path, so the fast lane
needs the tree.

On 2026-11-06 the same clone was made in a local worktree, so the fast gates
could run as CI runs them (phase-543 W3). The next `just ci gate` failed in
`check::build`: `template-copy-out` built `ament-workspace-port/zephyr` and
reported "FAIL — the copy does not build". Before the clone, the same unit
recorded NOT VERIFIED ("Zephyr not provisioned — no west workspace resolves").
Removing the clone brought the earlier behaviour back.

The cause is the provisioning test in `scripts/check-template-copy-out.sh`:

    zws="$(bash scripts/lib/zephyr-workspace.sh --absolute resolve)"
    if [ -z "$zws" ] || [ ! -d "$zws/zephyr" ]; then  # "Zephyr not provisioned"

A sparse `zephyr-workspace/zephyr` holding one include directory passes it. It
is a source tree, not a west workspace: no `.west/`, no modules, nothing
`west build` can use.

## What it is

Two different capabilities share one path, and the test cannot tell them
apart:

- **zephyr-tree**: some Zephyr sources (here only `lib/cpp/minimal/include`),
  for gates that read headers.
- **zephyr**: a resolvable west workspace that can build an image.

phase-543 names them as two capabilities in `.config/gate-lanes.toml`. The
fast lane claims the first only, so under the nextest harness the build-side
gates stay out of the fast lane's scope. The old runner has no scope, so its
gates run whatever the test lets through.

## What would close it

"Is a west workspace provisioned" is answered in one place, and it asks
something a sparse source tree cannot satisfy (`west topdir` from the
candidate, or a `.west/config` naming the manifest). `template-copy-out`,
`zephyr-workspace-foreign-checkout` and every other reader of
`zephyr-workspace.sh resolve` take that answer. The sparse header tree moves
to a path of its own, or the readers that want headers alone ask for the
header path, never for "the workspace".

## Resolution (2026-11-09)

The one resolver answers it. `nros_zephyr_ws_resolve`
(`scripts/lib/zephyr-workspace.sh`) takes a ladder candidate only when it holds
`zephyr/` AND `.west/`. `west init` makes `.west/`, and a clone never does, so
CI's sparse header tree no longer resolves. `template-copy-out`,
`check-tier-preconditions` and every other reader of `resolve` take that
answer with no change of their own: a sparse tree now reads as "Zephyr not
provisioned".

- The CLI spells the same ladder in Rust (`zephyr_base_with` in
  `cmd/build.rs`, "rung for rung" with the shell), so its rungs take the same
  predicate. Otherwise `nros build` would have handed the sparse tree to west
  as `ZEPHYR_BASE`.
- `$NROS_ZEPHYR_WORKSPACE` stays as stated in both: it is the user's choice,
  and in the shell also the install target of `just zephyr setup`. The lane
  capability probe (`gate-lane-preflight.py`, `zephyr`) keeps its own `.west/`
  test for exactly that override.
- The header readers already name the header path, never "the workspace":
  `check-cxx-compat-shim-facilities.py`, `provision-fast-lane.sh` and the
  `zephyr-tree` capability probe. So the sparse tree stays where CI puts it.

Tests: `just check zephyr-workspace-resolve` (`tests/zephyr-workspace-resolve-tests.sh`:
nothing, the sparse tree, the same tree after `west init`, the override). It
fails with the `.west/` test removed (checked). Also the Rust
`a_source_tree_without_dot_west_is_not_a_ladder_rung`, with the override and a
real workspace as controls.
