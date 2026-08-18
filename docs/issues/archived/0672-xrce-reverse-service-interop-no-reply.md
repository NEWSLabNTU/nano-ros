---
id: 672
title: "`test_ros2_service_xrce_client` could not wait for its ROS 2 server: a buffered `python3` plus a stdout-only reader made the readiness wait a 5 s no-op"
status: resolved
type: bug
area: testing, rmw-xrce
related: [issue-0445, issue-0480, issue-0670, phase-233, phase-342]
---

## What was actually wrong

The test starts an rclpy `add_two_ints` server, waits for it, then runs the
nano-ros XRCE client against it. The wait was:

```rust
let _ = ros2_server.wait_for_output(Duration::from_secs(5)); // let it reach "Service server ready"
std::thread::sleep(Duration::from_secs(1));
```

Three compounding defects, none of which could be seen from the failure:

1. **The helper ran a BUFFERED `python3`.** `Service server ready` sat in
   Python's stdio buffer, so it was not on the pipe when the waiter looked.
2. **`Ros2DdsProcess::collect_until` reads STDOUT ONLY**, and ROS 2 logging goes
   to **stderr**. Even unbuffered, the marker was unobservable through this API.
3. **The result was discarded** (`let _ =`), so the wait could not fail, and the
   failure message could not say whether the server had ever come up.

So the "wait for the server" was a 5 s sleep that never observed anything, and
the client was started against a server that may or may not have been listening.
The comment claimed readiness; the code could not deliver it. Same shape as
issue 0480 (literal readiness waits) and issue 0670 (the verdict replacing the
evidence).

## Fix

* `python3 -u` and `2>&1` at all FOUR rclpy helper spawns in `ros2.rs` — not
  just the one this test used, since every one of them is waited on by somebody.
* The test waits for `output::ROS2_SERVICE_SERVER_READY` (a constant, per the
  no-literal-greps rule) with a real budget, and keeps the output for the panic
  message.

Measured, same host, same binaries:

| | wall clock | result |
| --- | --- | --- |
| before | 8.3 s (5 s of it a no-op wait) | passed *when it passed* |
| broken intermediate (unbuffered, still stdout-only) | 23.2 s | passed, wait burned its full 20 s |
| after | **3.4 s** | 3/3 pass |

Faster because the client now starts when the server is actually ready instead
of after a fixed budget.

## On the "regression" this was first filed as

This issue originally claimed a source regression inside the ten commits of the
`174542aba..0a9e77298` rebase, on the evidence that the test passed against
10:19 fixtures and failed against 13:57 ones. **That attribution was wrong**, and
the correction is worth keeping:

* checking out the pre-rebase tree (`packages/{core,api,rmw}` at `174542aba`)
  and rebuilding the fixture still FAILED — so the window was not the cause;
* reverting #0670's epoch guard and rebuilding still FAILED — so that was not
  either (this had already been ruled out by measurement before the fix landed);
* every component — agent, rclpy server, XRCE client — worked when driven by
  hand at the test's own domain, ordering and environment.

What remained was a race the test could not win reliably because it could not
observe readiness. It is timing-sensitive, which is exactly why it looked like a
clean before/after when fixtures were rebuilt: the rebuild changed *when* things
started, not *what* they did.

## Related hazard, NOT fixed here

`unique_ros_domain_id()` is not unique for a filtered/solo run: with
`NEXTEST_TEST_GLOBAL_SLOT=0` and `seq=0` it returns **domain 1** every time. Any
orphaned DDS participant left on domain 1 by an earlier run therefore lands in
the next one. This host was carrying orphaned `zenohd` processes days old at the
time of the investigation, which is the issue-0659 class one transport over.
Worth its own issue if a domain-1 collision is ever observed directly — it was
not observed here, only noted as reachable.
