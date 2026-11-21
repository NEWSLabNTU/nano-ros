---
id: 2792
title: "The C/C++ native levels runner never places zenoh-pico's transport tasks in the Linux transport band, so they stay on SCHED_OTHER under SCHED_FIFO levels"
status: resolved
type: bug
severity: medium
area: [linux, cpp, scheduling, zenoh]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(#2792): the C/C++ native runners place the transport in the Linux transport band"
related: [0765, 2617, 2796, 2797]
---

**Owner:** unassigned.

## What happens

The Linux priority plan (`packages/boards/linux/nros-board.toml`) puts
zenoh-pico's read and lease tasks in `reserved.transport = [90, 99]`, above
every sched task. The Rust runner does this: `LinuxBoard::run_sched_tasks`
calls `zpico_set_task_config(90, 0)` before the session opens, and its comment
says why. A SCHED_FIFO level outranks every SCHED_OTHER thread
unconditionally, so a transport left on SCHED_OTHER is preempted by the app
that publishes over it, always.

The C/C++ native runner, `nros_board_native_run_levels_in` in
`packages/api/nros-cpp/src/lib.rs`, has no such call (`git grep
zpico_set_task_config` finds none outside the boards). The boot thread census
of the levels-cpp image (`workspace-cpp-native-levels`), granted
`CAP_SYS_NICE` by the docker launcher, reads:

| Thread | Policy | Priority |
| --- | --- | ---: |
| `linux_entry` | OTHER | 0 |
| `linux_entry` | OTHER | 0 |
| `linux_entry` | FIFO | 89 |
| `linux_entry` | FIFO | 88 |

Two levels run SCHED_FIFO at 89 and 88, and nothing sits in the transport
band. The two SCHED_OTHER threads are the main thread, which runs the least
urgent level, and one other thread, which by elimination is the transport's.
The runner does not name its threads (every one is `linux_entry`), so the
census cannot name them; that it is the transport is inferred, not read.

The Rust runner's census of the realtime-rust image
(`workspace-rust-native-realtime`), same launcher, shows the placement the
C/C++ runner lacks:

| Thread | Policy | Priority |
| --- | --- | ---: |
| `linux_entry` (main, the boot level) | FIFO | 10 |
| `linux_entry` (the transport) | FIFO | 90 |
| `nros-sched-task` (a level; `comm` is cut at 15 bytes) | FIFO | 80 |

`linux_thread_census_is_placed_by_the_priority_plan` (in
`rt_sched_levels_e2e.rs`) passes on the levels-cpp image. It checks that every
real-time thread sits in a band and is not foreign, and a SCHED_OTHER thread
is outside every band by construction. It does not require the transport band
to be occupied.

## What closes it

- The C/C++ runner places the transport in the plan's transport band before
  the session opens, as the Rust runner does. It has to stay a no-op when the
  zenoh backend is not linked; the Rust runner's comment records the link
  failure an unguarded call produced.
- It names its level threads (`nros-sched-task-<name>`, as the Rust runner
  does), so the census can tell them apart.
- The census test then also requires a thread in `reserved.transport`.

## Resolved (2026-11-21)

The band floor and the decision moved to `nros::transport_band::place`, which
both Linux runners call before the session opens: the Rust board with its
feature-gated direct call, and both C/C++ native runners (`run_levels_in` and
`run_sched_tasks_in`) through
`packages/api/nros-cpp/c-stubs/native_transport_task_config.c`, a WEAK
reference to `zpico_set_task_config`. That reference resolves when the zenoh
backend is linked (zpico.c is whole-archived with it) and is NULL otherwise,
where the image has no transport to place. `check-sched-task-priority-plan`
now reads the floor from `nros::transport_band`.

The levels-cpp census after the change prints
`nros: transport tasks at SCHED_FIFO 90 (reserved band floor; most urgent sched task is 80)`,
and both census tests now also require a `SCHED_FIFO` thread in
`reserved.transport`. Both pass.

Not closed here: naming the level threads. The POSIX port never names any
thread it spawns, in either language, so this is the port's defect rather than
the runner's; filed as issue 2796. The census test does not need the names,
because it requires the transport band to be OCCUPIED, not a thread called the
transport.
