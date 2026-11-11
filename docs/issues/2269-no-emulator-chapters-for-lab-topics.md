---
id: 2269
title: "Five of the seven lab topics an instructor needs have no emulator chapter (QoS, service, parameters, lifecycle, real-time tiers)"
status: open
type: enhancement
area: book, education
purpose: [education]
related: [phase-537, phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-537](../roadmap/phase-537-book-user-guide-organization.md)** — W5, which absorbed phase-523 W1 (maintainer, 2026-11-06). Found by phase-523 W3 round one.

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
  (issue [2432](archived/2432-derived-sched-tasks-differ-by-language.md)).
- Lifecycle: blocked on issue
  [2428](2428-workspace-lifecycle-is-one-machine-per-image.md). A workspace
  image has one lifecycle state machine, named `/nros_app`, and no component
  can see its state, so a node publishes while `unconfigured`.
