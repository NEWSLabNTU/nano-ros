---
id: 2600
title: "`nros_rmw_cyclonedds_ros2_pubsub_e2e` A.1 captures nothing in its 60 s deadline at load ~50, and `ros2 daemon stop` alone takes 73 s"
status: open
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
