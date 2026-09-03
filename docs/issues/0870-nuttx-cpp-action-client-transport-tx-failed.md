---
id: 870
title: "NuttX C++ action client fails `create_action_client` — the session
  reports `Transport(ConnectionFailed)` against a router the server reached"
status: open
type: bug
area: rmw, examples
related: [issue-0867, issue-0891, issue-0460, issue-1007]
---

## Symptom

`test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`, run alone
on an idle host (load 0.53), fails its first two attempts and passes the third:

    Waiting for action goals (Ctrl+C to exit)...          # server is up
    [nros] examples/qemu-arm-nuttx/cpp/action-client/src/main.cpp:40
           node.create_action_client(client, "/fibonacci") -> -100

`-100` is `NROS_CPP_RET_TRANSPORT_ERROR` / `_Z_ERR_TRANSPORT_TX_FAILED`
(`nros_cpp_ffi.h:585`, `result.hpp:67`): the transport could not TRANSMIT. So
this is not the client failing to find the server — it is the client's own
declaration failing to leave the box.

## Not issue 0867, and not fixed by it

0867 is the C client's `Failed to send goal: -2` (`NROS_RET_TIMEOUT`), caused by
the client being started alongside the server and asking before the server's
queryable existed; the fix orders the request/response start on the server's
readiness banner. That fix is verified and it does help this cell — nuttx C++
action passed at 23.5 s in one 9-cell run — but the `-100` predates it and
survives it, and it is a different failure at a different point: 0867's client
gets as far as `Sending goal`, this one never finishes construction.

Both were present before either fix, which is why they were easy to conflate:
the same cell produced `-2` and `-100` on different runs.

## What is known

* Reproduces solo on an idle host, so it is not the host-load class of 0891.
* Roughly 2 failures in 3 attempts, and nextest's retries mask it — the cell is
  reported FLAKY rather than failing, so it has been passing CI on its third try.
* The server side is healthy and prints its banner every time.
* C on the same board and the same transport does not hit it.

## Where to look

`_Z_ERR_TRANSPORT_TX_FAILED` on a DECLARATION suggests the zenoh-pico session's
TX path is not ready, or is out of a resource, at the moment the C++ binding
declares the action client's entities. An action client is several entities at
once (goal / cancel / result queries plus feedback and status subscriptions),
declared back-to-back — a burst the C client does not produce identically.

Two candidates, neither yet tested:

* TX buffer / batch sizing on the zenoh-pico session during a declaration burst.
* Queryable and subscriber pool capacity — `ZPICO_MAX_QUERYABLES` is 8 embedded
  and `[param_services]` + `[lifecycle]` claim slots before the app declares
  anything (issue 0460). An exhausted pool surfacing as a TX failure rather than
  as `-6` (`NROS_RET_FULL`) would also explain why the error names the transport.

## Measured: the error was hidden behind THREE layers of collapse

The guesses above (TX buffer sizing, queryable pool capacity) were both wrong,
and so was the title. They were reached by reading return codes that lie. Three
separate seams each replaced a typed error with a less specific one:

1. `nros_cpp_action_client_create` — `Err(_) => NROS_CPP_RET_TRANSPORT_ERROR`,
   discarding a typed `NodeError`. Fixed: it now calls `node_error_to_cpp_ret`,
   which already existed and already prints the variant (issue 0557 built it for
   exactly this collapse, one layer in).
2. That revealed `NodeError::ActionCreationFailed` — itself a flattening of 17
   `session.create_*` sites in `executor/action.rs`, every one
   `map_err(|_| NodeError::ActionCreationFailed)`. `NodeError` ALREADY carries
   `Transport(TransportError)`; nothing needed adding, the error was simply not
   passed on. Swept all 17.
3. Which finally names it:

       [ERROR] nros: NodeError::Transport(ConnectionFailed)

So `-100` was accidentally in the right FAMILY and useless about the cause: not
a TX failure, a **connect** failure. The session cannot establish its link when
the client declares its entities — while the action SERVER, on the same port and
the same router, connected fine and printed its banner.

## What that reframes

The server reaching the router proves the router is up and reachable on that
port, so this is not "the router was not started". Two QEMU guests each connect
outward through their own slirp stack to `10.0.2.2:<port>`; the second one
fails. Candidates, none tested:

* the client's connect deadline is too short for a loaded host (two arm-virt
  QEMUs under `-icount`), so the TCP connect times out and surfaces as
  `ConnectionFailed`;
