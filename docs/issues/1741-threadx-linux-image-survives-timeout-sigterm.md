---
id: 1741
title: "A threadx-linux image catches SIGTERM and keeps running, so `timeout N`
  does not bound it — one lived 34 days"
status: open
type: bug
area: [testing, platform]
severity: low
found: 2026-10-07
related: [1723, 0659, phase-480]
---

## What was measured

While clearing issue 1723's orphans (2026-10-07), this host still had, from
2026-09-03:

    timeout 6 examples/threadx-linux/c/service-server/build-cyclonedds/c_service_server

`timeout` (pid 1702514) in `sigsuspend`, its child `c_service_server`
(9 threads, state S) alive 34 days later. The child's `/proc/<pid>/status`:
`SigIgn 0`, `SigBlk 0`, `SigCgt 0x100004a02` — SIGTERM (bit 15) is CAUGHT.
So `timeout`'s SIGTERM reached a handler that did not end the process, and
with no `-k` nothing followed. cwd was `/home/aeon/repos/nano-ros`.

## Why it is filed

Same class as issue 1723 one layer over: a deadline that sends one SIGTERM
bounds nothing for a process that handles it. 1723 fixed the ROS 2 side and
gated it (`check-ros2-cli-deadline`, `ros2`/`python3` only). A nano-ros
image is our own code, so the better fix may be in the image — the ThreadX
Linux port drives its scheduler with signals, and which handler swallows
SIGTERM was NOT established.

## Not established

- Which handler catches SIGTERM (ThreadX's Linux port, the cyclone backend,
  or the board crate), and whether every threadx-linux image does.
- Which harness site started this one (`timeout 6` appears in no tracked
  file today: `git grep -n "timeout 6 "`).

## Shape of a fix

Either the image exits on SIGTERM (preferred: it is our code), or every
harness deadline on a nano-ros image escalates like `ros2_deadline` does.
The orphan was left running for whoever reproduces this (pid 1702516).
