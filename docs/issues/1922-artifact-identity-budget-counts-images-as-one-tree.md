---
id: 1922
title: "`artifact-identity-budget` counts every image of a workspace as one tree: two freshly built images read 8 nros_core identities against a one-image budget of 4"
status: open
type: bug
area: [build, testing]
severity: low
found: 2026-10-15
related: [0499, 1832, 1919, phase-340]
---

## Measured

On a worktree where `just freertos build` and the native fixture build both ran
inside one `started_at` window, `just check fast` stopped at:

```
artifact-identity budget: FAIL
  counted 275 of 743 rlib(s) — those written since started_at=2026-10-15T07:16:12Z (issue 0499).
  nros_core has 8 distinct -C metadata identities in examples/workspaces/mixed/build
  (budget 4, recorded 2026-08-07 by phase-340 W4).
```

The eight identities split into two sets of four, one set per image:

- `examples/workspaces/mixed/build/freertos-zenoh-mps2-an385-freertos/cmake/cargo/{nano-ros_<h>,nros_ws_runtime_<h>}/…`,
  each at host and `thumbv7m-none-eabi`;
- `examples/workspaces/mixed/build/posix-zenoh-linux/cmake/cargo/{nano-ros_<h>,nros_ws_runtime_<h>}/…`,
  each at host and `x86_64-unknown-linux-gnu`.

Each image has the 4 identities the budget allows: two cargo roots (workspace
member vs. path dep, phase-340's "R2 re-measured") times two `--target`
spellings. No image has a new identity.

## Why

The budget was recorded when the mixed workspace built ONE feature set into one
tree. Since issue 1832, `nros build` writes each image to
`<ws>/build/<image>/…`, and `scripts/check-artifact-identity-budget.sh` adopted
`examples/workspaces/mixed/build` as its subject, which the script describes as
"the same subject — the mixed workspace at one feature set". It stays one
subject only while a single image has been built in the window. `just freertos
build` builds `workspace-fixtures-build.sh freertos mixed`, which adds a second
image beside the native one. The gate then fails on a tree where every image is
within budget.

No CI lane runs this gate (issue 1832), so the red is local. It blocks
`just ci gate` and the `pre-push` hook. The workaround used on 2026-10-15
(phase-502 W6) was to delete the FreeRTOS image's build dir.

## Direction

Budget `nros_core` identities per image dir (`<ws>/build/<image>/`). Keep the
copy ceiling counted across the whole workspace tree, because issue 1919's
regression is two build ROOTS of ONE workspace writing the same identity
(`build/nros-metadata/…` and `build/<image>/…`), and a per-image split would
hide it. Add a self-test case with two images, each within budget.

## Progress

- **2026-10-15, the identity half: the gate reads one image.** It no longer
  globs `examples/workspaces/mixed/build`. It reads `row_artifact_root()` of
  fixture row `workspace-mixed-native` (`build/posix-zenoh-linux/cmake`),
  which is issue 1832's derived-tree fix. A FreeRTOS image beside it, the
  metadata probe tree, and pre-phase-500 `posix-zenoh-native` residue are no
  longer counted. Measured in the box tree on that root: nros_core 4/4, worst
  crate 5/5, worst identity 5/5.
- **Still open: the copy ceiling across build roots.** Issue 1919's shape is
  `build/nros-metadata/…` against `build/<image>/…` writing one identity
  twice, and a per-image subject does not see it. That needs the second
  reading the Direction above describes: copies counted over `<ws>/build`, or
  over the two roots by name. A self-test case with two images, each within
  budget, should come with it.
