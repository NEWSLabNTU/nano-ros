---
id: 2224
title: "`nros dev image board-facts` panics on a generated Zephyr Rust workspace entry since #2199 forwards `NROS_IMAGE` as `--deploy`"
status: open
type: bug
severity: high
area: [cli, zephyr, build]
related: [2199, 0941]
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

## Fix direction (not applied)

Two separable defects:

- The `expect` should be an error that names the leaf and the flags it got,
  or `nothing_to_carry`'s early return should not cover the no-`system.toml`
  case.
- The verb answers for the wrong directory: the entry's image lives in the
  bringup's `system.toml` (`NROS_IMAGE_SYSTEM_DIR`), so `--deploy zephyr` is
  only meaningful against that file. Either cmake passes the system dir, or the
  verb follows the entry to its claiming bringup, as `board_of_cargo_leaf`
  already does for selection packages (issue 1305).

## Reproduce

```
NROS_ZEPHYR_FIXTURE_FILTER=build-ws-rs-entry-zenoh just zephyr build-fixtures
```
