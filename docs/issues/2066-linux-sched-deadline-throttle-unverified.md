---
id: 2066
title: "The Linux SCHED_DEADLINE level throttle has never run: no CI host has CAP_SYS_NICE"
status: open
type: tech-debt
area: [executor, linux, testing]
severity: low
found: 2026-10-26
related: [phase-505, phase-509, rfc-0108, 2018]
---

**Home:** [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W1, since 2026-10-26.

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

## Closes when

The throttle half runs and passes somewhere it can: a CI job with
`CAP_SYS_NICE` (a privileged container, or `setcap cap_sys_nice+ep` on the
test binary), or a recorded manual run. If neither is possible, the RFC-0108
docs must state that the Linux kernel throttle is unverified.
