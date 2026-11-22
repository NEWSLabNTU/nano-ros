---
id: 2859
title: "A changed `[knobs.memory] heap_bytes` does not rebuild the ThreadX byte pool: the board build script never watches `NROS_MEMORY_HEAP_BYTES`"
status: resolved
type: bug
severity: low
area: [build, memory, threadx]
related: [2732, 0491]
found: 2026-11-23
resolved: 2026-11-23
---

## What

`nros_board_common::threadx_sources::define_byte_pool` sizes the pool from
the `memory.heap_bytes` ladder when `NROS_THREADX_BYTE_POOL_BYTES` is unset:

```rust
Some(rungs) => rungs.memory_value("heap_bytes", derived),
```

`BuildRungs::memory_value` reads the environment (`NROS_MEMORY_HEAP_BYTES`)
and emits no `cargo:rerun-if-env-changed` for it. The build script watches
`NROS_THREADX_BYTE_POOL_BYTES` (`watch_knob_env`) and the app stack's
`NROS_MEMORY_APP_STACK_BYTES`, never the heap's.

## Measured (2026-11-23, issue 2732's work)

realtime-rust `[image.threadx]` (threadx-linux), already built with a
721,152-byte pool. Adding `[image.threadx.knobs.memory] heap_bytes = 459008`
put `NROS_MEMORY_HEAP_BYTES = "459008"` in the image's `nros-cargo.toml`, and
`nros build` exited 0 with `byte_pool_storage` still 721,152 bytes
(`nm -S`). The board's build-script `output` lists no
`rerun-if-env-changed=NROS_MEMORY_HEAP_BYTES`. The same value through the
watched `NROS_THREADX_BYTE_POOL_BYTES` re-ran the script at once.

So a stated heap size is silently ignored on an incremental build, including
one that shrinks the pool below what the image needs: the build-time floor
check never runs.

## Sweep

`git grep -n 'memory_value("heap_bytes"' -- packages`: this site and
`nros-board-freertos/build.rs` (the FreeRTOS heap). Check whether the
FreeRTOS one watches the variable before fixing only this one. The fix
belongs where the rung is read (`memory_value` / `memory` should watch
every spelling it reads, `watch_knob_env`), not at each caller.

## Resolved (2026-11-23)

The missing edge was in the reader, not the caller: `BuildRungs::memory` is
the only build-script rung resolver that reads the environment, and it
watched nothing. It now emits `cargo:rerun-if-env-changed` for every spelling
of every memory knob it resolves (`memory_env_watches`, unit-tested), so each
caller gets the edge: the ThreadX pool, the FreeRTOS heap, and both boards'
app stacks. The callers' own watches stay; they cover their no-rungs branch
(a bare `cargo build`), which reads the environment without the resolver.

Measured, realtime-rust `[image.threadx]`, incremental builds:
`[image.threadx.knobs.memory] heap_bytes = 800000` gave a 816,640-byte pool,
then `850000` gave 866,640 (each plus the port's 16,640-byte raise). Before,
the second build kept the first pool. The board's build-script output now
lists `rerun-if-env-changed=NROS_MEMORY_HEAP_BYTES`.
