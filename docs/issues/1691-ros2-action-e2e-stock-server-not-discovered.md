---
id: 1691
title: "`ros2_action_e2e` over zenoh: a stock ROS 2 `/fibonacci` server never appears
  in `ros2 action list` within 20 s, on a host that has rmw_zenoh and the example"
status: open
type: bug
area: [testing, rmw]
severity: medium
found: 2026-10-05
related: [1651, 1333, 1685]
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
