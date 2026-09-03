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

<!-- BEGIN: services.rs wave (2026-09-04) -->
## Fixed — `tests/services.rs` (both sites), 2026-09-04

Scope of this block: `packages/testing/nros-tests/tests/services.rs` ONLY. The
other seven sites in the table above are untouched.

### What the test is actually for, and what the client really prints

MEASURED, by running the fixture (`native/rust/service-client`, zenoh, against a
`zenohd_unique` router with no server):

```
PROBE n=1 at 1.021163202s why=None seen_count=1
PROBE n=2 at 2.020642364s why=None seen_count=1
PROBE n=3 at 3.014332016s why=None seen_count=1
PROBE n=4 at 4.018483002s why=None seen_count=1
PROBE n=5 at 5.013361215s why=None seen_count=1
PROBE n=6 at 6.01650743s  why=None seen_count=1
```

The client prints `[INFO] Service call failed, retrying: Runtime` — the `Err`
arm of `call_for_name` in `examples/native/rust/service-client/src/lib.rs` —
once per 1 s timer tick, from ~1 s after spawn, **forever**. It never exits
(`spin = "forever"`, issue 0274) and it never prints anything resembling
`Timed out waiting`. The only producer of that wording in the tree is the
unrelated `service-client-callback` example, which spells it
`Timed out waiting for reply to {} + {}`. So the greped pattern was dead in
both tests.

A fourth defect, on top of the three the issue listed: `or_else` **discards the
first wait's output**. The 12 s window collected ~12 failure lines, the `Err`
carried them into a formatted message, and `or_else` threw the whole thing
away — so the assertion only ever saw the ~2 s the fallback re-collected. The
original run printed two lines where twelve had happened.

The property both tests are for, restated:

* `test_service_client_starts_without_server` — the error path is REACHABLE:
  the first call fails and says so, promptly, without the client dying.
* `test_service_client_timeout` — it STAYS reachable: every attempt fails and
  is reported at the timer cadence, no reply is ever manufactured, and the
  client survives its own timeouts.

Both now wait with `ManagedProcess::collect_until_count` (the `ManagedProcess`
sibling of the `QemuProcess` primitive the issue names; it returns as soon as
the count is reached and, unlike `wait_for_output`/`wait_for_all_output`, kills
NOTHING on timeout). The tests own the lifetime and call `client.kill()`
themselves. Assertions: `>= 3` failure markers within 20 s (1 Hz ⇒ ~3 s),
`!output.contains(SERVICE_RESULT_PREFIX)`, and `is_running()` sampled BEFORE
the kill.

Stated bound, per acceptance: neither test observes whether the client would
eventually give up (it has no such contract) nor that a call succeeds once a
server appears (that is `test_service_multiple_sequential_calls`).

### Mutation evidence

**The old test passes with a fully working server present** — the sharpest
statement of the defect. Same file, only the mutation added (spawn the server
the test is supposed to be missing):

```
running 1 test
Timeout test output:

test test_service_client_timeout ... ok
        PASS [  14.315s] nros-tests::services test_service_client_timeout
```

Note `Timeout test output:` is EMPTY: the assertion ran against `""` and passed
on `!client.is_running()` alone, which the fallback's kill had just made true.

**New test, same mutation (server present) — RED:**

```
a client with no server must report a failed call on every attempt: expected >= 3
`Service call failed, retrying:` within 20s, saw 0:
...
[INFO] Result of add_two_ints: 5
        FAIL [  20.316s] nros-tests::services test_service_client_timeout
```

**New test, SUT silenced** (stand in a client build whose `Err` arm prints
nothing and never exits — the pre-phase-338 `Err(_) => {}` shape) — RED:

```
expected >= 3 `Service call failed, retrying:` within 20s, saw 0:
...
[INFO] Waiting for service requests
        FAIL [  20.313s] nros-tests::services test_service_client_timeout
```

Same mutation against the sibling `test_service_client_starts_without_server` —
also RED (`FAIL [ 15.315s]`).

**Restored, GREEN**, whole file:

```
        PASS [   1.321s] nros-tests::services test_service_client_starts_without_server
        PASS [   3.322s] nros-tests::services test_service_client_timeout
     Summary [  10.681s] 5 tests run: 5 passed, 0 skipped
```

Wall clock also improves: the two tests were 14.3 s + 12 s of pure deadline;
they are now 1.3 s + 3.3 s of real waiting.

### Follow-ups this wave did not take

* `SERVICE_CALL_FAILED_MARKER` is a file-local `const` in `services.rs`, not a
  `nros_tests::output` constant, because this wave owned one file. It belongs
  beside `SERVICE_RESULT_PREFIX` — every Rust group copy of the client
  (`qemu-arm-nuttx`, `qemu-arm-freertos`, `threadx-linux`) prints the same
  wording, and the C/C++ copies print a different one (`Service call failed
  with error %d`), which is worth a second constant.
* On the issue's point 3 (a gate for unfalsifiable assertions): the shape that
  made this one unfalsifiable is *a disjunct the test's own preceding call
  makes true* — here `!is_running()` after a helper that kills. That is
  narrower than "unfalsifiable" in general and might be greppable: an
  `is_running()`/`try_wait()` disjunct in an `assert!` that follows a
  `wait_for_all_output`/`wait_for_output` in the same body. Not attempted here.
<!-- END: services.rs wave (2026-09-04) -->
