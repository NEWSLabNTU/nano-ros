---
id: 1892
title: "`native-check` fails on a fresh worktree: the Linux host leaves are linted without `nros sync`"
status: resolved
type: bug
area: [testing, examples, build]
severity: medium
found: 2026-10-14
resolved: 2026-10-14
related: [1882, 1886, 1305, 1381, 1887, 1888, 1891, phase-500]
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

## Resolution (2026-10-14)

By the time this was fixed, issues 1887/1888 had already made the SYNC
unconditional (every leaf with a `system.toml` or a `package.xml`). Two
platform-directory decisions were still in `lint-example-leaf.sh`, and the
precondition text was still wrong:

- `--config build/<image>/nros-cargo.toml` was passed only when
  `platform != "native"`. The real property is whether `nros sync` already
  wired that file into the leaf's own `.cargo/config.toml` as an `include`
  (issue 1381). Sync does that for EVERY image-declaring leaf, board leaves
  included, so the board leaves were reading their settings twice. It now
  passes `--config` only when the include is absent (sync skips it for a leaf
  that states its own `[build] target` / `rustflags`). After a sync, 0 of the
  49 linted leaves need `--config`.
- The profile flags were withheld from `native` leaves, a `--release`-for-
  firmware rule from before the named profiles. Every leaf now lints at the
  profile the fixture lane builds it at.
- `just linux check` had a message-bindings precondition that told the user
  to run `just generate-bindings`. Since 1887 it could only refuse a leaf that
  the lint was about to render, so it was removed. The `native-check` recipe
  comment now names what the lane needs: the `nros` CLI on PATH.

The shape tests live in ONE place, `scripts/build/leaf-settings.sh`:
`nros_leaf_sync_renders`, `nros_leaf_declares_image` (any `[image.<id>]`, no
id is named) and `nros_leaf_inside_config_args`. No other script in the
fixture or lint roads chooses by platform directory where the property is
leaf shape. The fixture road already keyed on `system.toml` + `Cargo.toml`.
The remaining `platform = "linux"` tests (`fixtures-build.sh`,
`workspace-fixtures-build.sh`, `scope.sh`, `check-template-copy-out.sh`,
`fixture-inventory.py`) are host-vs-cross decisions, not leaf shape. Sweep:

```
git grep -nE '(!=|=|==) *"?(native|linux)"?( |\]|;|\)|$)' -- 'scripts/**' 'just/**'
```

**Guard.** `check-host-platform-vocabulary` (fast line, so `pull_request` and
`merge_group`) gained item 9. It runs the lint in plan mode
(`NROS_LINT_LEAF_PLAN=1`, no sync and no cargo) over five synthetic leaf
shapes, each placed under `examples/native/`, `examples/linux/` and
`examples/mps2-an385-baremetal/`. Every plan must equal the shape's expected
plan. It also requires every tracked image-declaring example leaf to plan a
render. Its cost is about 0.7 s.

**Measured**, on a fresh worktree (`p1892b`, `just setup-cli`,
`setup-worktree`, `setup-launch-resolve`, no hand `nros sync`, 0 leaf `build/`
dirs before the run):

| run | rc |
| --- | --- |
| `just check native-check` (49 leaves) | 0 |
| mutation: render gated on `platform != "native"` again, `check-host-platform-vocabulary` | 1 (4 synthetic shapes + 21 real native leaves named) |
| same mutation, `just check native-check` (native `build/nros` removed first) | 2 (`failed to load manifest for dependency native_rs_async_action_client_nros_selection`) |
| restored: guard / `native-check` | 0 / 0 |
