---
id: 2224
title: "`nros dev image board-facts` panics on a generated Zephyr Rust workspace entry since #2199 forwards `NROS_IMAGE` as `--deploy`"
status: resolved
type: bug
severity: high
area: [cli, zephyr, build]
related: [2199, 0941, 2229]
found: 2026-11-03
---

## What

`just build-test-fixtures lane=all` on main (`75d49a3541`) fails in the zephyr
family on `build-ws-rs-entry-zenoh` (the Rust workspace entry,
`examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry`):

```
CMake Error at cmake/NanoRosBoardFacts.cmake:209 (message):
  nano-ros: board facts for .../examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry
  --deploy zephyr could not be resolved (`nros dev image board-facts`):

  thread 'main' panicked at nros-cli-core/src/cmd/board_facts.rs:797:18:
  nothing_to_carry answered the no-board-crate case
```

Every other Zephyr fixture in the family configured.

## Why

Three facts meet:

1. The generated entry's `CMakeLists.txt` sets `NROS_IMAGE zephyr` and
   `NROS_IMAGE_SYSTEM_DIR` (the bringup that claims it). The entry directory
   itself has no `system.toml`.
2. Issue 2199 (`b46ba47d6d`, 2026-11-01) made `nros_resolve_board_facts`
   forward `NROS_IMAGE` as `--deploy`. Before it, this entry reached the verb
   with no `--deploy`.
3. In `board_facts.rs`, `nothing_to_carry` returns `None` as soon as `--board`
   or `--deploy` is set. With no `system.toml` the verb then takes the cargo-leaf
   branch and calls `board_of_cargo_leaf(..).expect(..)`. That expect assumes
   `nothing_to_carry` already answered the "no board crate" case, which it skipped.
   The entry depends on `nros-board-zephyr`, and no board descriptor names it as
   `board_crate` (a Zephyr board is chosen by `-b`), so the lookup returns `None`.

So the `expect` is reachable whenever `--deploy` is passed for a directory with
no `system.toml` whose Cargo.toml names no catalogued board crate. That is a
panic where issue 0941 requires an answer or a stated error.

## Resolution

Two defects, as filed, closed by two changes:

- **Asking the wrong directory**: fixed by issue 2229 (`33ba61ad46`), filed
  and fixed in parallel. `nros_resolve_board_facts` now asks about the dir
  `NROS_IMAGE_SYSTEM_DIR` names, the bringup whose `system.toml` declares the
  image, before `APPLICATION_SOURCE_DIR`. 2229 also turned the no-`Cargo.toml`
  shape of the same path into a stated error.
- **The `expect`**: still reachable after 2229, for a cargo dir that depends on
  no catalogued board crate. A generated Zephyr entry is exactly that: it
  depends on `nros-board-zephyr`, and no descriptor claims it. It is now a
  refusal naming the image, where to resolve it from, and `--board`. Test
  `a_named_image_in_a_cargo_dir_on_no_board_crate_is_refused_not_a_panic`.

Measured on main plus this change, in a fresh worktree:
`NROS_ZEPHYR_FIXTURE_FILTER=build-ws-rs-entry-zenoh just zephyr build-fixtures`
rc=0 and links `zephyr.exe`; `test_zephyr_workspace_entry_native_sim_e2e`
passes (with the Linux family built for its `int32-sink` peer).
