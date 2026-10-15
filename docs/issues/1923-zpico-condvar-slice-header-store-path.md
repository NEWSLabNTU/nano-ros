---
id: 1923
title: "Every FreeRTOS zenoh build outside an edited checkout panics reading `nros_zp_condvar_slice.h` from the SDK store: the header path was derived from where zenoh-pico came from, not from `zpico-sys`"
status: open
type: bug
area: [build, rmw-zenoh]
severity: high
found: 2026-10-15
related: [1196, phase-502, 0491]
---

## What was seen

`just check template-copy-out` went red on `main` after fb99dae0d6
(phase-502 W2, issue 1196): the three FreeRTOS C++ templates
(`cpp-port-minimal-publisher/mps2-an385-freertos`,
`rclcpp-compat-smoke/mps2-an385-freertos`,
`topic-state-monitor-port/mps2-an385-freertos`) each failed in the
`zpico-sys` build script with

```text
panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:3238:
read /home/aeon/.nros/sources/zenoh-pico/c/zpico/nros_zp_condvar_slice.h:
No such file or directory (os error 2)
```

## Cause

phase-502 W2 hands the fork's `Z_NROS_CONDVAR_WAIT_SLICE_MS` the value of
`NROS_ZP_CONDVAR_SLICE_MS`, read from
`zpico-sys/c/zpico/nros_zp_condvar_slice.h`. The FreeRTOS-only block in
`build_zenoh_pico_unified` located that header as

```rust
zenoh_pico_src.parent().expect("zenoh-pico sits inside zpico-sys")
    .join("c/zpico/nros_zp_condvar_slice.h")
```

The `expect` text states the assumption, and it is false: `zenoh_pico_dir`
resolves zenoh-pico through `nros_build_paths::locate::try_source`, which
answers the SDK store (`~/.nros/sources/zenoh-pico`) unless the checkout's own
submodule carries an edit. So the in-tree submodule — the only layout where the
parent of zenoh-pico is `zpico-sys` — is the exception, and every other build
read `<store>/c/zpico/...`, which never exists. The header is nano-ros's own
source; where zenoh-pico came from says nothing about where it lives.

It escaped the author's run because a checkout whose zenoh-pico submodule is
dirty (a contributor editing the fork, as phase-502 did) resolves the
submodule, and that is the one shape the derivation is right for.

The same function already had the right base two steps earlier:
`interp.nros` (`CARGO_MANIFEST_DIR` of `zpico-sys`), from which it reads
`zenoh-sources.txt`, with a comment explaining that a `.parent()` walk is the
shape that broke before (phase-321 W2.d).

## Sweep

`git grep -n 'parent()' packages/rmw/zenoh/nros-zpico-build/src/ packages/rmw/zenoh/zpico-sys/build.rs`
and `git grep -n 'c/zpico' -- 'packages/rmw/zenoh/**.rs' '*.cmake' 'zephyr/**'`:
this was the only site deriving a `zpico-sys` path from zenoh-pico's location.
The Zephyr side of the same header (`zephyr/cmake/nros_rmw_zenoh.cmake`) adds
`${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/c/zpico` — rooted at the repo,
independent of the zenoh-pico source.
