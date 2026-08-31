---
id: 868
title: "A `send_goal` TIMEOUT prints as `Goal was rejected by server`, so an
  intermittent XRCE action failure reads as a deterministic server decision"
status: resolved
type: bug
area: examples, testing
---

## The diagnostic defect (the part worth fixing)

`examples/native/cpp/action-client/src/main.cpp:81` reports every non-OK
`send_goal` as a server rejection:

```cpp
ret = client.send_goal(goal, goal_id);
if (!ret.ok()) {
    fprintf(stderr, "Goal was rejected by server (order=%d, ret=%d)\n", order, ret.raw());
```

`ErrorCode::Timeout` is `-2` (`packages/api/nros-cpp/include/nros/result.hpp:36`),
so a goal whose response never arrived prints:

```
Goal was rejected by server (order=10, ret=-2)
```

The server rejected nothing. It may never have received the goal at all. The
message names a specific server decision — and `on_goal` does have a reject path
(`order out of range`) — so it sends a reader to the wrong file. It cost exactly
that here: the observed failure was in the XRCE service/action inbox path that
had just been rewritten, and the message pointed at `on_goal` rather than at a
timeout.

Five copies carry the line, and they are a portability group, so the fix moves
all five together:

```
examples/{native,qemu-arm-freertos,qemu-arm-nuttx,qemu-riscv64-threadx,threadx-linux}/cpp/action-client/src/main.cpp
```

A rejection and a timeout should read differently — the rejection branch is
`ret.raw()` matching the reject code, everything else is "no response".

## The flake underneath it

`nros-tests::native_example_reqresp_e2e case_18_cpp_xrce_action` fails
intermittently in the FULL sweep only.

**Measured** (this host, 2026-08-28, on the tree at `d74cc3de0`):

| run | result |
| --- | --- |
| full `just ci` sweep (1541 tests) | FAIL — `ret=-2` |
| full `test-all` sweep (1573 tests) | PASS |
| solo, `-E 'test(case_18)'` | PASS x4 |
| whole `native_example_reqresp_e2e` suite, solo | PASS |

So: 1 failure in 2 full sweeps, 6 passes outside one. The other four XRCE cells
(08, 09, 16, 17) passed in every run including the failing sweep, which is what
argues against a payload or serialization defect — those share the same inbox
path.

**Reasoned, NOT measured:** that this is contention rather than a code defect.
The evidence is consistent with it (load-dependent, non-deterministic, siblings
green) but nothing here identifies *what* is contended. A goal response that
misses its window under parallel load could be the agent, the port, or the
client's own wait budget, and this issue does not distinguish them.

Not established either: whether it predates the phase-395 pull. It was not
observed in the sweeps run earlier the same day, but those were on a different
tree and the sample is far too small to date it.

## Direction

1. Fix the message first — it is cheap, it is a five-copy class, and until it is
   fixed every future occurrence of this flake will be mis-triaged the same way.
2. Then re-measure the flake with a message that distinguishes the two, and a
   sweep repeated enough times to give the rate meaning. One failure in two runs
   is a report, not a rate.

## Resolution

### The message could not have been fixed on its own

The issue proposed branching on the return code: rejection prints one thing,
everything else prints "no response". Reading the code first showed why that
would still have lied. `nros_cpp_action_client_send_goal` had **two codes for
three outcomes**:

| outcome | code before |
| --- | --- |
| server accepted | `OK` |
| server RECEIVED the goal and declined it | `ERROR` (-1) |
| the goal could not be SENT at all | `ERROR` (-1) |
| no goal response within 30 s | `TIMEOUT` (-2) |

So a `-1` was "the server said no" *or* "it never left", and no message at
this call site could tell a reader which. The C twin `nros_action_send_goal`
had the identical collapse. Same class as issue 0586, one layer over: "an
unmapped variant is not cosmetic, it is the whole diagnosis."

The C ABI had already named the right code and nobody used it —
`NROS_RET_REJECTED` (-13) is documented in `nros-c/src/error.rs` as *"Request
was rejected (e.g., **goal rejected by server**)"*. The C++ mirror had drifted
narrower ("Rejected (QoS/ABI incompatibility)"), which is probably how it
stopped being the obvious choice.

