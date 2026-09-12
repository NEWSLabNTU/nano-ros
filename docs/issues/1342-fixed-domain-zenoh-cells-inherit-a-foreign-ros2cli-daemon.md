---
id: 1342
title: "issue 1333's remedy 2 defends only cells with a UNIQUE domain, and the
  zenoh action cells are on a FIXED domain 0 — so they inherit whatever daemon
  domain 0 already has"
status: open
type: bug
area: testing
severity: medium
related: [issue-1333, issue-0763, issue-0707, issue-1127, phase-455]
---

## What was measured

`native-action-rust-zenoh-{r2n,n2r}` (phase-455 W3) were run against a live peer
for the first time on 2026-09-12. Both failed at the same gate:

```
thread '…_over_zenoh' panicked at packages/testing/nros-tests/tests/ros2_action_e2e.rs:119:5:
the stock ROS 2 `/fibonacci` action never appeared in `ros2 action list` within 20 s
```

for the n2r case, whose peer is a **stock** `examples_rclcpp_minimal_action_server`
— nano-ros is not in the loop at that gate at all.

Reproduced outside the harness, on a clean graph with a live `rmw_zenohd` and
nothing else running (`tmp/live/n2r-v5.sh`, `n2r-v6.sh`):

```
=== ros2 node info /minimal_action_server --no-daemon:
  ...
  Action Servers:
    /fibonacci: example_interfaces/action/Fibonacci
=== ros2 service list --no-daemon --include-hidden-services | fibonacci:
/fibonacci/_action/cancel_goal
/fibonacci/_action/get_result
/fibonacci/_action/send_goal
=== ros2 action list -t:
                                     <-- empty
```

and then, with one command inserted:

```
=== ros2 daemon stop / start:
The daemon has been stopped
The daemon has been started
=== ros2 action list (daemon-only verb):
/fibonacci
```

With `ros2 daemon stop` before the test, the n2r case PASSES (62 s, twice).
It is now recorded that way in `.config/interop-verdicts.toml`, with the
workaround named in `where`.

## Why 1333's remedy does not reach these two cells

Issue 1333 closed this class with three parts. Part 1 (`--no-daemon` at one
construction point) explicitly **cannot** cover `ros2 action list`, which
rejects the flag on Humble — 1333 says so and allowlists `ros2_action_e2e.rs`,
"defended by remedy 2 instead".

Remedy 2 is `domain_daemon_port_busy`: `unique_ros_domain_id()` skips a domain
whose `11511+d` is listening, so a test that takes a unique domain never lands
on a foreign daemon.

**The zenoh action cells do not take a unique domain.** `ros2_action_e2e.rs`
declares `const ZENOH_CELL_DOMAIN: u8 = 0` with a correct reason: the nano side's
domain is a COMPILE-TIME bake (`option_env!("NROS_DOMAIN_ID")`) and the
`linux/rust/zenoh` fixtures bake no value, so the peer must be on domain 0 or it
never sees the node. The isolation for those cells is the router's ephemeral
PORT, not the domain.

So the one cell family that cannot use `--no-daemon` is also the one family
remedy 2 cannot defend, and the two facts have the same cause: a fixed domain.
Every zenoh cell in the tree shares domain 0's daemon, each with its own
short-lived router and its own tempdir `ZENOH_SESSION_CONFIG_URI`, so the
daemon's captured configuration is the PREVIOUS cell's dead router — which is
1333's own mechanism, arriving through the door remedy 2 left open.

This is not a rare interleaving. A daemon lingers two hours
(`ros2cli.daemon.serve`, `timeout=2*60*60`), so after the first zenoh cell of a
session every later `ros2 action list` on domain 0 reads a stale graph.

## Why the obvious fix is not obviously right

`ros2 daemon stop` in the test is what 0763 rejected and 1333 restated: under a
parallel suite it is a cross-test kill, because the daemon is a singleton and
stopping it kills the one another test is mid-query against. The zenoh action
cases are in the serial `ros2-interop` nextest group, but other groups run
beside it.

Candidates, none picked here:

* **Ask a verb that can take `--no-daemon`.** `ros2 node info <node> --no-daemon`
  prints an `Action Servers:` section (measured above), and the hidden
  `…/_action/send_goal` service is in `ros2 service list --no-daemon
  --include-hidden-services`. Either is a daemon-free discovery gate with the
  same meaning, and `await_fibonacci_action` is the only caller.
* **Give the nano side a runtime domain** so the cells can take a unique one and
  remedy 2 applies. That is a fixture change (`NROS_DOMAIN_ID` bake), not a test
  change, and it would also remove the `ZENOH_CELL_DOMAIN` comment's reason for
  existing.
* **A precondition probe**: refuse to run when `11511+0` is listening and the
  daemon is not ours. Turns a silent 20 s discovery timeout into a named
  precondition, without changing what the gate asks.

## Cost meanwhile

Any live-peer lane run that reaches a zenoh cell before `ros2_action_e2e` makes
`native-action-rust-zenoh-*` fail for a reason that is not about nano-ros — and
the failure text says "discovery", which is exactly the misattribution the gate
was written to prevent.
