---
id: 2253
title: "A generated entry kept the board's panic crate under `panic = \"platform\"`, so the image failed with E0152"
status: resolved
type: bug
severity: medium
area: [build, cli, boards]
related: [1305]
found: 2026-11-04
---

## What

On `mps2-an385-freertos` (and `mps2-an385`), a workspace image stating
`panic = "platform"` did not compile:

```
error[E0152]: found duplicate lang item `panic_impl`
  = note: the lang item is first defined in crate `panic_semihosting`
  = note: second definition in the local crate (`freertos_talker_entry`)
```

The board descriptor carried its panic crate (`use panic_semihosting as _;`
plus its dependency) as a general crate-root item. The two roads that emit
those items each spelled the rule for when to keep them, and they disagreed:

- the leaf road (`orchestration/facade.rs`, the selection package) cleared
  EVERY crate-root item for a non-`own` image. That is right for the panic crate
  but wrong for any other item, such as Zephyr's `extern crate zephyr;`;
- the generated-entry road (`builder/entry.rs`, `render_source` /
  `render_manifest`) cleared none.

So `examples/mps2-an385-freertos/rust/talker` (`panic = "platform"`) compiled
as a leaf, and the same image failed as a workspace image. Found by phase-535
W0, moving the example pairs into the language workspaces.

## Fix

- The descriptor states the panic crate apart:
  `[board.entry] own_panic_root_extra` / `own_panic_root_deps`. The two mps2
  descriptors move their `panic-semihosting` lines there.
- `BoardFacts::for_image_panic` is the one rule both roads call. It folds those
  items into the crate-root items when the image's effective policy (its own
  `panic`, else the board's default) is `own` or unstated, and drops them
  otherwise. Other crate-root items are kept whatever the policy.
- The descriptor gate that checks every crate-root `use` has its dependency now
  checks both pairs. `scripts/lib/leaf_target.py` (read by
  `check-image-panic-policy`) reads both keys.

## Verified

- `pubsub_bringup:freertos_talker` with `panic = "platform"`, and
  `demo_bringup:freertos` with the board default `own`, both build.
- New unit test `a_non_own_panic_drops_the_boards_panic_crate`: source and
  manifest drop the crate under `platform` and keep it when unstated.
- `check-image-panic-policy`: OK.
