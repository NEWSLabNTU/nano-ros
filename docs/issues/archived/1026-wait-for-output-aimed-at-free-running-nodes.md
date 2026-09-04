---
id: 1026
title: "Six run-to-completion waits are aimed at free-running nodes, turning a
  timeout into the node's lifetime — and one test cannot fail on any build"
status: resolved
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

<!-- BEGIN: remaining-sites wave (2026-09-04) -->
## Fixed — the other seven sites + the two helper layers, 2026-09-04

Scope of this block: everything in the table above EXCEPT `services.rs` (which
the wave above owns). Files touched:
`src/{ros2.rs,ros_env.rs,zephyr.rs}`,
`tests/{interop_e2e.rs,native_api.rs,ros_editions_e2e.rs,native_async_roundtrip_e2e.rs,zephyr.rs}`.

Note on one path in the issue's sweep: the three `zephyr.rs:886/977/1681` sites
are in **`tests/zephyr.rs`**, not `src/zephyr.rs` (which is 1428 lines and has
no such call). `src/zephyr.rs` gained a doc warning instead — see below.

### The helper layer first, because six of the seven sites needed it

`RosPeer` (`src/ros_env.rs`) had exactly one wait, `wait_for_output`, and it is
`ros2::wait_child_output` — a terminal drain that kills the process group at
the deadline and reads stdout only. There was nothing else to call, so every
`ros_editions_e2e` cell used it, and the issue's "no doc warning" note
understates it: the warning was missing because the alternative was missing.

* `ros2::wait_child_output` and the new `ros2::collect_child_until` are now one
  loop (`collect_child_inner`) parameterised by an `Option<(pattern, count)>`
  stop condition. `None` = the historical terminal drain, byte-for-byte;
  `Some` = return at the n-th occurrence, **kill nothing**, and hand the stream
  back so a later wait resumes.
* `RosPeer::collect_until{,_count}` expose it, mirroring
  `ManagedProcess::collect_until` and `QemuProcess::collect_until`.
* `RosPeer::wait_for_output` now carries the warning (what it kills, what a
  `spin = "forever"` node makes of the timeout, and that it is stdout-only —
  which is safe here only because `nano_node_cmd*` bakes `2>&1`).
* `ZephyrProcess::wait_for_output` (`src/zephyr.rs`) got the same warning. It
  is the worst of the three: *every* return path passes through
  `kill_process_group`, and its only early-outs are four hard-coded terminal
  markers no cell asserts. `wait_for_pattern` was already the condition-shaped
  sibling; nothing pointed at it.
* `ros2.rs:673 collect_ros2_output` — DELETED (dead, zero callers).

### `Ros2Process::topic_echo` — the baked `timeout --foreground 10`

Two facts, and the second is why the horizon could not simply be raised:

1. The baked timeout is a HORIZON, not an implementation detail. A caller
   waiting longer than it gets a truncated transcript and a failure that reads
   like "no delivery". It is now a parameter — `topic_echo_for(…, window)`,
   with `topic_echo` delegating at the named `DEFAULT_ECHO_WINDOW` (10 s) — so
   the peer's lifetime and the caller's wait are one decision at one site.
2. **The baked timeout was doubling as the flush mechanism.** `ros2 topic echo`
   is a Python entry point, so with stdout on a pipe its `print`s sit in a
   block buffer until the process exits. That is why every caller had to drain
   to completion: a count wait would have seen nothing and timed out. The
   command now sets `PYTHONUNBUFFERED=1`, which is what makes a condition wait
   possible at all here. MEASURED: case 1 below returns in ~1 s against a 25 s
   window, so the echo is streaming, not flushing at exit.

`Ros2Process::topic_echo` has exactly one caller (`interop_e2e` case 1), which
is why it could be changed here. The four `Ros2DdsProcess::topic_echo*`
siblings behind the bridge/xrce tests were deliberately LEFT as they are: their
callers are files this wave does not own, they still drain to completion (so
the buffered-until-exit behaviour is load-bearing for them), and giving them
`PYTHONUNBUFFERED=1` would change delivery timing under tests nobody here can
run. They are the same horizon and still unstated — the remaining piece of this
row.

### Site-by-site

| site | now | stated bound |
| --- | --- | --- |
| `interop_e2e.rs` case 1 (nano→ros2 echo) | `wait_for_output_count("data:", 1, 20s)` over a 25 s echo window | FIRST delivery only |
| `interop_e2e.rs` case 5 (ros2 server ↔ nano client) | `collect_until(SERVICE_RESULT_PREFIX, 15s)` | the demo client is SINGLE-SHOT (`State::done` latches, then it idles), so one result is all it will ever print — no count >1 exists to assert |
| `native_api.rs` action body | client: `collect_until(ACTION_RESULT_PREFIX, 20s)`; server: `collect_until(ACTION_EXECUTING_MARKER, 5s)` | one goal; nothing after it |
| `ros_editions_e2e.rs` ×3 | `RosPeer::collect_until` on the marker each cell already asserts | first delivery / one reply / one result |
| `tests/zephyr.rs` 886, 1681 | `ManagedProcess::collect_until` on the listener sample line | first cross-process delivery |
| `tests/zephyr.rs` 977 | `ZephyrProcess::wait_for_pattern` on the sample line | first delivery |
| `native_async_roundtrip_e2e.rs` ×2 | wait for the client's TERMINAL line, and assert it | see below |

