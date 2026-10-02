---
id: 1657
title: "FreeRTOS mps2-an385 zenoh C/C++ images (cmake road) open a zenoh session
  and then never tick — on origin/main, so the zenoh heap default of that road
  could not be re-derived"
status: open
type: bug
area: [boards, freertos, rmw, testing]
severity: medium
found: 2026-10-03
related: [1624, 1598, 1197]
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
