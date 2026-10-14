---
id: 1892
title: "`native-check` fails on a fresh worktree: the Linux host leaves are linted without `nros sync`"
status: open
type: bug
area: [testing, examples, build]
severity: medium
found: 2026-10-14
related: [1882, 1886, 1305]
---

## Measured

Found while fixing issue 1886, in a fresh worktree (`just setup-cli`, `just setup-worktree`, `just setup-launch-resolve`). `native-check`
(`just/check/lanes.just` → `just linux check` → `scripts/build/lint-example-leaf.sh` per leaf)
fails even when run on its own, with no other gate in parallel:

- 21 `examples/native/rust/*` leaves fail at manifest parse. Each needs
  `build/nros/selection`, the generated `<leaf>_nros_selection` package.
- `examples/native/rust/logging` also needs the sync-written
  `.cargo/config.toml` that patches `nros-log`.

Running `nros sync` by hand in each leaf turns the gate green, which is what
the 1886 proofs had to do.

## Cause

`lint-example-leaf.sh` renders a leaf with `nros sync` only when
`platform != "native"` (lines 24 and 39). The comment that justifies this
(issue 1882) says a board-image leaf is "every non-`native` platform with a
`system.toml`". That was true when 1882 was written against an older tree. Since issue 1305
phases D–F, the Linux host leaves are on the generated selection package too
(`[image.<id>]` in `system.toml`, RFC-0098 D1). So they are board-image leaves
by the same definition, and the platform exemption is now false.

The gate's documented precondition is also wrong. The recipe comment says
it needs `just generate-bindings`, which neither syncs a leaf nor writes its
`.cargo/config.toml`.

## Fix direction

- Decide by the leaf's SHAPE, not by its platform directory: a leaf with a
  `system.toml` declaring `[image.*]` is rendered first. That is the
  derived-property rule 1882 meant. A platform spelling cannot carry it, and phase-500 W4
  (`examples/native/` → `examples/linux/`) would silently move the
  exemption anyway.
- Correct the precondition text.
- Prove it on a fresh worktree: `just check native-check` (or its
  phase-500 name) gives rc=0 with no hand provisioning, and it fails if the shape test is
  reverted to the platform test.
