---
id: 2466
title: "The ci-image FreeRTOS C++ build has no stack frames for zpico C functions that a local build has, so the stack-floor walk is refused there"
status: open
type: bug
severity: medium
area: [memory, freertos, build, ci]
found: 2026-11-12
related: [issue-2400]
---

## What

Live-peer run 43153068357 (ci-image job 130967366665) ran the issue 2400 W2 stack-floor CHECK
on four FreeRTOS C++ cmake fixtures on mps2-an385:

- `minimal_publisher`
- `workspace_port`
- `topic_state_monitor`
- `rclcpp_compat_smoke`

The `--indirect fold` walk was refused for all four:

```
nros-stack-floor: REFUSED
root `service_server_is_available_trampoline`:
  - functions reached with no frame in any input (a missing frame would count as 0):
    zpico_graph_entry_at, zpico_graph_entry_count
```

The same image built locally, from
`examples/templates/cpp-port-minimal-publisher/mps2-an385-freertos` with `nros build`, gave a
different result. It has those frames: the fold completes, and the CHECK reports
"OPEN stack 65536 B holds the floor 15896 B (unmeasured)".

So one image, from the same sources, has a stack-floor input in one build environment and not in
the other.

## Why it matters

The walk refuses rather than counting a missing frame as 0, so no floor is computed low in
silence. But whether the CHECK can run at all then depends on the build environment rather than
the image:

- **An unmeasured image** degrades to a warning. That was the main red, fixed by #3586 / #3597.
- **A MEASURED image** in the same environment FAILS the build, by user ruling 2026-11-12
  ("go (c) and fail"). It would pass locally and fail in CI, or the reverse, for no reason in its
  code.
- A floor that sees a frame in one build and not the other is not one measurement.

## Where the frames come from

`zpico_graph_entry_at` / `zpico_graph_entry_count` are C, defined in
`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c`. cc-rs compiles that file inside the `zpico-sys`
build script, under the Corrosion import.

`nros_stack_floor_cargo_env` (`cmake/NanoRosStackFloor.cmake`) puts
`CFLAGS=-fstack-usage -fcallgraph-info=su` on that import, and the CHECK reads `.su` files with
`--su ${CMAKE_BINARY_DIR}`. A `.su` that is not written, or not under the binary dir, is a
missing frame.

## Hypotheses (NOT verified)

1. **sccache.**
   - The FreeRTOS road keeps `RUSTC_WRAPPER`. The comment says so: it "keeps … its sccache hits".
   - cc-rs uses an sccache `RUSTC_WRAPPER` as its C compiler wrapper too.
   - A cache hit restores the `.o`. Whether it restores the `.su` written beside it is unmeasured.
   - The ci-image has a warm sccache; a fresh local leaf may not.
   - The Zephyr carrier (W4) clears `RUSTC_WRAPPER` for its check.
2. **Target dir outside the binary dir.**
   - If the CI configure puts the cargo target dir (with cc-rs's `OUT_DIR`) outside
     `CMAKE_BINARY_DIR`, the `.su` files exist but are not read.
   - Then the missing frames would be every cc-rs function, not just two. The two the message
     names are only the first ones a fold path reaches.

## Next step

Re-run one of the four fixtures in the ci-image with `NROS_STACK_FLOOR` on, and check:

- whether `zpico.c`'s `.su` exists under the build's cargo `OUT_DIR`;
- whether that dir is under `CMAKE_BINARY_DIR`;
- whether `SCCACHE_*` / `RUSTC_WRAPPER` are set.

Then do the same with `RUSTC_WRAPPER=` empty. The answer decides the fix: clear the wrapper as W4
does, or name the cargo target dir to `--su`.
