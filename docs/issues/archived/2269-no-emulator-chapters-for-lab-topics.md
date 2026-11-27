---
id: 2269
title: "Five of the seven lab topics an instructor needs have no emulator chapter (QoS, service, parameters, lifecycle, real-time tiers)"
status: resolved
type: enhancement
area: book, education
purpose: [education]
related: [phase-537, phase-523, rfc-0116, phase-554]
found: 2026-11-05
resolved: 2026-11-27
resolved_in: "the lifecycle lab (tutorial/lifecycle.md), the last of the seven topics, replayed by three weekly QEMU tracks"
---

**Home: [phase-537](../../roadmap/phase-537-book-user-guide-organization.md)** — W5, which absorbed phase-523 W1 (maintainer, 2026-11-06). Found by phase-523 W3 round one.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E11, task 2's coverage table: publish (talker only, no subscriber walkthrough) and the ROS 2 graph are covered; QoS (API page only), services (`nros new --template server/client` plus GitHub pointers), parameters and lifecycle (Linux-host pages only) and real-time tiers (one ThreadX paragraph; rt-execution-model uses contributor-only `cargo nextest -p nros-tests`) have none.

This is phase-523 W1's work list; the probe confirms the gap from the reader's side.

## Next

phase-537 W5's Tutorial, with the labs it absorbed from phase-523 W1.

Progress (2026-11-11):
- QoS: `book/src/tutorial/qos.md`, "Lab: QoS is checked twice", on three
  weekly probe tracks (`lab-qos`, `lab-qos-c`, `lab-qos-cpp`).
- Real-time tiers: `book/src/tutorial/two-tiers.md`, "Lab: two sched tasks
  keep a promise", on three weekly probe tracks (`lab-tiers`, `lab-tiers-c`,
  `lab-tiers-cpp`), all on `mps2-an385-freertos`. It authors `[tasks]`
  explicitly, because with none authored the schedule differs by language
  (issue [2432](2432-derived-sched-tasks-differ-by-language.md)).
- Lifecycle: blocked on issue
  [2428](../2428-workspace-lifecycle-is-one-machine-per-image.md). A workspace
  image has one lifecycle state machine, named `/nros_app`, and no component
  can see its state, so a node publishes while `unconfigured`.

Progress (2026-11-21): six of the seven topics are covered. Services and
parameters are covered by Tutorial ch6 (`tutorial/services-and-actions.md`)
and ch7 (`tutorial/parameters-and-launch-files.md`), both probed on QEMU
(`mps2-an385-freertos`). Publish and the ROS 2 graph were covered at filing,
and QoS and real-time tiers by the two labs above. Only lifecycle remains,
blocked on issue [2428](../2428-workspace-lifecycle-is-one-machine-per-image.md)
and phase-554 W6/W9.

## Resolution (2026-11-27)

The seventh topic, lifecycle, is "Lab: a managed node waits for its cue"
(`book/src/tutorial/lifecycle.md`). It continues from Tutorial 7's
workspace, on the Tutorial's QEMU board (`mps2-an385-freertos`, zenoh).
`sensor_node` becomes a `<lifecycle_node>` in the launch file, and the
bringup gains `[lifecycle] autostart = "none"`, with no code change. Stock
`ros2 lifecycle` commands from the host then drive it:

- the boot is silent, and the dropped-publish line prints;
- `nodes` and `get` show `unconfigured`;
- `configure` gives `inactive`, `activate` gives `active`, and `deactivate`
  gives `inactive` again;
- an illegal transition is refused with exit 1;
- `list` names the transitions open from `inactive`.

Three weekly tracks, `lab-lifecycle`, `-c` and `-cpp`, replay it after
Tutorial 1-7, and all three passed on 2026-11-27. The behaviour it
teaches, publishers gated on the node's state, was fixed under issue 2428
(phase-554 W6). Its embedded acceptance is the FreeRTOS QEMU lifecycle
cells of #4979.

All seven topics now have an emulator chapter: publish and the graph
(Tutorial 1-3), QoS (Lab), services (ch6), parameters (ch7), real-time
tiers (Lab) and lifecycle (Lab).