### What changed

1. **Both blocking `send_goal`s return `REJECTED` for a server decision**
   (`nros-cpp/src/action.rs`, `nros-c/src/action/client.rs`). `ERROR` now
   means only "could not send". The return contract is documented on both
   functions and regenerated into the committed cbindgen headers.
2. **The C++ `Rejected` docstrings** (`nros-cpp/src/lib.rs`, `result.hpp`)
   broadened to match the C definition they had drifted from.
3. **Ten example copies, two portability groups, moved together.** The five
   C++ clients branch three ways; the five C clients — which said only
   `"Failed to send goal: %d"`, vague rather than wrong — now name the
   rejection and the missing response too. Both groups were byte-identical
   before and after (verified by md5).
4. **Two `output.rs` constants** (`ACTION_GOAL_REJECTED_PREFIX`,
   `ACTION_GOAL_NO_RESPONSE_PREFIX`), registered in `output_marker_gate`'s
   `MARKERS`. Proven non-vacuous: restoring the literal in `native_api.rs`
   fails the gate at `native_api.rs:796`.

### The test this was hiding

`test_cpp_action_goal_rejection` drives a REAL rejection (order 100 > 64) and
asserted `contains("Goal was rejected by server")`. Because the client printed
that line for every non-OK return, **the assertion was satisfied by a timeout
just as well as by a rejection** — it proved "something failed", not "the
reject path ran", on the one test whose entire purpose is the reject path.

It now asserts the rejection marker (which only a `REJECTED` return can
produce) AND the absence of the no-response marker, so a transport failure
fails the test instead of passing it.

### Audit finding E7, closed for the right reason

`docs/development/audit-findings-2026-07-28-deep-CE.md` looked straight at this
test five weeks ago. It correctly found `assert!(!client_output.contains("[OK]"))`
asserts a marker the client never prints — then **downgraded it**:

> Downgraded from the finder's P2 because the **positive** assertion two lines
> above ("Goal was rejected by server") carries the test — this is decoration,
> not a hidden bug.

The positive assertion was the one that could not carry it. The audit's
recommended fix ("assert via an `output.rs` constant so the coupling is
checked") was right; its reason for deprioritising was backwards, and that is
why this sat. The dead `[OK]` check is now `!contains(ACTION_GOAL_ACCEPTED_PREFIX)`
— the assertion it was trying to be.

### Runtime proof

Against freshly rebuilt native zenoh fixtures (client + server relinked
2026-09-01 04:04, after the source change — checked with `stat`, and all three
new branch strings confirmed present in the binary with `strings`):

```
$ cargo nextest run -p nros-tests --test native_api \
      -E 'test(test_cpp_action_goal_rejection)' --no-capture

C++ action client output:
nros C++ Action Client (Fibonacci)
Sending goal
Goal was rejected by server (order=100)

PASS [0.427s] (1/1) nros-tests::native_api test_cpp_action_goal_rejection
```

`(order=100)` with no `ret=` suffix is the NEW rejection branch — the old line
carried `ret=%d`. So `REJECTED` round-trips from the Rust ABI through the C++
`Result` into the branch that names a server decision. `--no-capture` because a
0.4 s pass on a test that spawns a router, a server and a client is exactly
what a vacuous early-return looks like; the client output is the evidence it
actually ran.

`test_cpp_action_communication` (the ACCEPT path, same rebuilt fixtures) also
passes, so the `OK` arm is unaffected by the split.

**Not proven at runtime: the timeout branch.** Staging a server whose send-goal
queryable exists but never answers is not cheap, and I did not do it. It rests
on the code: `TIMEOUT` is a distinct return from a distinct exit of the spin
loop, and the example branches on `ErrorCode::Timeout` before the fallback.

### Not done

**The flake underneath (`case_18_cpp_xrce_action`) is untouched.** Direction
step 2 — re-measure with a message that distinguishes the two, over enough
runs for a rate to mean something — is the remaining work, and it is now
possible for the first time: a recurrence will say which of the three
outcomes it was. One failure in two sweeps was never a rate, and nothing here
changes that number.
