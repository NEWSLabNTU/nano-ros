---
id: 2016
title: "The Rust Linux, FreeRTOS and NuttX tier arms still open one executor per tier"
status: resolved
type: tech-debt
area: [executor, linux, freertos, nuttx]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [phase-509, rfc-0108, 2020]
---

## Measured

phase-509 P10 moved every C-ABI tier runner, plus the Rust Zephyr and ThreadX
runners, onto levels: one executor per core and a worker per level, through
`nros::levels::LevelCore`. Three Rust tier arms were left behind:
`LinuxBoard::run_tiers`, FreeRTOS `run_tiers_entry` and NuttX `run_tiers`.
Each opened an executor per spawned tier over the shared session
(`Executor::open_with_session_slot` / `open_with_session_handle_slot`), so a
Rust image on these boards paid a context and a backing per tier and ordered
work only inside each executor. `LevelCore` had no teardown for the hosted
arm.

## Resolution

All three arms run on `LevelCore`, with the shape of the Zephyr and ThreadX
Rust runners.

- `nros::levels::LevelCore::into_runtime` is the teardown: once every other
  level's thread has been joined, the boot level takes the runtime back (the
  executor lock removed). `spin_level_once` / `spin_boot_once` return the
  pass's `SpinOnceResult`, which NuttX's heartbeat reads.
- **Linux** (`nros-board-linux`): the least urgent tier is the boot level,
  on the calling thread, and owns the session. Every other tier is a scoped
  `std::thread` worker, started once the level before it has set up. The lock
  is the POSIX port's recursive `PTHREAD_PRIO_INHERIT` mutex, keyed by
  `pthread_self`. A termination signal or `NROS_ENTRY_SPIN_MS` ends every
  level; the scope joins the workers, and `into_runtime` hands the runtime
  back so the session is closed before `run_tiers` returns. The census arm is
  unchanged.
- **FreeRTOS** (`nros-board-freertos`): the chained spawn is kept, each task
  a level worker. The lock is the FreeRTOS recursive mutex, keyed by a new C
  helper, `nros_freertos_current_task_id`.
- **NuttX** (`nros-board-nuttx`): the boot tier stays the session owner. The
  others are detached `sys::spawn_tier` pthreads, now chained (each starts the
  next after its setup). The lock is the POSIX recursive mutex, keyed by
  `pthread_self`. The non-owner levels keep their kernel `SCHED_SPORADIC`
  server; the owner still never takes one.
- Per-tier executor scheduling policy is now the boot level's alone, since
  the levels share one executor, as on Zephyr and ThreadX. Kernel facts stay
  per level thread. A spawned level whose setup fails fails the boot on all
  three arms. The tiers' executor backing is still checked (issue 1571) but
  unused.
- RFC-0108 "As built in P10" lists the three runners.

`realtime_tiers_e2e` rows run locally: native/rust, native/rust-derived,
freertos/rust and threadx_linux/rust pass. The nuttx-arm/rust and
nuttx-riscv/rust rows built and booted (both levels dispatch on the console),
but the host observer saw no `/telem` deliveries. The same rows fail the same
way on origin/main's NuttX board on this host, so that is this host's QEMU
network, not the change.

A native end-to-end check that an urgent level's action server ticks on its
own level (issue 2020's follow-up) is not added: no Rust level workspace has an
action server, and the host executor test from issue 2020 covers the
mechanism.
