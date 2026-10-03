---
id: 1657
title: "FreeRTOS mps2-an385 zenoh C/C++ images (cmake road) open a zenoh session
  and then never tick — on origin/main, so the zenoh heap default of that road
  could not be re-derived"
status: resolved
type: bug
area: [boards, freertos, rmw, testing]
severity: medium
found: 2026-10-03
resolved_in: 2026-10-03
related: [1624, 1598, 1599, 1197, 1658, 1659]
---

## What

Found while re-deriving the FreeRTOS heap default per RMW for issue 1624
(PR #1579). Every cmake-built zenoh C/C++ FreeRTOS image tried boots to
`Network ready`, opens its zenoh session against a live `rmw_zenohd`, and then
prints nothing: no `[talker_pkg] sent:` (workspace `c`), no `[ctrl] tick=`
(`realtime-c`, `realtime-cpp`).

Reproduced on **pristine `origin/main` (`28ffc826b5`)** in the same worktree —
fresh `just setup-cli`, rows rebuilt — so it is not PR #1579's:

| image (row) | session | output after `Network ready` | heap peak |
| --- | --- | --- | --- |
| `workspace-c-freertos` | ESTAB to the router, router logs "New transport opened" | none in 40 s | 92,032 of 3,145,728 |
| `workspace-c-freertos-realtime` | same | no `tick=` in 60 s | 92,032 of 2,883,584 |
| `workspace-cpp-freertos-realtime` (on the PR branch) | same | `realtime_tiers_e2e`: "qemu did not print `[ctrl] tick=` within 90s" | 92,032 of 2,621,440 |

Rig: store QEMU 11.0.0-nros2, `-machine mps2-an385 -icount shift=auto -nic
user,model=lan9118,net=192.0.3.0/24,host=192.0.3.1` (the harness's
`start_mps2_an385_freertos_slirp`), `/opt/ros/humble` `rmw_zenohd` with
`listen/endpoints=["tcp/0.0.0.0:<baked port>"]` and the paired
`zenoh_cpp_vendor` on `LD_LIBRARY_PATH`. The same-board CycloneDDS image
(`workspace-cpp-mps3-an536-freertos`, a different machine) delivers in-image
on the same host.

The identical 92,032-byte heap peak across three different images suggests all
three stop at the same point after session open, before any entity work.

## Why it matters beyond itself

On the cmake road `FreeRTOSConfig.h` alone sizes the heap. Issue 1624 derived
the Cyclone/XRCE default from a running image; the zenoh default on this road
(3072 KiB minus `.bss` tier stacks, issue 1598) could not be re-derived the same
way, because no zenoh cmake image here gets past session open, so its peak
is not a working set. It was left unchanged rather than cut unmeasured.

## Not established

Whether this is host-specific (the nightly `freertos` lane passed its last run)
or a regression on main; no bisect was run. The test harness (`entry_e2e`)
needs a native C listener fixture to judge the non-tiered cells.

## Resolution

**Root cause: one FreeRTOS config fact had two carriers, so two layouts of one
kernel struct were linked into the same image.** The cmake board overlays
(`mps2-an385`, `mps3-an536`, `s32z270`) passed `configUSE_TRACE_FACILITY=1` as
`target_compile_definitions(freertos_kernel PUBLIC …)` — so CycloneDDS's
`ddsrt_gettid()` could reach `vTaskGetInfo()` — and the cyclone branch of
`cmake/platform/nano-ros-freertos.cmake` repeated it in `CMAKE_C_FLAGS`. That
`-D` reached the kernel and the cmake targets linking it. It did NOT reach the
C that cargo compiles: zenoh-pico in `zpico-sys` reads the same
`FreeRTOSConfig.h`, where the facility was off. The facility adds
`uxQueueNumber`/`ucQueueType` to `Queue_t`, so the kernel's queue was 80 bytes
and zenoh-pico's `StaticSemaphore_t` 72 (both read from the image's DWARF).

That was harmless while every kernel object came out of the heap at the
kernel's own size. Issue 1598 (`0ba0600e5c`) set
`configSUPPORT_STATIC_ALLOCATION 1`, and zenoh-pico's FreeRTOS port then EMBEDS
a `StaticSemaphore_t` in each `_z_mutex_t`. `_mutex_rx` and `_mutex_tx` are
adjacent in `_z_transport_common_t`, so initialising `_mutex_rx` wrote its
`ucQueueType` (4, `queueQUEUE_TYPE_RECURSIVE_MUTEX`) 76 bytes in — over
`_mutex_tx`'s `pcHead`. A non-NULL `pcHead` means "not a mutex", so the kernel
stopped recording a holder, and the first RECURSIVE re-take of the tx lock
blocked the app task forever, in `declare_node_liveliness` inside
`ZenohSession::new`: after the TCP session was up, before any entity — which is
why every image stopped at the same 92,032-byte heap peak.

### How it was found

