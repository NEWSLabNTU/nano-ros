---
id: 2413
title: "Scheduled `host-tests` build shard runs the FreeRTOS C++ template copy-outs on the distro arm-none-eabi-gcc 10.3, which issue 1936's floor refuses"
status: open
type: bug
severity: medium
area: [ci, gates, toolchain, templates]
related: [1936, 1117, 1453, 2382, 2369, phase-543]
found: 2026-11-10
---

## Measured

Scheduled `host-tests` run 42747790725 on `main` `5f6b2cd21a`, job
129603946231 "tier-1 gates (just check) [2/2]", step `just check build
(shard 2 of the build lane)`:

```
Summary [2732.650s] 31 tests run: 27 passed (14 slow), 4 failed, 616 skipped
FAIL build::check::template-copy-out#ament-workspace-port/mps2-an385-freertos
FAIL build::check::template-copy-out#cpp-port-minimal-publisher/mps2-an385-freertos
FAIL build::check::template-copy-out#rclcpp-compat-smoke/mps2-an385-freertos
FAIL build::check::template-copy-out#topic-state-monitor-port/mps2-an385-freertos
```

Each of the four stops at configure with the same refusal:

```
arm-none-eabi-gcc 10.3.1 (via system PATH) is below the C++ floor of GCC 11.
```

The other 27 build-tier gates in that shard passed. Shard 1 was cancelled
behind it.

## What it is

The refusal is correct: it is issue 1936's configure-time floor doing its job.
The job, though, never provisions the pinned cross compiler. It runs
`just setup native`, which installs no `arm-none-eabi-gcc`, so the copy-out
build resolves the runner image's distro `gcc-arm-none-eabi` (Ubuntu 22.04,
10.3.1) from `PATH`. All four failing rows are C++ images on
`mps2-an385-freertos`. The C-only FreeRTOS rows are warned, not refused, so
they pass.

## Why it shows up now

This is the first scheduled run whose shard 2 got this far. In the 11-06 to
11-09 runs the same job failed earlier: on 11-09 (job 128959187019),
`check::default` failed after 4 s and no build-tier gate ran. Issue 2382's
lane-claim fixes are what let the shard reach `template-copy-out`. The defect
predates those fixes; they only stopped masking it.

## What it is NOT

- Not a regression in the templates or in `NanoRosEntry.cmake`. The floor
  landed on 2026-10-17 (`c2b55ed069`).
- Not issue 2382's disk floor or its preflight. This shard passed both and
  ran 31 tests.
- Not #3438 (issue 2369). That caches arm-gcc for the merge-group copy-out
  jobs, which already provision it; it does not touch this host-tests shard.

## What would close it

The build-tier `template-copy-out` gate, wherever a lane runs it, resolves a
GCC 11 or newer cross compiler, or refuses up front with a named precondition
rather than failing four rows mid-shard. Candidate sites, any one of which
closes it:

1. The host-tests tier-1 build shard runs `nros setup --tool arm-none-eabi-gcc`
   before `just check build`. This is the smallest change, and #3438's cache
   key for that tool can be reused.
2. The `template-copy-out` gate declares the pinned cross compiler as a
   `needs:` precondition, so any lane lacking it reports a SKIP with a named
   remedy instead of a FAIL. This is phase-543's lane-contract shape.
3. The C++ FreeRTOS rows' setup step (`[setup] setup complete for
   target=freertos-zenoh`) provisions the tool itself, so a user copy-out
   behaves the same way.

Acceptance: a scheduled `host-tests` run whose shard 2 reports all four rows
passing (or skipped with a named remedy, under option 2).

## Fix (2026-11-10)

Option 2 of the three above, provisioned the way option 1 asks, which is
phase-543's lane-contract shape:

- A capability, `arm-gcc` (`.config/gate-lanes.toml`): the pinned
  `[tool.arm-none-eabi-gcc]` dist. The lane preflight probes it with the
  index's own resolver (`index_tool.py resolve`), which accepts the store dist
  or a PATH copy that passes the row's smoke probe, so the distro 10.3 does not
  count.
- The units declare it. `check-template-copy-out.sh` lists a unit as
  `<unit>\tneeds:arm-gcc` when one of its boards' `[board.<b>] packages` row in
  the index names `arm-none-eabi-gcc` (`index_tool.py board-packages`). That
  is the same list `nros setup <board>` provisions, so no unit is named by hand.
  Today it marks exactly the four rows that failed.
- The `build` lane claims it, and `scripts/ci/provision-gate-lane.sh build`
  runs `nros setup --tool arm-none-eabi-gcc`.
  `check-gate-lane-provisioned` counts `arm-gcc` among the claims a job must
  fetch, so a job running the lane without the provisioning step is refused.
- Lanes that do not claim it (`dev`, `queue`, `runner`) list the four units as
  not run, with "needs arm-gcc, which this lane does not claim", never as a
  failure and never as a pass. The merge-queue copy-out jobs run with no lane,
  so every capability is claimed there, and they provision the compiler
  themselves.