* something about the second guest's slirp path to the same host port.

Note this is NOT the 0867 ordering bug — that fix (start the client only after
the server's banner) is in, and this cell still fails. If anything the ordering
makes the client start LATER, so a connect-deadline theory has to explain why
later is not better.

## Measured: the real error is `ZpicoError::Generic`, and it is DETERMINISTIC

A fourth collapse sat below the three already fixed: `From<ZpicoError> for
TransportError` maps BOTH `Generic` and `Session` to `ConnectionFailed`, so even
the corrected chain could not say which. Issue 0465 records the cost of exactly
this pair — an exhausted session pool "spent two months looking like
`Transport(ConnectionFailed)` — a router/network problem, and chased as one".

With a diagnostic naming the variant (restricted to those two: this conversion is
on `drive_io`'s hot path, where `Timeout` converts on every quiet tick, so
logging unconditionally would flood a WORKING image):

    [ERROR] nros: zpico Generic -> ConnectionFailed
    [ERROR] nros: NodeError::Transport(ConnectionFailed)
    create_action_client(client, "/fibonacci") -> -100

`Generic` is C-shim return code `-1` (`zpico.rs:84`).

**And it reproduces BY HAND**: router up, server given a 32 s head start, idle
host, two QEMU. So this is not load, not the harness, and not the 0867 ordering
race — it is deterministic in the C++ image. The **C** action client, through the
SAME `register_action_client_raw`, succeeds under the same conditions.

## CORRECTION: it is INTERMITTENT, not deterministic

An earlier revision of this issue said the failure "reproduces BY HAND …
deterministic in the C++ image". That was wrong, and the way it was wrong is
worth recording because it nearly produced a false fix.

The hand-run reproduced it once, and a handful of test runs failed, so it was
called deterministic. Later, after a rebuild, the same cell passed 3/3 twice in
a row — which looked like a fix, from instrumentation that only logs on the
FAILURE path and therefore cannot run at all on a passing one. Reverting the
instrumentation and rebuilding: still 3/3.

The discriminating run was a rebuild with EVERY diagnostic removed:

    run 1: FAIL    run 2: PASS    run 3: FAIL

So the cell is flaky at roughly two failures in three — exactly what this issue
originally reported — and three consecutive passes was luck, not evidence. Two
consecutive clean sets of three had a prior of about 0.1 %, which is precisely
why "it passes now" should not have been treated as a result.

**Nothing is fixed.** The build sequence that produced the passes is recorded
here only so the next person does not mistake it for one.

## Not yet known

Which of the four declarations returns `-1`. `register_action_client_raw` makes
three `create_client` calls plus one `create_subscription`, and the C client
makes the same four. Ruled out as the source: `ZenohServiceClient::new` (returns
`TopicNameInvalid` / `ServiceClientCreationFailed`, never `Generic`) and
`declare_entity_liveliness` (swallows errors with `.ok()`). Finding it needs
per-call instrumentation in the shim — the next step, and the last layer.

Worth fixing regardless of cause: `TransportError::ServiceClientCreationFailed`
exists and is precise for a failure inside `create_client`; `ConnectionFailed` is
the wrong name for it and is what sent both earlier diagnoses at the network.

## Acceptance

* The cell passes on its FIRST attempt, repeatably, on an idle host.
* MET ALREADY: the failure names its own cause rather than reporting a generic
  transport error — that half is fixed and is worth keeping independently of the
  connect bug, since it is what made the connect bug findable at all.

## phase-414 W3 (2026-09-03): not shared with 0867, and the blindness that hid it

**The shared-cause question is answered: NO.** 0867's cause was harness ordering
and its fix (`start_server_then_client`) covers all three languages, so C++ has
been starting after the server's banner all along and still fails ~2 in 3. And
the failure POINTS cannot be one defect: 0867 failed at `send_goal`, after this
client's declarations had all succeeded; this fails INSIDE them, before any
interaction with the server exists.

### Why nobody could read the real error: `printk` was a no-op on NuttX

`zpico.c`'s printk chain had arms for Zephyr, FreeRTOS, ThreadX and bare-metal,
then `#else #define printk(...)`. NuttX defines `ZENOH_NUTTX` + `ZENOH_LINUX` and
matched no arm, so **every diagnostic in the shim compiled away** — including the
two that name this fault outright:

    zpico: z_declare_subscriber (ring) failed: %d for '%s'
    zpico: z_liveliness_declare_token failed: %d for '%s'

the second of which has a comment calling a failed token "a SILENT graph outage
… say so on the console". Not a platform limitation: NuttX has full POSIX stdio
and the TU already includes `<unistd.h>`. FIXED — NuttX now routes printk to
`printf`. This is the fifth error-collapse layer in this issue's chain and the
first one below the Rust boundary.

### Which declaration fails, by elimination — INFERRED, not observed

`register_action_client_raw_sized` makes four `session.create_*` calls. The three
`create_client` calls do NO network I/O (`ZenohServiceClient::new` only builds
keyexprs; its only errors are `TopicNameInvalid` / `ServiceClientCreationFailed`),
and the liveliness declares inside them are swallowed by `.ok()`
(`shim/session.rs:564`). The feedback `create_subscription` is the ONLY site in
the whole construction that converts a `ZpicoError`, so it is the only one that
can produce the observed `Generic -> ConnectionFailed`. That narrows to
`z_declare_subscriber() < 0` (`zpico.c:2246`).

Construction performs six network ops: five liveliness declares (all invisible)
plus one `z_declare_subscriber` (the only one that speaks). **Whether the other
five also failed is currently unknowable, and that distinction is the
diagnosis:** 6-of-6 means the session's declare/TX path is dead at that moment
(which is what `ConnectionFailed` accidentally named correctly); 1-of-6 means
something subscriber-specific.

### MEASURED, and it kills this issue's two standing guesses

Both leaves' compiled shim constants are byte-identical (same fingerprint hash):
`ZPICO_MAX_QUERYABLES = 32` — **not 8** — `MAX_SUBSCRIBERS = 8`,
`MAX_LIVELINESS = 16`, `MAX_PENDING_GETS = 4`, `MAX_SESSIONS = 1`. Neither image
registers param services or lifecycle (both opt-in, neither example calls them),
so issue 0460's "6+5 slots claimed before the app" does not apply here. **The
queryable-capacity lead is dead**, and so is the TX-buffer one for the same
reason: the two languages share the shim config exactly.

C and C++ also declare the same four entities, with the same names, resolving to
the same session slot 0. Fixture rows are symmetric but for the port.

### Still open, and it is the whole remaining question

**Why C++ and not C is unexplained.** No structural asymmetry was found by
reading: same shim, same pools, same entities, same names, same slot, same
ordering relative to session open. The remaining differences are timing-shaped
(C interposes `nros_executor_init` between session open and the declarations;
C++ goes through `Executor::open_in`). None is a mechanism defensible from
reading alone. **Treat any C-vs-C++ story that has not been measured as a
guess — this issue has already burned three of them.**

### Next step, now cheap

The four per-declaration diagnostics from `f5674ed52` are still in the tree
(`action.rs:1236/1246/1256/1266`) and `nros_log` demonstrably reaches the NuttX
console. With printk unmuted, the next FAILING run names which declaration failed
AND prints zenoh-pico's raw return code — at zero extra cost. Worth pairing with
a log at the swallowed liveliness site (`shim/session.rs:564`), which is the
canary the design deliberately muted.

## 2026-09-03 experiment: COULD NOT REPRODUCE. The issue stays OPEN.

The decisive question — do all SIX network operations fail, or only the
subscriber — is **still unanswered**, because nothing failed.

**MEASURED: 28 solo C++ runs, `--retries 0`, 28 PASS, 0 FAIL.**

| batch | condition | result |
| --- | --- | --- |
| 22 runs | idle host (~5 of 48 cores) | 22/22 PASS, 44-50 s |
| 3 runs | C++ co-selected with the C cell | 3/3 PASS |
| 3 runs | 32 busy loops, load avg 35 | 3/3 PASS, 61-67 s |

Every pass is a real round trip, not an early exit — `Result received:
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`. At the reported 2-in-3 failure rate,
28 consecutive passes has probability (1/3)^28. **The reported rate does not
hold for the current build on this host.**

**This is NOT a fix and must not be read as one.** This issue's own record says
removing the diagnostics restored FAIL/PASS/FAIL, which points at a
timing-sensitive fault whose probability moves with image CONTENT — and today's
image carries more code than any previously measured one. Which change moved it
is unknown, and per this issue's own history (three guesses already burned) no
fourth is offered.

### The instrumentation is armed now, verified by linkage

`strings` on the fixtures, Aug-21 binary vs today:

| string | Aug-21 | today |
| --- | ---: | ---: |
| `zpico: z_declare_subscriber (ring) failed: %d for '%s'` | 0 | 1 |
| `zpico: z_liveliness_declare_token failed: %d for '%s'` | 0 | 1 |
| `action client: feedback subscription failed` | — | 1 |
| `action client: send_goal client failed` | — | 1 |

Both C and C++; sizes grew 806,892 -> 852,964 (C++) and 752,604 -> 793,336 (C).
Every `printk` in `zpico.c` is on a failure path, so **the NuttX printk arm is
verified by LINKAGE, not by observation** — nothing emitted because nothing
failed. The next failing run will speak; this one had nothing to say.

### The history this reframes

The binaries in this checkout when the experiment started were dated **Aug 21
09:01**. This issue's quoted output (`zpico Generic -> ConnectionFailed`) comes
from instrumentation landed **Aug 29** (`f5674ed52`). So the binaries in this
tree were never the ones that produced those measurements — that work happened
against a build that no longer exists here, and "it passes now" is not a delta
against "it failed then". The two are not the same image and cannot be diffed.

### First hard number on the C-vs-C++ asymmetry

**C: 3/3 PASS at 26.4-27.2 s. C++: 44-50 s.** The C++ image takes ~20 s longer
for the same action round trip on the same board and transport, even when it
succeeds. That is a measurement, not a mechanism — but it is the first evidence
of any kind about the asymmetry, which until now was only guesses.

### Also found, filed separately

* **Issue 1007** — a clean `just nuttx build-fixtures-arm` can leave every arm
  cell unrunnable, and the remedy it prints is the command that just
  short-circuited. Cost a forced kernel build plus a second full fixture rebuild
  before any measurement could be taken.
* Every NuttX C/C++ configure logs `no Corrosion at the pinned prefix … falling
  through to FetchContent`, i.e. it clones Corrosion from git. It succeeded here
  (network available); this is the offline-failure shape of issues 0500/0726.

### What would actually settle this

A failing run. Since it will not fail on demand here, the cheapest honest option
is to leave the instrumentation in place and catch it in a sweep — the cell is
reported FLAKY by nextest retries, so the CI history already knows when it fails
even though no one has read a failing run since the diagnostics landed.

## 2026-09-04 — policy: NO RETRIES on this cell

Owner decision, and it reframes what the cell is for. These e2e cells exist to
show an RTOS meeting its obligations **including under load** — that is the
property under test. A cell that fails once and passes on the third attempt has
not demonstrated it; it has demonstrated that the guarantee is PROBABILISTIC,
which is the failure. Retrying converts that finding into a green.

`retries = 2` -> `retries = 0` for `binary(rtos_e2e) and test(Platform__Nuttx)`.

That retry is what kept this issue unreadable for weeks: the cell failed roughly
two runs in three, was reported FLAKY, passed CI on its third try, and nobody
looked. Both halves of the blindness are now gone:

* the shim diagnostics are linked in (verified by `strings`, absent from the
  Aug-21 binaries) — the NuttX `printk` arm exists now;
* `rtos_e2e.rs` already prints server boot, server post-boot and client output
  on EVERY run, so a failure carries them;
* the action assertion now names the question its own output answers.

**A red here is information, not noise.** And if concurrency is what makes a
cell fail, the fix is the `test-group` and port routing — configuring the
concurrency correctly — not re-rolling the dice.

### What a failing run will now settle

The one remaining unknown: **six of six, or one of six?** Construction performs
five liveliness declares plus the feedback subscriber. If all six failed, the
session's declare/TX path is dead at that moment and `ConnectionFailed` was
accidentally the right family. If only the subscriber failed, it is
subscriber-specific and zenoh-pico's raw code names it. Nobody has ever seen
that, because nobody has read a failing run since the diagnostics landed.

### Deliberately NOT done

The other three RTOS e2e overrides still carry `retries = 2`:
`Platform__Freertos`, `Platform__Threadx*`, `ThreadxLinux`. They mask the same
class and the same argument applies to them, but issue 0968 records ~12
unreproduced tier-2 e2e failures — flipping all four at once produces a wall of
red that obscures rather than reveals. Extending this is a one-line change per
override and a separate decision, not a rediscovery.
