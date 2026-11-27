---
id: 3010
title: "On the FreeRTOS POSIX port the monotonic clock counts ticks, and the tick runs slow under host load"
status: open
type: bug
severity: medium
area: [platform, freertos, timing]
found: 2026-11-27
---

## What happens

On the `freertos-posix` board, nano-ros's monotonic clock is the FreeRTOS
tick count. Under host load the POSIX port's tick is delivered late and
never caught up, so the clock runs slow against real time. Every interval
measured on it stretches by the same ratio: input ages, watchdog and
deadline timeouts, timer periods and the control rate.

Observed in a CARLA-driven run on `freertos-posix`, at host load ~80-120:

- the island's tick-based log time fell ~10 s behind the host over ~440 s
  (~2.3 %);
- the control loop ran at 32.7 Hz against a nominal 33.33 Hz (~1.9 %);
- wall-clock stamps were correct (they are read from the host, not the tick).

## Why

`nros_platform_clock_ns` in `packages/platform/nros-platform-freertos/src/platform.c`
has a sub-tick path only for Cortex-M SysTick. Every other build, the POSIX
port included, returns `xTaskGetTickCount() * NS_PER_TICK`
(`configTICK_RATE_HZ` = 1000 in the board's `FreeRTOSConfig.h`).
`nros-core`'s clock (`packages/core/nros-core/src/clock.rs`) reads that
symbol for the steady clock, so steady time, timeouts and timers are all
tick-backed.

The POSIX port's tick source (`third-party/freertos/kernel/portable/ThirdParty/GCC/Posix/port.c`,
`prvTimerTickHandler`) is a host thread that loops `pthread_kill(SIGALRM)`
then `usleep(portTICK_RATE_MICROSECONDS)`. Each iteration's period is the
sleep plus the thread's wakeup latency plus the signal delivery, and one
signal advances the count by exactly one tick (`xTaskIncrementTick`). There
is no catch-up against `CLOCK_MONOTONIC`, so wakeup latency on a loaded host
is lost time, permanently. The port already reads `CLOCK_MONOTONIC`
(`prvGetTimeNs`), but not to drive the tick.

On hardware the tick is a timer interrupt and this does not arise; the
defect is specific to the host-simulated port.

## Consequence

On a loaded host, a deadline written as N ms waits longer than N ms of real
time, and a rate measured against the host comes out low. A test or a
closed-loop simulation that compares island time with host time (CARLA,
a ROS 2 peer's stamps) sees the island fall behind steadily.

## Ruling (user, 2026-11-27): tick catch-up in the port

Patch the FreeRTOS POSIX port on our FreeRTOS fork's patch-line branch.
The tick thread counts elapsed ticks from host `CLOCK_MONOTONIC` and
delivers every tick it owes, instead of one tick per `usleep`. That keeps
one time base for nano-ros's clock and for FreeRTOS's own delays and
timeouts.

Rejected: backing `nros_platform_clock_ns` with host `CLOCK_MONOTONIC`
while the tick stays as it is. The clock would stay true, but tick-based
delays (`vTaskDelay`, blocking timeouts) would still stretch. A period
computed from the clock and slept in ticks would then disagree with itself
under load.

Fork rules apply: the patch goes on a patch-line branch with `branch =`
recorded in `.gitmodules`, and the fork push needs the maintainer's OK.
As of 2026-11-27, `third-party/freertos/kernel` pins the upstream
`FreeRTOS/FreeRTOS-Kernel` with no `branch =`, so no fork exists yet. A
fork and its patch-line branch have to be created before the first patch
lands, and the submodule repointed at them. The maintainer creates the
fork under their own account, with a `nano-ros` branch cut at the pinned
commit. Once they confirm it exists, the submodule's `url` and
`branch = nano-ros` are repointed. The patch is committed on that branch,
and the maintainer pushes it.

Check what the other host-simulated RTOS port (threadx-linux) does with the
same question. If it has the same defect, file it.

## Acceptance

- A test loads the host deliberately and measures FreeRTOS tick time
  against host `CLOCK_MONOTONIC`. Drift stays under 0.1 % over a minute.
  The unloaded case is the control.
- The ASI control loop on `freertos-posix` holds 33.33 Hz under host load.

## Measured, with the ruled fix applied locally (2026-11-28)

The patch is committed on a local `nano-ros` branch of the kernel
submodule, and is unpushed until the fork exists. Harness: a
standalone program built from the pinned kernel sources with this board's
`FreeRTOSConfig.h`. One task runs `vTaskDelay(pdMS_TO_TICKS(1000))` 60
times, and the tick count's elapsed time is compared with host
`CLOCK_MONOTONIC`. Host: 20 cores.

| | load avg | tick time vs host | worst 1000 ms delay |
|---|---|---|---|
| unpatched, ambient | ~5 | 9.31 % slow | 1145.7 ms |
| unpatched, 40 busy loops | 14 → 37 | 12.60 % slow | 1328.0 ms |
| patched, ambient | ~5 | −0.000 % | 1000.2 ms |
| patched, 40 busy loops | 14 → 37 | −0.001 % | 1008.0 ms |

The unpatched port loses time even on a lightly loaded host: each
`usleep(1000)` overshoots, and nothing recovers the overshoot. That is
larger than the ~2.3 % first seen, which came from a different host and
load. Patched, the drift is about 100 times inside the 0.1 % acceptance
bound. The control-rate half of the acceptance (ASI holding 33.33 Hz) and a
committed regression test are still to do.

The other host-simulated port has the same defect: issue 3017.
