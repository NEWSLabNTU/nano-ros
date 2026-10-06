---
id: 1691
title: "`ros2_action_e2e` over zenoh: a stock ROS 2 `/fibonacci` server never appears
  in `ros2 action list` within 20 s, on a host that has rmw_zenoh and the example"
status: resolved
type: bug
area: [testing, rmw]
severity: medium
found: 2026-10-05
related: [1651, 1333, 1685, 1342, phase-480]
---

## What fails

`nros-tests::ros2_action_e2e the_nano_ros_action_client_drives_a_stock_ros2_server_over_zenoh`:

    the stock ROS 2 `/fibonacci` action never appeared in `ros2 action list`
    within 20 s, so the goal below could only have failed for a discovery
    reason wearing a wire-format costume. Last listing:
    <empty>

## Evidence and reach

- Local, SOLO (`-j1`), Ubuntu 22.04 with ROS Humble, `rmw_zenoh_cpp`
  (`rmw_zenohd` present) and `examples_rclcpp_minimal_action_server`
  installed; fixtures built from the tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38. Red, 21 s. Also red in the full
  local tier-1 `test-all`.
- On the `host-tests` runner (run 37252649866) all four `ros2_action_e2e`
  cases SKIP — the image lacks the ROS example package (issue 1685) — so
  CI cannot currently see this one way or the other.

## Lead, not a diagnosis

`await_fibonacci_action` is the one query that cannot pass `--no-daemon`
(`ros2 action list` rejects it), and the ros2cli daemon is keyed on
`ROS_DOMAIN_ID` alone and serves whatever RMW / discovery config its starter
had (issue 1333). The test's comment argues a unique domain makes that safe;
over zenoh the daemon also needs the router config the test sets per process,
which is the half 1333 measured going missing. Check whether a daemon started
under the zenoh env sees the server before touching the client.

## Acceptance

The case passes solo on a host with the peer installed, and the `host-tests`
image either installs the peer or a lane that has it runs the case.

## Resolution — 2026-10-06 (phase-480 W3)

**Root cause: the ros2cli daemon, exactly as 1342 measured.** The lead was
right. `ros2 action list` rejects `--no-daemon` on Humble, so the gate always
asked the domain-0 daemon, and the zenoh cells run on a FIXED domain 0
(`ZENOH_CELL_DOMAIN`) where 1333's `unique_ros_domain_id` defence never
reaches. A daemon a previous zenoh cell started is connected to that cell's
router, which no longer exists, so it reports an empty graph.

Measured by hand with the harness's session-config shape (client mode,
connect to one router, multicast scouting off), stock
`examples_rclcpp_minimal_action_server` on `rmw_zenohd` (Humble):

| daemon | `ros2 action list` | `ros2 service list --no-daemon --include-hidden-services` |
|---|---|---|
| fresh, started under this router | `/fibonacci` | `/fibonacci/_action/send_goal` (+ cancel, get_result) |
| left by a run on a different router | **(empty)** | `/fibonacci/_action/send_goal` (+ cancel, get_result) |

With the default session config (peer mode, multicast on) the stale daemon
still saw the server through scouting, which is why the failure needs the
harness's isolating config to show.

**Fix** (`fix/1691-action-gate-without-daemon`): `await_fibonacci_action`
waits for `/fibonacci/_action/send_goal` through
`ros2_query_cmd(.., "service list --include-hidden-services")`, which appends
`--no-daemon`. `check-ros2-daemon-queries` drops its
`("ros2_action_e2e.rs", "action list")` exemption, so a daemon-backed
`ros2 action list` cannot be written anywhere in the tree.

**Before / after**, solo, on this host (Ubuntu 22.04, Humble, `rmw_zenoh_cpp`,
the example package installed), with a stale zenoh daemon deliberately left on
domain 0:

- origin/main's test file: `the_nano_ros_action_client_drives_a_stock_ros2_server_over_zenoh`
  FAIL after 21.6 s, "never appeared in `ros2 action list`" — this issue's red.
- this branch: all five `ros2_action_e2e` cases PASS (n2r zenoh 61.7 s, r2n
  zenoh 7.1 s, both Cyclone cases, `cases_bound_to_interop_cells`).
- gate negative control: `check-ros2-daemon-queries` refuses origin/main's test
  file.

Sweep: `grep -rn 'ros2 action list\|ros2 action info' packages scripts tests`
— this was the only site that ran one.

**Not measured / left behind:**

- The `host-tests` CI image still lacks
  `ros-humble-examples-rclcpp-minimal-action-server`, so CI still SKIPS these
  cases (issue 1685). The acceptance's "the image installs the peer, or a lane
  that has it runs the case" is not done here.
- Not run in a parallel `test-all`: the fix removes the daemon from the gate,
  so the parallel case has nothing left to share, but that is reasoned, not
  measured.
