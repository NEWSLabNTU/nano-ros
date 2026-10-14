# ws-violation-cpp - the violation channel, end to end (phase-474 T4)

One C++ component, `violation_pkg::Handler`, whose 10 Hz timer publishes
`std_msgs/Int32` on `/t4/state`. `src/demo_bringup/launch/system.contract.yaml`
bounds that path at `max_latency: 50ms`, which the generated Zephyr entry
installs as a `max-latency-runtime` monitor row.

The handler is an INIT/RUN state machine in the safety island's shape:

| tick | what it does | what the monitors do |
| --- | --- | --- |
| 1 | 80 ms of start-up work | not armed: counted in `suppressed_before_arm`, not stored |
| 3 | `rclcpp::arm_monitors()` (RUN) | armed |
| 6 | 150 ms overrun | one stored `max-latency-runtime /handler/state` verdict |
| 9 | its own take of tick 9's `/t4/avail` busy-waits 250 ms | tick 10 is late |

The native_sim conf states `CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and
`CONFIG_NROS_VIOLATION_DRAIN_REPORT=y`, so each stored verdict is a console
line `contract violation #<n>: <rule> <endpoint> measured=.. declared=..`.
`packages/testing/nros-tests/tests/violation_channel_e2e.rs` boots the image
under a router and asserts exactly one such line for the latency rule, after
the RUN line.

phase10-B3: each tick also publishes `/t4/avail`, which the handler takes
itself, and `/t4/answer`. The contract's `react` path runs from that take to
`/t4/answer` with a 180 ms budget, installed as a `route-latency-runtime`
row. An on-time route is one period (~100 ms). Tick 6's route (the take
waits for tick 6, then 150 ms of overrun) and tick 10's (late by the 250 ms
take) exceed it: the test asserts exactly those two route verdicts, both on
`/handler/answer`, and none from the callback-level rule on `/t4/answer`.

The busy-wait is `k_busy_wait` on Zephyr: on native_sim simulated time does not
advance while a thread spins on a clock, so a `steady_clock` loop never ends.

Build: `NROS_ZEPHYR_FIXTURE_FILTER=violation just zephyr build-fixtures`
(fixture `workspace-zephyr-cpp-violation`).
