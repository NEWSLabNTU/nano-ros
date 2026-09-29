---
id: 1582
title: "The Zephyr realtime-rust entry and derived-entry west leaves share a coordinate, so building both leaves the wrong image under one name"
status: resolved
type: bug
area: [testing, zephyr, fixtures]
severity: medium
found: 2026-09-29
resolved: 2026-09-29
resolved_in: "branch fix/1582-zephyr-leaf-collision"
related: [issue-1571, issue-1537, issue-1016]
---

## What happens (reported by the issue-1571 agent, not yet re-measured)

`build-ws-rs-realtime-entry-zenoh` and
`build-ws-rs-realtime-derived-entry-zenoh` have the same coordinate. When both
were built in one run, the realtime-entry image contained the DERIVED content:
its console printed `boot tier derived-telem_node`. As a result,
`sched_dims_applied_e2e` zephyr/rust CorePin and EdfDeadline failed on an
image that was never the one they meant to test.

The derived leaf was added by the #1537 test lane (#1431).

## Likely shape

Either two manifest rows resolve to one artifact root, or the generated-entry
path is keyed on platform + RMW rather than on the bringup/entry. The second
is the collision risk noted in the #1436 review, and it was never filed.
`check-west-leaf-vocabulary` (issue 1016) checks names, not whether two names
share an output.

## Acceptance

- The two leaves build to distinct artifacts, and each image's console names
  its own tiers.
- A gate refuses two fixture rows or west leaves that resolve to one artifact
  or one generated-entry path.

## Resolution (2026-09-29)

**Re-measured, and the mechanism is the second shape: a generated entry keyed
on the image ID, and two bringups using one ID.** No two manifest rows shared
a coordinate or an artifact root, and the two west build dirs were distinct.
What they shared was the west APPLICATION SOURCE:

- `realtime-rust`'s `demo_bringup` and `derived_bringup` both declared
  `[image.zephyr]`. `nros build` generates an entry at
  `build/<platform>-<rmw>/<id>_entry/`, and on the west road that directory IS
  the Zephyr application. Both leaves were configured with
  `APPLICATION_SOURCE_DIR=…/realtime-rust/build/zephyr-zenoh/zephyr_entry` (one
  path, read from both leaves' `CMakeCache.txt` in the shared store).
- Every `nros build` rewrote that directory's `src/lib.rs`. After the derived
  build it said `launch = "derived_bringup"` (mtime 19:38). The authored-tier
  image was linked afterwards (19:45) and carried the string
  `derived-telem_node` (`grep -a`), which is exactly the reported console.
- The selection facade `generated/nros-selection/<id>_entry` and the cmake
  target `<id>_entry` in `build/<cmake-coord>/` have the same key, so the class
  covers all three roads, not only west.

**Sweep.** I ran `nros build --all --dry-run` through the new refusal on every
tracked workspace that has a bringup (31). It collided in two:

- `realtime-rust`: the reported pair.
- `realtime-c`: `smp_bringup` repeated `demo_bringup`'s `freertos`, `native`,
  `nuttx` and `riscv_nuttx` images verbatim. None of them has a hand-written
  application, so each generates a cmake target of the same name into its
  twin's root. Building `smp_bringup:native` would have replaced the
  `native_entry` that every realtime-c native cell runs. The shared
  hand-written `src/zephyr_entry` (demo + SMP) is legitimate. It generates
  nothing, picks the bringup itself, and each row has its own west build dir.

**Fix.**

- The derived image is `[image.zephyr_derived]`, so its entry is
  `build/zephyr-zenoh/zephyr_derived_entry/`.
- `smp_bringup` keeps only its Zephyr image.
- `nros build` now refuses, before generating anything and across every image
  in the workspace, any path that two images would generate:
  `cmd::build::generated_outputs`, per road, with a hand-written application
  claiming nothing. The three inline spellings of the entry dir now use one
  helper, `generated_entry_dir`.
- Keying the path on the bringup instead was rejected. It would move every
  user-facing generated path (the book's `build/posix-zenoh/native_entry/…`)
  for a case that one distinct ID solves. RFC-0065 F7 is amended.

**Gate.** `check-generated-output-collisions` runs on the fast line and tests
itself. Its rules:

1. In every tracked workspace, a generated image ID is declared by at most one
   bringup.
2. Over `fixtures-manifest.py west-leaves`, no two leaves share a build-dir
   name, a leaf ID, or a generated application.

Mutation-tested on the real tree:

| Mutation | Result |
| --- | --- |
| `derived_bringup` reverted to `[image.zephyr]` | rule 1 fires |
| Row reverted to `image = "zephyr"` | rule 2 fires |
| `smp_bringup`'s four duplicate images restored | rule 1 fires four times |

For the CLI: removing the `bail!` fails the pipeline test, and changing
`> 1` to `> 2` fails two unit tests.

**Acceptance, measured.**

- Both leaves were built in one run into a private build root
  (`build/zephyr-1582`), at 21:55, after the last code commit (21:40).
- Their `APPLICATION_SOURCE_DIR`s are now `…/zephyr_entry` and
  `…/zephyr_derived_entry`.
- `derived-telem_node` appears 0 times in the authored image and once in the
  derived one.
- Booted against `rmw_zenohd` on 7491:
  - the authored image prints `core pin tier=`high``, `EDF deadline set tier=`high` 10000us` and
    `multi-tier entry up (2 tiers, boot tier `low`)`;
  - the derived image prints `multi-tier entry up (2 tiers, boot tier `derived-telem_node`)`.
- `sched_dims_applied_e2e` passed. The arm lines were
  `[zephyr rust CorePin] ACCEPT`, `[zephyr rust EdfDeadline] ACCEPT` and
  `[zephyr rust DerivedTierBelowTransport] SILENT`. The cells it skipped
  (c/cpp/nuttx/threadx/freertos/linux) skipped only because those fixtures were
  not built in this run.
- Only one build order was run. With distinct application dirs the order no
  longer matters, by construction.
