---
id: 2232
title: "A generated Zephyr application cannot configure: board facts ask its own dir, and the verb panics"
status: resolved
type: bug
area: [zephyr, build, cli]
found: 2026-11-03
related: [issue-2199, phase-530]
---

## Symptom

`nros build demo_bringup:zephyr` on `examples/workspaces/rust` fails at the
west configure, every time, before any knob is resolved:

```
CMake Error at cmake/NanoRosBoardFacts.cmake:209 (message):
  nano-ros: board facts for .../build/zephyr-zenoh/zephyr_entry
  --deploy zephyr could not be resolved (`nros dev image board-facts`):
  thread 'main' panicked at nros-cli-core/src/cmd/board_facts.rs:797:18:
  nothing_to_carry answered the no-board-crate case
```

Found while measuring phase-530 W3's baseline for the generated Rust west
entry; reproduced by hand with
`nros ws board-facts <ws>/build/zephyr-zenoh/zephyr_entry --deploy zephyr`.

## Cause

Issue 2199 made `nros_resolve_board_facts` forward `NROS_IMAGE` as the deploy.
A generated Zephyr application states `NROS_IMAGE` and
`NROS_IMAGE_SYSTEM_DIR` (the bringup its image lives in), but the function
asked `APPLICATION_SOURCE_DIR`, the generated application's own directory,
which holds no `system.toml`. With a deploy named, the verb's
`nothing_to_carry` declines to answer, so it falls into the no-`system.toml`
branch and looks for a board crate in the application's `Cargo.toml`. The
Zephyr board descriptors state no `board_crate`, so none matches, and an
`expect` whose message assumed that case was already answered panicked.

## Fix

- `cmake/NanoRosBoardFacts.cmake` asks `NROS_IMAGE_SYSTEM_DIR` (resolved the
  way the module hook resolves it) before `APPLICATION_SOURCE_DIR`.
- `nros dev image board-facts` returns an error naming the deploy instead of
  panicking (`a_deploy_with_nothing_to_resolve_it_against_is_an_error`).
