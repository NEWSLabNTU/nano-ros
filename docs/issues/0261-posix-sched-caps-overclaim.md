---
id: 261
title: "posix SchedCaps overclaim: realizer records Native edf/reservation/affinity while the posix board applies nothing natively"
status: open
type: bug
severity: medium
area: orchestration
related: [rfc-0052, issue-0259, phase-302]
---

## Finding (implementation-completeness audit, 2026-07-25)

`sched_caps_for("posix")` claims `edf: true, reservation: true,
affinity: true` (`packages/cli/nros-cli-core/src/orchestration/`
`rtos_realizer.rs` ~118-125 — the Linux SCHED_DEADLINE story), so L1
realization records mark deadline/budget/placement **Native** on posix
deploys. But `nros-board-posix`'s `run_tiers` applies NOTHING natively —
no `sched_setattr`, no `pthread_setaffinity_np`, not even
`sched_setscheduler` for priority (its own doc admits priority is
advisory). The executor SchedContext backfill is the sole enforcement.

A "Native" record that is really a backfill is exactly the mislabeled
honesty gap phase-296 W5.5 fixed for Zephyr EDF — same class, posix arm.

## Fix direction

Either (a) truth the caps: posix caps become `edf/reservation/affinity:
false` until real consumers exist (records then say Backfill/Degrade —
accurate), or (b) build the consumers (`sched_setattr` SCHED_DEADLINE /
SCHED_FIFO + affinity on the tier threads — root/CAP_SYS_NICE gated,
fail-loud fallback like the W5.7 pattern). (a) is the honest quick fix;
(b) is phase-162 territory. Phase-302 W1 takes (a).
