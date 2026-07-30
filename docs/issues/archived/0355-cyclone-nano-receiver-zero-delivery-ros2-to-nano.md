---
id: 355
title: "CycloneDDS ROS 2 → nano interop: the nano-cyclone RECEIVER gets 0 messages (pubsub ros2→nano + service nano-server), while nano→ROS 2 TX delivers"
status: resolved
type: bug
severity: medium
area: rmw
related: [phase-324, issue-0146, issue-0324]
resolved_in: (nros-c executor spin loops)
---

## Observation (phase-324 live validation, 2026-07-31)

Running `interop_e2e` live on a humble host (overlaid `rmw_zenoh_cpp` + cyclone +
XRCE Agent), with `NROS_SKIP_FIXTURE_CHECK=1` to get past the mtime treadmill:

```
case_6_cyclone_pubsub_nano_to_ros2 ... ok     # nano TALKER → ros2 echo
case_7_cyclone_pubsub_ros2_to_nano ... FAILED # ros2 pub → nano LISTENER
case_8_cyclone_service_nano_server ... FAILED # ros2 client → nano SERVER
```

`case_7` fails fast (~4 s, not a timeout) at `output.rs:385`:

```
Listener: expected at least 1 received messages, got 0.
nros C Listener
Node created: listener
Total messages received: 0
```

So the asymmetry is clean: **nano-cyclone TX works** (case_6 — a nano talker
reaches `ros2 topic echo` over `rmw_cyclonedds_cpp`), but **nano-cyclone RX
delivers nothing** — the nano C listener (and the nano service server, which is
also a receiver of the request) never sees the ROS 2 side's samples.

All zenoh interop (both directions, pub/sub + service) and the lifecycle cycle
pass; only the cyclone RECEIVER path is dark. This is independent of phase-324
(that phase touched the test-matrix SSoT, not the delivery path — the per-case
binding asserts pass for every case).

## MUST rule out first (stale fixture, not a product bug)

The run used `NROS_SKIP_FIXTURE_CHECK=1`, so the cyclone C listener binary
(`examples/native/c/listener/build-cyclonedds/c_listener`) may have been a
genuinely stale build (the freshness probe flagged it against recent codegen /
core changes). A stale `c_listener` with a real ABI skew would also present as
0-delivery.

**Step 1 (blocking):** rebuild the cyclone native C fixtures TRULY fresh
(`just build-test-fixtures` for the native cyclone family, or
`just cyclonedds setup` + the native-c-cyclonedds leaf) and re-run
`case_7`/`case_8` WITHOUT `NROS_SKIP_FIXTURE_CHECK`. If they deliver, this is a
fixture-staleness artifact — close as not-a-bug and note it. If they still get
0, it is a real cyclone-rx interop defect and the investigation below applies.

## Investigation leads (if real)

- Domain: the pair bakes/derives a domain; a nano receiver on the wrong
  `ROS_DOMAIN_ID` sees no SPDP from the ROS 2 pub (cf. the phase-180 cyclone
  split-brain and issue 0161 domain pinning).
- RxO / QoS: issue 0146 showed a BEST_EFFORT ros2 pub vs a RELIABLE nano sub
  drops on the publisher side. Check the ros2 `topic pub` QoS vs the nano C
  listener's requested QoS — a RELIABLE/BEST_EFFORT mismatch is a silent
  publisher-side drop, and would reproduce ONLY on the rx direction (matching the
  symptom).
- Reader discovery: nano→ros worked, so SPDP/wire is fine one way; the nano
  READER's endpoint may not be announced or matched (writer-side match count 0).
  Trace with `NROS_RMW_TRACE_OPEN=1` and cyclone's `CYCLONEDDS_URI` tracing.
- Type hash: humble vs the baked edition RIHS01 — a keyexpr/type mismatch drops
  the sample. nano→ros passing argues against this, but the reader topic
  registration path differs from the writer's.

## Repro

```
source /opt/ros/humble/setup.bash && source ./activate.sh
# (after a TRUE fresh cyclone fixture build)
cargo test -p nros-tests --test interop_e2e cyclone_pubsub_ros2_to_nano \
  -- --test-threads=1 --nocapture
```

## Root cause + resolution (2026-07-31)

NOT a stale fixture and NOT cyclone-rx wire: with a publisher present from t=0 the
nano cyclone C listener receives every sample (verified: 50/50, 24/24). The defect
is in the C executor's blocking spin loops.

`nros_executor_spin` / `nros_executor_spin_period` (`packages/core/nros-c/src/executor.rs`)
detected a "dead session" by counting consecutive non-`OK` returns from
`nros_executor_spin_some`. But `spin_some` returns `NROS_RET_TIMEOUT` on **every
idle tick** (`SpinOnceResult::any_work() == false`) against a perfectly LIVE
transport. So a healthy listener that simply idled — the normal state before its
publisher is discovered — accumulated `SPIN_ERROR_TOLERANCE` (16) idle returns and
the loop bailed after ~16 × 100 ms ≈ 1.6 s, shorter than DDS SPDP discovery. It
surfaced on cyclone ros2→nano because the test starts the listener 3 s before the
ROS 2 publisher; zenoh interop uses a **Rust** listener (different spin path) and
so was unaffected.

The executor already tracks the CORRECT signal — `Executor::session_io_failures()`,
incremented only on a genuine `drive_io` error and reset on any successful drive
(issue 0324). Cyclone's `session_drive_io` returns `NROS_RMW_RET_OK` on idle
(it owns its RX threads), so the counter stays 0 while idle. The C spin loops had
reimplemented dead-session detection against the wrong proxy instead of reading it.

**Fix:** both C spin loops now gate the dead-session bail on
`get_executor(...).session_io_failures() >= SPIN_ERROR_TOLERANCE` — the real
`drive_io` health counter — not on `spin_some`'s idle `NROS_RET_TIMEOUT`. Idle no
longer counts; a genuinely dead transport (drive_io returning `Err` every tick,
e.g. zenoh-pico's router gone) still trips the same threshold, preserving 0324.

**Verified:** `interop_e2e` cyclone `case_6/7/8` all green (were 7/8 red);
full `interop_e2e` 10/10; regression test
`executor::tests::idle_spins_never_raise_session_io_failures` (nros-node, tier 1)
proves 64 idle spins leave `session_io_failures()` at 0. nros-c 26 + nros-node
executor 184 unit tests green.
