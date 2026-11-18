---
id: 2600
title: "`nros_rmw_cyclonedds_ros2_pubsub_e2e` A.1 captures nothing in its 60 s deadline at load ~50, and `ros2 daemon stop` alone takes 73 s"
status: resolved
resolved_in: "the change that archives this file"
type: bug
area: [testing, rmw-cyclonedds]
severity: low
found: 2026-11-15
related: [1139, 1009, 1333]
---

**Standalone.** This is the cell issue 1139 resolved on 2026-10-06. The stage
stamps 1139 added are what make this report readable.

## What happens

`just ci gate` → `check::build` → `rmw-cyclonedds`, ctest test 30, on a
32-core host with load about 50. Two `just ci gate` runs and a metadata-probe
build were running on the host at once.

```
[t+73s] ros2 daemon stop returned
[t+73s] ros2 peer rmw load check returned
=== 117.12.A.1: nros pub → ros2 echo ===
...  Killed  ... ros2 topic echo --no-daemon --csv /chatter std_msgs/msg/String
[t+138s] A.1 verdict reached (captured=0); stopping the echo
FAIL: ros2 echo did not capture expected payload in 60s
  publisher attempts: 9 (0 exited non-zero), last rc=0
=== 117.12.A.2: ros2 pub → nros sub ===
note: subscriber attempt 1..4 exited rc=6 (no sample in its 10s budget)
PASS: nros sub captured 'hello-from-ros2!' (attempt 5)
```

- **A.1 (our pub → `ros2 topic echo`): FAIL.** The publisher ran nine times
  and exited 0 each time, so it opened a session and published. Within the
  60 s deadline, the echo wrote nothing.
- **A.2 (`ros2 topic pub` → our sub): PASS,** but only on the fifth 10 s
  subscriber window.
- **`ros2 daemon stop`** takes 73 s. It runs before either deadline starts,
  so it does not eat a sub-case's budget. It does show the scale: a single
  Python `ros2` CLI start cost about 70 s on this host at this load.

The same commit passed solo at load about 19, with the test taking 28.12 s.
That run was the failed and unrun steps only, not the whole gate. The PR
touched no Cyclone code (gate.yml and docs only).

## What this says, and what it does not

- 1139 removed two mechanisms: the LAN bus, and timing constants that assumed
  a fast CLI start. This run is on loopback (`bus pinned to loopback`), so the
  first mechanism is not back.
- One reading fits every line: the echo's own startup (Python import, rclpy
  init, Cyclone participant, discovery) did not finish inside 60 s. The
  daemon stop measured that startup at about 70 s on its own, and A.2's slow
  windows are the same delay seen from the other side. **Not established:**
  whether the echo had started at all when it was killed. `ECHO_ERR` held
  nothing, and no stamp marks "echo subscribed".
- So this is the host, not the backend. But a red that reads "did not
  capture expected payload" says the opposite, and that is 1139's complaint
  again.

## Where to look first

1. **Stamp the echo's readiness.** The echo could log once it is subscribed,
   or a `ros2 topic info /chatter -v --no-daemon` probe could run first. That
   tells "CLI never came up" apart from "came up and got no data" in the
   verdict line.
2. **Start the deadline when the echo is ready, not when it is launched.**
   Then a slow CLI start costs wall time, not the verdict. Keep an outer cap
   so a broken backend still fails.
3. **`ros2 daemon stop`:** the `--no-daemon` queries below it no longer need a
   daemon gone (issue 1333). Measure whether the stop is still needed; it is
   the cell's slowest step under load.

## Workaround

Run one gate per host. A red at high load gets the whole gate re-run solo
before anyone reads it as a backend regression.

## Resolution (2026-11-18)

**A `ros2` CLI's start is no longer inside a behaviour deadline.** Each
sub-case that drives a Python `ros2` command waits for it to be UP before its
deadline starts, under a separate cap (`NROS_E2E_START_CAP_S`, 300 s) that
has its own verdict. "Up" is a process in the CLI's tree holding a UDP
socket: Cyclone opens its sockets when rclpy creates the node, and an
importing or initialising Python holds none. The tree is read from `/proc`,
with no procps. The helpers are in `ros2_e2e_common.sh`:
`nros_pid_tree`, `nros_tree_has_udp_socket`, `nros_ros2_wait_up`.

Applied to every such site in the two Cyclone `ros2` e2e scripts:

- A.1: `ros2 topic echo`, the case this issue saw fail;
- A.2: `ros2 topic pub` (this issue's slow subscriber windows);
- B.1 in `ros2_srv_e2e.sh`: `ros2 service call`, which ran under a 15 s
  deadline that also covered its start. A slow start there read as "nros
  server still had no request", a server failure.

A CLI that never comes up now fails with "did not come up within N s (no DDS
socket); a host too loaded to start a ros2 CLI, not a delivery result",
first in its verdict chain.

**Proof, deterministic** (newslab-118). A `sitecustomize.py` on
`PYTHONPATH` delays only those three CLI commands at Python start-up, which
is this issue's slow CLI start without host load. Each run used a 20 s delay
and `NROS_E2E_DEADLINE_S=10`:

| tree | pub/sub (A.1, A.2) | srv (B.1) |
| --- | --- | --- |
| before | both FAIL ("did not capture expected payload in 10s", "took no sample in 10s") | FAIL ("nros server still had no request") |
| after | both PASS | PASS |
| after, `NROS_E2E_START_CAP_S=5` | both FAIL "did not come up within 5s" | FAIL "did not come up within 5s"; B.2 unaffected |

Unloaded, the scripts pass as before (pub/sub 3.2 s).

**Not changed:** `ros2 daemon stop` before the deadlines (this issue's item
3). It costs wall time under load but no verdict, since it runs before any
deadline starts.

Sweep: `git grep -nE 'ros2 (topic (echo|pub)|service call)' -- '*.sh'`.
The other hits are a probe (`scripts/probe/verify-zenoh-interop.sh`, a
60 s deadline around its echo, outside the merge-gating lanes), a twin smoke
script and repro scripts. They are not changed here.