- gdb on the QEMU gdbstub (raw-TCB unwind of `s_app_task`): the app task sat in
  `xQueueSemaphoreTake` ← `_z_transport_tx_mutex_lock` ← `_z_send_declare` ←
  `z_liveliness_declare_token` ← `ZenohSession::new`; the tx mutex read
  `pcHead = 4, holder = NULL, uxRecursiveCallCount = 1, uxMessagesWaiting = 0`.
- A hardware watchpoint on that `pcHead` from reset (`-S`) caught the writer:
  `xQueueCreateMutexStatic` called from `_z_mutex_init(&ztu->_common._mutex_rx)`
  (`transport.c:43`).

### Bisect

First bad commit: **`0ba0600e5c` fix(#1598): FreeRTOS C/C++ tier task stacks
are the entry's statics** — measured on `workspace-c-freertos` with zpico-sys
forced to recompile at each end (`touch zpico-sys/build.rs`): its parent
`22bdcace1c` prints `[talker_pkg] sent: 0…`; `0ba0600e5c` stops after
`Network ready` at `heap peak 92032`. A plain incremental step had reported
`0ba0600e5c` GOOD — a false negative: before `4c298b41e1` (issue 1599) zpico-sys
did not watch `FreeRTOSConfig.h`, so its zenoh-pico was still the STATIC=0 build
(`libzenohpico.a` 3,891,842 bytes, where the fresh STATIC=1 build is
3,986,866). The automated `git bisect run` over `0ba0600e5c..main` was
abandoned for that reason and for issue 1659 (under `git bisect run` the CLI
inherits `GIT_DIR` and `nros sync` refuses every step).

### Fix

The facility is stated once, in the shared
`packages/boards/nros-board-freertos/config/FreeRTOSConfig.h` (all three family
boards include it; the POSIX board's own config already had it), and the four
`-D` sites are gone. The Rust road compiled its kernel with the facility OFF and
now has it ON too, so every road and every compiler agree (+8 bytes per TCB and
per queue). New gate `check-freertos-config-single-carrier` (fast line) refuses
a `config*` macro passed by any tracked build file — `-Dconfig*`,
`.define("config*")`, `*_compile_definitions(… config*)` — with one allowed
syntax-only probe; negative control: it flags the pre-fix mps2 overlay at line
156. ThreadX was checked for the same shape: its layout selectors
(`TX_INCLUDE_USER_DEFINE_FILE` …) already reach zpico-sys through
`nros-platform-threadx/nros-platform.toml`'s `defines`, one carrier for both
roads.

### Measured (QEMU 11.0.0-nros2, `/opt/ros/humble` `rmw_zenohd`, 30 s boots)

Every image rebuilt from the tree named (fix reverted in the working tree for
"before", restored for "after").

| row (port) | before | after |
| --- | --- | --- |
| `workspace-c-freertos` (7930) | 4 lines; heap peak 92,032 / 3,145,728 | `[talker_pkg] sent: 0`, `Received: 0` …; heap peak 99,216 |
| `workspace-cpp-freertos` (8030) | 4 lines; 92,032 / 3,145,728 | `Received: 0..3` …; heap peak 99,368 |
| `workspace-mixed-freertos` (8130) | 4 lines; 92,032 / 3,145,728 | `[c_talker_pkg] sent: 0`, `Received: 0` …; heap peak 99,216 |
| `workspace-c-freertos-realtime` (7991) | 4 lines; 92,032 / 2,883,584 | `[ctrl] tick=0..`; heap peak 99,896 |
| `workspace-cpp-freertos-realtime` (8091) | 4 lines; 92,032 / 2,621,440 | `[ctrl] tick=0..`; heap peak 103,776 |
| `workspace-cpp-freertos-realtime-subnode-portable` (8092) | 4 lines; 92,032 / 3,145,728 | `[subnode/ctrl] tick=0..`; heap peak 99,384 |

`realtime_tiers_e2e` (bare `cargo nextest`): before — `2 of 18 row(s) FAILED:
freertos/cpp … tier ctrl never published; freertos/c … tier ctrl never
published`; after — PASS, 3 rows ran (freertos rust/c/cpp), 15 skipped for
fixtures not built here. The Rust FreeRTOS realtime row passed both before and
after: its road had one carrier (the facility off everywhere).

The zenoh cmake road's heap working set is now measurable, which issue 1624
could not do: ~99–104 KB of a 2.5–3 MiB default. The default is not changed
here.

### Not measured

- `entry_e2e`'s cross-process FreeRTOS cells (the host listener fixture was not
  built); delivery was observed in-image only (talker → listener `Received:`).
- mps3-an536 and s32z270 images were not rebuilt or booted; their change is the
  same `-D` removal against the same shared header.
- The single-node Rust FreeRTOS examples were not re-booted after the fix.

### Why nothing noticed

No lane that boots one of these images reached its cells — filed as
[issue 1658](../1658-freertos-zenoh-boot-regression-reaches-no-lane.md).