`native_async_roundtrip_e2e` was the one whose ASSERTION had to change, not
just its wait. It asserted `ACTION_GOAL_ACCEPTED_MARKER`, which the client
prints in the middle of its run: acceptance, then a feedback stream, then an
awaited `get_result`. A stall in either of the last two printed the accepted
line and reported PASS. It now waits for and asserts `ACTION_RESULT_PREFIX`
too, with acceptance kept as the earlier of two assertions so a rejection and a
post-acceptance stall stay distinguishable.

### Class sweep in the same files

`native_api.rs` had eight more copies of
`wait_for_output_pattern(M, t).or_else(|_| wait_for_all_output(2s)).unwrap_or_default()`.
That idiom destroys its own evidence twice — the strict wait has already
collected the transcript into an `Err` the `or_else` drops, and the fallback
then KILLS the client to gather the two seconds that remain — so a failing cell
reports the tail of a run it truncated. All eight are now `collect_until`.
Sweep command:

```
rg -n 'or_else\(\|_\| .*wait_for_all_output' packages/testing
```

Two survivors, both outside this wave's files: `tests/services.rs` (the wave
above) and `tests/action_multigoal.rs:76`. **`action_multigoal.rs:76` is an
eighth site of this class that the issue's original table does not list.**

One `wait_for_all_output` was deliberately KEPT, with the reason written at the
call site: `native_api.rs`'s `native_rust_service_interop` drains the server
purely to `eprintln!` it — nothing asserts on that string, and the kill is the
teardown the test wanted anyway. Per acceptance, a stated bound is the answer
there, not a rewrite.

### Evidence

MEASURED on this host (ROS 2 humble with `rmw_zenoh_cpp`; `cargo test`, not
nextest, so `skip!` panics show as failures):

| lane | result |
| --- | --- |
| `--test interop_e2e` (whole file) | **10 passed, 0 failed** (36.0 s) |
| `--test interop_e2e` cases 1+5, 3 repeats | 2 passed ×3 (3.45 / 3.50 / 3.50 s) |
| `--test native_async_roundtrip_e2e` ×3 | 2 passed ×3 (1.53 s) |
| `--test zephyr` (the three e2e cells) ×3 | 3 passed ×3 (5.0 s solo / 14.9 s together) |
| `--test native_api` (whole file) | **32 passed, 4 failed** — see the pre-existing red below |

Wall-clock, since a blind window is also a bill: the two zephyr pubsub cells
were 40 s each of pure deadline and now finish the pair in 10 s; the workspace
Entry cell was 40 s and is 5 s; interop cases 1+5 were ≥23 s of deadline and
are 3.5 s together.

**Mutation evidence** (the acceptance asks for it on `services.rs`; doing it
here too, since two of these assertions changed meaning):

* async action, wait+assert pointed at a marker the client never prints:
  `FAILED` in 1.32 s with `accepted the goal but never resolved its awaited
  RESULT` — and note the GOAL-ACCEPTANCE assertion still passed in that run,
  which is exactly the blind spot the old cell had.
* `interop_e2e` case 1 with the talker killed before it can publish: `FAILED`,
  and the failure reads
  `[wait data:] … printed 'data:' fewer than 1 time(s) within 20s`.
  The wait diagnostic goes on a SEPARATE channel from the asserted string
  (issue 0670): the error text names the pattern `data:`, so folding it into
  the output would have made `count_pattern(&out, "data:")` match the complaint
  about the missing samples and pass. This was the first thing tried and it did
  silently pass; the two-channel shape is what caught it.

**NOT RUN, and why:**

* `ros_editions_e2e` — every cell `skip!`s here: `example <x> not built for
  jazzy — run 'just ros_editions build-e2e-fixtures jazzy'` (the per-edition
  docker fixture set is not on this host). Compile- and clippy-clean only.
* `native_api`'s 4 reds are `test_threadx_linux_cyclonedds_{talker,cpp_talker,
  service,action}`, all **pre-existing and unrelated**: they fail identically
  with and without `NROS_SKIP_FIXTURE_CHECK=1`, their bodies (lines 1103–1291)
  contain none of this wave's hunks, and the failure is a cyclone delivery
  failure on `lo` (`selected interface "lo" is not multicast-capable:
  disabling multicast`, listener saw 0 of 2).
* The C/C++ native and zephyr fixtures were STALE by mtime on this host (a
  rebase refreshed `nros-zpico-build/src/lib.rs` at 02:31 while the binaries
  were built at 01:46; the file's content last changed 2026-08-31, i.e. before
  the build). The `native_api` and `interop_e2e` runs used
  `NROS_SKIP_FIXTURE_CHECK=1` for that reason. The three zephyr images were
  built properly (`NROS_ZEPHYR_FIXTURE_FILTER='build-rust-(talker|listener)-zenoh'`
  and `'build-ws-rs-entry-zenoh'`), so those runs are on fresh binaries.

### On the issue's point 3, a gate

Nothing added. The other wave's note is the sharper lead (a disjunct the test's
own preceding call makes true). For THIS half of the class the greppable shape
is different and probably worth more: a *terminal drain used as an observation*
— `wait_for_output` / `wait_for_all_output` / `wait_child_output` whose result
is bound and then asserted on, in a body that never kills the process itself.
The three types now each have a condition-shaped sibling
(`collect_until`/`collect_until_count`/`wait_for_pattern`), so such a gate would
have a remedy to name, which is the precondition for adding one.
<!-- END: remaining-sites wave (2026-09-04) -->
