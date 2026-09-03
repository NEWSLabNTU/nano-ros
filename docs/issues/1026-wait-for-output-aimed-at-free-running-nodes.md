---
id: 1026
title: "Six run-to-completion waits are aimed at free-running nodes, turning a
  timeout into the node's lifetime — and one test cannot fail on any build"
status: open
type: bug
area: testing
severity: high
found: 2026-09-04
related: [issue-1013, issue-0906, phase-414]
---

## The class

`wait_for_output` and its siblings are RUN-TO-COMPLETION waits — "wait for the
process to produce output **and exit**". Aimed at a node that never exits, the
timeout stops being a deadline and becomes **the node's lifetime**: the helper
kills it when the window closes.

Issue 1013 was one instance, now fixed — `test_rtos_pubsub_e2e` SIGKILLed its
talker after exactly 12 publishes, which is why a build carrying the pre-0906
`Z_TRANSPORT_LEASE = 10000` passed it. Six more share the shape. Found while
fixing 1013; **none of them is fixed**.

Note `RosPeer::wait_for_output` (`src/ros_env.rs:174`) carries no doc warning at
all, unlike `QemuProcess`'s, so the hazard is invisible at that call site.

## The one that cannot fail — `tests/services.rs:212`

Worst first, and it is worse than a bounded window:

```rust
let output = client
    .wait_for_output_pattern("Timed out waiting", Duration::from_secs(12))
    .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
    .unwrap_or_default();

assert!(
    output.contains("Timed out waiting for /add_two_ints service")
        || output.contains("Service call failed")
        || !client.is_running(),
    ...
);
```

Three defects compounding:

1. The primary wait greps a string **nothing prints**, so it always times out.
2. The `or_else` fallback therefore always runs — and `wait_for_all_output`
   calls `kill_process_group` when ITS window closes (verified in
   `process.rs`).
3. The assertion's third disjunct is `!client.is_running()`, and
   `is_running()` is `matches!(try_wait(), Ok(None))`. The fallback just killed
   the process, so that disjunct is **true by construction**.

**This test passes on every build, including one where the client never times
out at all.** It is not caught by `check-no-vacuous-tests`, because that gate
keys on "a body whose only effects are PRINTS" and this body has a real
`assert!`. The assertion is simply unfalsifiable.

## The rest

| site | shape |
| --- | --- |
| `services.rs:101`, `interop_e2e.rs:483`, `native_api.rs:523` | the window IS the SUT's whole life, and it carries the assertion |
| `ros_editions_e2e.rs:190/209/237`, `zephyr.rs:886/977/1681` | blind-collect against `spin=forever` nodes asserting only the FIRST event; two even document "always runs the full duration" |
| `native_async_roundtrip_e2e.rs:99` | asserts a mid-run marker, so a hang AFTER goal acceptance reports PASS |
| `Ros2Process::topic_echo` | a baked `timeout --foreground 10` — the same horizon one layer down, behind four bridge/interop sites |

`ros2.rs:673 collect_ros2_output` is dead and can go.

## Why it matters beyond tidiness

A bounded window silently bounds what a cell can OBSERVE, and the cell reports
PASS regardless. Issue 1013 measured the cost concretely: the pubsub cell could
not see a lease defect that broke delivery in production, because it killed the
publisher before the lease could lapse. Each site above has its own version of
that blind spot, and none of them states it.

## Direction

The shared primitive now exists: `QemuProcess::collect_until_count` (added by
1013), built on `collect_until_pred`. Wait on a COUNT or a predicate, let the
node run, kill nothing until the condition is met.

1. **`services.rs:212` first, and separately** — it is not a bounded-window
   problem, it is an unfalsifiable assertion, and it should be fixed even if
   nothing else here is. Decide what that test is actually for and assert that.
2. **Migrate the rest to the count/predicate shape**, each with a stated bound
   for what it can still not see.
3. **Consider a gate.** `check-no-vacuous-tests` cannot catch an assertion that
   is merely unfalsifiable; whether that is checkable at all is an open
   question worth a few minutes before anyone tries.

## Acceptance

For each site: either it waits on a condition rather than a lifetime, or it
states in a comment what it cannot observe and why that is acceptable.
`services.rs:212` must be able to FAIL — demonstrated by a mutation, not by
inspection.
