---
id: 2361
title: "A configure that finds the metadata-probe sidecars stale stops listing them as inputs, so a later `nros configure` never re-configures and the image stays sized from the closure fallback"
status: open
type: bug
severity: medium
area: [cmake, zephyr, cli]
related: [1662, 2086, phase-530]
found: 2026-11-07
---

**Home:** [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)

## What

A Zephyr image's sizing descriptor is derived from the workspace's
metadata-probe sidecars (`src/<pkg>/metadata/*.json`, written by
`nros configure`). When the sidecars on disk are stale against the tree the
build compiles, the descriptor refuses each subscription's
`registration_path` (`[endpoint.refused] ... a stale or missing
sidecar`, `packages/cli/nros-cli-core/src/sizing_descriptor.rs`), and the
receive slots are priced at the closure buffer. That fallback is correct.
The defect: a configure in that state registers NO sidecar as a
`CMAKE_CONFIGURE_DEPENDS` input. So when the sidecars are made current
again (`nros configure`), ninja does not re-run cmake, and every later
`cmake --build` / `west build` keeps the oversized image until a fresh
configure is forced. A build dir that was configured against current
sidecars does list them, so the dependency set depends on the state it
is meant to track.

## Measured (safety island, S32K344, nano-ros ac23c0155b)

Sequence: the island's diag build applies a patch to the nano-ros tree,
runs `nros configure` (the sidecars are re-keyed to the patched tree: the
probe-closure field of `provenance.inputs_digest` reads `67208a3a...`
patched against `e1038fae...` clean), builds a separate build dir and
reverts the patch. The normal build dir `build-board` listed the
sidecars as configure inputs, so the next `west build` re-ran cmake:

- the descriptor found all sidecars stale and refused `registration_path`
  on all six subscriptions (`in_place` in the good descriptor);
- `NROS_CPP_LEVELS_BACKING_SIZE` went from 33,128 to 64,544 B, and
  `__nros_levels_backing` grew by 31,416 B;
- the link failed: `region 'DTCM' overflowed by 18336 bytes`.

The relocation list, `compile_commands.json`, `CMakeCache.txt` and
`.config` were byte-identical between the good and the failing build;
the descriptor, that header and that symbol were the only differences.

Then `nros configure` on the clean tree made the sidecars current again, and
`cmake --build build-board` still failed the same way: the configure in
the stale state had registered no sidecar input, so nothing re-ran it.
Only a fresh configure (the island repo's board-build recipe) recovered.

## Expected

The set of configure inputs should not depend on whether the sidecars
were current: a configure that read a sidecar (and found it stale) or
looked for one (and found none) should register that path, or the
sidecar directory, so that `nros configure` making it current re-runs the
configure. The `input` rows the sizing verb prints (consumed in
`zephyr/cmake/nros_cargo_build.cmake`, `_nros_west_leaf_sizing`, and
the entry-image producer) look like the place: emit them for every
sidecar consulted, current or not.

## Workaround (island, not a fix here)

The island's diag build re-runs its sync recipe after it reverts its patches,
and its `board-flash` flashes with `west flash --no-rebuild`
(NEWSLabNTU/simple-autoware-safety-island 9de23c9).
