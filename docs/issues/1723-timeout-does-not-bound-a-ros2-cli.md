---
id: 1723
title: "`timeout N ros2 …` does not bound a ros2 CLI that is waiting: it ignores the SIGTERM, so the deadline never fires"
status: open
type: bug
area: testing
severity: medium
found: 2026-10-06
related: [issue-0659, issue-1139, phase-480]
---

## What happens

GNU `timeout N cmd` sends `SIGTERM` at the deadline, waits for the child, and
sends `SIGKILL` only when given `-k`. A waiting `ros2` CLI does not always die
on `SIGTERM`. Measured 2026-10-06 while working on issue 1352:

- `timeout 25 ros2 service call /param_talker/set_parameters …`, against a
  server that dropped the request, was still running **20 minutes** later.
  Only `SIGKILL` ended it. `timeout -s KILL 25` bounds the same call at 25 s.
- On the same host, four `timeout 20 ros2 topic echo … /diagnostics`
  processes started on 2026-10-03 were still alive three days later, each
  holding a DDS/zenoh participant. They are not from this work.

## Why it matters

Several harness sites rely on `timeout N ros2 …` as the bound on a peer:
`tests/entry_e2e.rs`, `tests/interop_e2e.rs`, the three `*_ros2_interop_e2e.rs`
cells, `docker/can-demo/entrypoint.sh`, and the `bash -c "<env> && timeout N
ros2 run …"` chain that `process.rs` describes. When the CLI ignores the
signal, the cell hangs until something outside it gives up, and the orphan
keeps a participant on the bus. Issue 0659 is that same orphan, reached by a
different path.

It is also a candidate cause for issue 1139's unexplained wall clock (one
in-gate pass took 199.7 s against a typical 3 s), but nothing here shows it
caused that run.

## Not established

- Which `ros2` verbs ignore `SIGTERM`, and when. Seen with `service call`
  waiting for a reply and with `topic echo`. rclpy installs its own signal
  handling, so this may depend on where the CLI is when the signal lands.
- Whether `-k` (a kill-after grace) or `-s KILL` is the right spelling for the
  harness. The class fix is ONE helper that every site uses, not a flag added
  at each site.

## Sweep

```
git grep -nE "timeout [0-9\"\$A-Z_{}]+ +(env [^|]*)?ros2 " | grep -v -e "-k " -e "-s KILL"
```
