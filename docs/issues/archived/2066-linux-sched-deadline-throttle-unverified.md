---
id: 2066
title: "The Linux SCHED_DEADLINE level throttle has never run: no CI host has CAP_SYS_NICE"
status: resolved
type: tech-debt
area: [executor, linux, testing]
severity: low
found: 2026-10-26
resolved: 2026-10-27
resolved_in: "PR #2727 (2026-10-27)"
related: [phase-505, phase-509, rfc-0108, 2018]
---

**Home:** [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md) W1, since 2026-10-26.

## Measured

Issue 2018 (PR #2616) puts every budgeted spawned level thread on Linux under
`SCHED_DEADLINE` (`sched_setattr`). Without `CAP_SYS_NICE` the kernel refuses
the reservation and the executor enforces the budget instead.

The test burns 20 ms of CPU at 2 ms per 10 ms and requires at least 80 ms of
wall time under the kernel. It has run only without the capability. On
that path it checks the fallback (the reservation is refused and the thread
stays off `SCHED_DEADLINE`) and prints that the throttle half did not run.
Neither the development host nor CI has granted the capability, so these
have never been observed:

- the kernel throttling a level thread;
- `Executor::defer_budget_to_kernel` handing a level over with no executor
  budget skip.

NuttX's half (`SCHED_SPORADIC`) has a QEMU test that passes.

## Findings (2026-10-27)

On the development host, the unprivileged run again took the fallback.
`nros_board_linux` `tests::a_level_past_its_deadline_budget_is_throttled_by_the_kernel`
passes and prints "SCHED_DEADLINE refused on this host (no CAP_SYS_NICE)":

- The process holds no capability (`CapEff 0`), and no binary on the host
  carries `cap_sys_nice`.
- The kernel's RT bandwidth is the default (`sched_rt_runtime_us` 950000 of
  1000000), so admission would accept the test's 2 ms / 10 ms reservation.
- The user is in the `docker` group. Docker is rootful (no userns remap,
  cgroup v1, the builtin seccomp profile), so a container given
  `--cap-add SYS_NICE` would hold a real `CAP_SYS_NICE` against the host
  kernel. That is the "privileged container" road above, not an emulation.
  The 2026-10-27 session was not permitted to start such a container, so
  the throttle half has still not run.

The manual run, for someone allowed to grant the capability. Build the test
binary once:

```
cargo test --manifest-path packages/boards/nros-board-linux/Cargo.toml --lib --no-run
```

Then run it either way (`<bin>` is the `nros_board_linux-<hash>` path cargo
prints):

```
docker run --rm --cap-add SYS_NICE -v <bin>:/t:ro ubuntu:22.04 /t --nocapture a_level_past_its_deadline
sudo setcap cap_sys_nice+ep <bin> && <bin> --nocapture a_level_past_its_deadline
```

A pass without the "refused" line is the throttle half. The
`defer_budget_to_kernel` hand-over is a second matter. No test runs a Linux
level entry with the capability and checks that the executor skips no budget,
so even a passing throttle run leaves that half unobserved. It needs its own
run (the native levels entry under the same capability) or the RFC-0108
statement below.

## Resolution (2026-10-27)

Both halves ran on the development host with a real `CAP_SYS_NICE`: rootful
docker, `--cap-add SYS_NICE`, `ubuntu:22.04` (the host's own userland), kernel
6.5 with `CONFIG_HZ=250`. They found two defects, both fixed here.

**1. The C/C++ level runner lost its reservation in about one run in twelve.**
`rt_sched_levels_e2e::linux_level_budget_overrun_is_throttled_by_the_kernel`
(the `linux_budget` image of `levels-cpp`) failed 5 of 62 runs with every
release burning 10 ms of CPU in 10 ms of wall time, although the console said
`deadline budget set` and `enforced by the KERNEL`. A temporary print of
`sched_getscheduler(0)` in the load callback, over 15 runs, showed policy 1
(`SCHED_FIFO`) on exactly the two failing runs and 6 (`SCHED_DEADLINE`) on the
rest.

The cause is a race in the spawn. `nros-cpp`'s native level runner spawns each
level through `PlatformTask::spawn_with(..., priority)`; the POSIX port
re-applies that priority with `pthread_setschedparam(child, SCHED_FIFO)` after
`pthread_create`, while the child's trampoline is already asking the kernel
for `SCHED_DEADLINE`. When the spawner's write lands second it replaces the
reservation, and since the level's context had deferred its budget to the
kernel, nothing enforced the budget at all.

Fix: the trampoline waits (sleeping, not spinning: under `SCHED_FIFO` on one
CPU a spinning level starves its spawner) until the spawner's `spawn_with` has
returned, then asks for the reservation. After the fix: 30 of 30 runs with the
probe, policy 6 every time, and 10 of 10 without it. The Rust Linux board is
not affected: its level threads are `std::thread`s that set their own priority
and reservation, with no write from the spawner.

**2. The throttle checks were tighter than a tick-enforced kernel.** The
kernel enforces a `SCHED_DEADLINE` budget at the scheduler tick unless
`HRTICK_DL` is on (it is off by default), so a slot can overrun to the next
tick (4 ms at HZ=250).

- The unit test burned 20 ms of CPU at 2 ms per 10 ms and required 80 ms of
  wall time; it measured 70 ms. It now burns 100 ms and requires 300 ms; the
  test finished in 0.38-0.43 s over six runs (a share of about 26%, not 20%),
  where an unthrottled burn takes about 100 ms.
- `throttle_verdict` required every release to take half its ideal wall time.
  With the reservation held, releases of 10 ms took 14, 22, 30 or 40 ms
  (mean 26-32 ms per run); one run in seven had a 14 ms release. The verdict
  now requires every release to wait off the CPU for a quarter of a refill
  (each one waited at least 3.9 ms; an unthrottled release never leaves the
  CPU) and the MEAN wall time to reach half the ideal.

**The hand-over.** Every run printed `level budget tier=capped 2000us/10000us
enforced by the KERNEL`, and with the race fixed every release was throttled,
so the executor's `defer_budget_to_kernel` is observed with the kernel holding
the budget. The unit test of the executor side was already in
`nros-node`'s executor tests.

**Still not in CI.** No lane claims `rt-sched`, so these runs are manual. To
repeat them: build the fixture with `scripts/build/workspace-fixtures-build.sh
linux --id workspace-cpp-native-levels-budget`, then run `cargo test -p
nros-tests --test rt_sched_levels_e2e linux_level_budget_overrun`; its
default launcher is docker with `--cap-add SYS_NICE`. The unit test runs the
same way as the manual run above, with the `nros_board_linux` test binary.

## Closes when

The throttle half runs and passes somewhere it can: a CI job with
`CAP_SYS_NICE` (a privileged container, or `setcap cap_sys_nice+ep` on the
test binary), or a recorded manual run. If neither is possible, the RFC-0108
docs must state that the Linux kernel throttle is unverified.
