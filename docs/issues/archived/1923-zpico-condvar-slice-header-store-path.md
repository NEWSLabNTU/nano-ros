---
id: 1923
title: "A FreeRTOS zenoh build on a host whose SDK store holds the zenoh-pico pin panics reading `nros_zp_condvar_slice.h` from the SDK store: the header path was derived from where zenoh-pico came from, not from `zpico-sys`"
status: resolved
type: bug
area: [build, rmw-zenoh]
severity: high
found: 2026-10-15
resolved: 2026-10-15
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
dirty (a contributor editing the fork, as phase-502 did), or whose store does
not hold the pin, resolves the submodule — the one shape the derivation is
right for.

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

## Resolution

Fixed 2026-10-15, in the direction filed.

**The header is joined onto `zpico-sys`, never onto zenoh-pico.**
`condvar_slice_header(interp)` is `interp.nros.join("c/zpico/nros_zp_condvar_slice.h")`
— `interp.nros` is `CARGO_MANIFEST_DIR` of `zpico-sys`, the base the same
function already reads `zenoh-sources.txt` from. The parse moved beside it as
`condvar_slice_ms`, unchanged.

**Why the author's run was green.** `try_source("zenoh-pico")` answers the
store only when the store holds the PIN (`<store>/sources/zenoh-pico/<ver>+<ref8>`)
and the submodule is unedited; otherwise it falls to the checkout submodule,
the one layout where zenoh-pico's parent IS `zpico-sys`. Measured on a fresh
worktree: this host's store held four other pins and not `d958458e`, so the
same template built the submodule and did not reach the bug. Pointing
`NROS_HOME` at a scratch store holding the pin reproduced the report byte for
byte (`panicked at …/runner.rs:3238:33: read <store>/sources/zenoh-pico/c/zpico/nros_zp_condvar_slice.h`).

**Gate.** `runner::condvar_slice_header_tests` (in `test-unit` through
`--workspace`) builds an `InterpContext` whose zenoh-pico sits in a
store-shaped scratch dir with no `c/zpico`, and asserts the header resolves
under `zpico-sys` and parses; a second case asserts the submodule shape and
the store shape answer the same file.

| tree | `cargo test -p nros-zpico-build --lib condvar_slice` |
| --- | --- |
| fix | 2 passed |
| mutant (`interp.src.parent().parent()`, the filed derivation) | 2 FAILED — `got /tmp/nros-condvar-slice-store-<pid>/c/zpico/nros_zp_condvar_slice.h` |
| restored (`cmp` clean) | 2 passed |

**Symptom.** `scripts/check-template-copy-out.sh cpp-port-minimal-publisher`
with `NROS_HOME` = a scratch store holding the pin: before, `zpico-sys`'s build
script panics at `runner.rs:3238`; after, it reads
`zpico-sys/c/zpico/nros_zp_condvar_slice.h`, resolves zenoh-pico from the store
(300 store paths in its `output`) and produces `libzenohpico.a`.

The gate as a whole is still red on that template, for a different reason
reached later in the same cargo build: `nros-cpp`'s const assertion
`size_of::<PubMonitorCell>() … <= NROS_CPP_MONITOR_ROW_STORAGE` fails on
`thumbv7m-none-eabi` (64-byte rows on 32-bit, since phase-503's
deadline-armed monitor, 784c3bf244). Not this issue, and not filed here.

**Sweep.** `git grep -n 'parent()' packages/rmw/zenoh/nros-zpico-build/src/
packages/rmw/zenoh/zpico-sys/build.rs` and `git grep -n nros_zp_condvar_slice`:
the only site. Zephyr's include of the same header
(`zephyr/cmake/nros_rmw_zenoh.cmake`) is rooted at `${NROS_REPO_DIR}`.
