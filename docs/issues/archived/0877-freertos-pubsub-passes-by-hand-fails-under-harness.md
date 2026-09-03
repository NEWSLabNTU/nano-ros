---
id: 877
title: "FreeRTOS pubsub delivers by hand and delivers NOTHING under the test
  harness — and the talker trips a FreeRTOS queue assert"
status: resolved
type: bug
area: testing, boards
related: [issue-0891, issue-0830, issue-0387, issue-0906, issue-0899, issue-1005, issue-1013]
---

## Symptom

`test_rtos_pubsub_e2e::platform_1_Platform__Freertos`, all three languages,
fails at ~65 s with `0 messages received`. Solo, on an idle host (load 0.23),
and identically on `origin/main` — so it is neither load nor a local change.

The two sides look healthy in isolation:

    Talker output:                     Listener output:
    Network ready                      nros C Listener
    Publishing: 'Hello World: 1'       Locator: tcp/192.0.3.1:7900
    Publishing: 'Hello World: 2'       Subscriber created for topic: /chatter
    …                                  Waiting for messages (Ctrl+C to exit)...

Talker publishes, listener waits, nothing arrives. The router logs no session.

## The same two images DELIVER when run by hand

Started manually — router on `tcp/0.0.0.0:7900`, listener, 22 s, then talker,
with the harness's own QEMU arguments (`-machine mps2-an385`,
`-nic user,model=lan9118,net=192.0.3.0/24,host=192.0.3.1`):

    I heard: [Hello World: 15]
    I heard: [Hello World: 16]
    …

So the images, the board's lwIP plan, the slirp addressing and the router are
all fine. Whatever fails is in how the HARNESS runs them, not in delivery.

That is the useful half of this report: it rules out the transport, and rules
out the whole class of "the guest cannot reach the router" explanations.

## Two theories killed on the way, recorded so nobody re-runs them

* **`br-qemu` missing.** The bridge really is absent on this host, and it is
  irrelevant: `192.0.3.x` here is slirp with a custom net, not a bridge. The
  FreeRTOS *action* cells pass on the same host, which contradicted the theory
  before it cost anything.
* **`6ae0249aa` (recent talker/listener ENTITY_BOUNDS change).** It touched
  only the Rust copies; all three languages fail.

Ports are also not it — the manifest bakes talker and listener both on 7900
(service 7910, action 7920), verified from the built `build.ninja`.

## Second, separate bug found while reproducing

The manually-run talker dies after ~19 publishes:

    FreeRTOS ASSERT FAILED: third-party/freertos/kernel/queue.c:1673

Delivery had already worked by then, so it is not the cause of this issue — but
it is a real fault in the FreeRTOS C talker image and does not appear to be
recorded anywhere. It needs its own diagnosis; noted here so the observation is
not lost with this session.

## Where to look next

The difference is the harness, so compare what it does that a hand-run does
not: `ZenohRouter::start_slirp` calls `kill_listeners_on_port` before binding;
the router is started per `(variant, lang)`; several cells' routers may be alive
at once. None of that is yet ruled in or out.

## Acceptance

* The cell passes under the harness, or the harness difference that breaks it is
  named and fixed.
* The `queue.c:1673` assert is filed separately with its own reproduction.

## phase-414 W1 (2026-09-03): this report probably predates its own fix

**Not verified by a run — the rebuild is the next step and it has not been
done.** What follows is strong enough to change where anyone looks, and is
labelled so nobody mistakes it for a closure.

This issue was last written 2026-08-29. **Issue 0906 was fixed 2026-08-30**:
every zenoh-pico session dropped and rebuilt every ~20 s because
`Z_TRANSPORT_LEASE` was 10 s while `rmw_zenohd` keep-alives on a 30 s cadence.
Its reproduction is THIS FreeRTOS talker/listener pair, and its measured
before/after is:

    before:  77 published, 19 heard, 58 publish errors, reconnect every ~20 s
    after:   77 published, 77 heard, 0 errors -- three runs of three

**MEASURED, and this is why the cell would still fail today:** the source says
`Z_TRANSPORT_LEASE_MS = 60_000` (`nros-zpico-build/src/lib.rs:289`), and all 24
built FreeRTOS fixture configs in this tree bake `10000`. Binaries dated
2026-08-21; the fix landed 08-30. Museum binaries carrying exactly the defect
0906 measured.

**The staleness probe cannot see it** — filed as issue 1005. The constant lives
in `nros-zpico-build`, a build-script DEPENDENCY crate that never appears in
`zpico-sys`'s recorded `cargo:rerun-if-changed` set (41 entries, none of them
there), and that recorded set is what the probe reads. So it reports FRESH.
That is issue-0196's rule with a new input class, and it is the silent direction.

### A second, separate defect, real regardless of the above

The harness KILLS the talker 15 s in. `wait_for_output` (`qemu.rs:448`) is a
run-to-completion wait — its own doc-comment says "wait for QEMU to produce
output AND EXIT" — and `rtos_e2e.rs:729` aims it at a free-running 1 Hz
publisher with a 15 s window (`:725`). Then the test waits 30 more seconds for
the listener to hear from a corpse.

    listener t=0 -> stabilization_delay 20 s -> talker t=20
    -> SIGKILL t~35 -> verdict t~65

which is the reported ~65 s exactly. The service and action shapes use
`collect_until` and let the long-lived server run the whole window, which is why
those cells pass on the same host. Worth fixing whether or not it is the cause
here.

### Killed, so nobody re-runs them

* **Router killed by `kill_listeners_on_port`** — no. It only fires when
  something is already listening on that exact port, and `fuser -k <port>/tcp`
  matches the LOCAL port, so a guest's outbound connection to 7900 is not a
  match. Routers are per-(variant, lang) with injectivity proven by a test in
  `alloc.rs`.
* **Port collision** — no, and this issue's own note was right. FreertosMps2
  base 7800; C pubsub 7900, service 7910, action 7920, matching
  `fixtures.toml:2680`.
* **QEMU argv** — identical to the hand run, `-icount` included.
* **Pipe back-pressure** — both waiters drain stdout and stderr non-blocking;
  ~1 KB against a 64 KB pipe.

A QEMU BINARY difference is real but measured not to be the cause: the harness
prefers `build/qemu/bin/qemu-system-arm` and a hand run gets the system one
(issue 0930), but 0906 re-ran on the patched emulator and got 77/77.

### Provenance correction

"The router logs no session" cannot have come from the harness run:
`zenohd_router.rs:263` sends the router's stdout to `/dev/null` and its stderr to
an unread pipe unless `ZENOHD_LOG` / `NROS_TEST_LOGS` is set. That observation is
from the HAND-run router and should not be used as a harness-side fact.

### The second bug this issue records is already filed and fixed

`FreeRTOS ASSERT FAILED: queue.c:1673` is **issue 0899, resolved** — and it is
NOT independent: it is the same 0906 session churn one layer down.
`_zp_unicast_lease_task` freed the transport under the publishing task, a
use-after-free with two victims. Fixed on the fork's `nano-ros` line; the current
pin is well past it.

### Next

1. `just setup-cli` if stale, then rebuild the FreeRTOS pubsub fixtures, and
   CHECK THE BAKE reads 60000 — do not trust a FRESH verdict from the probe.
2. Re-run the cell with `NROS_TEST_LOGS=1 ZENOHD_LOG=debug` so the timeline is
   legible rather than inferred.

If green: close this as fixed by 0906 and file the `wait_for_output` talker-kill
separately. If still red: raise `talker_window` to 60 s, which separates
"delivery is broken" from "the publisher was killed first" in one run.

## VERIFIED 2026-09-03: the cell is GREEN — and my attribution above is WRONG

Rebuilt via the sanctioned `just build freertos` (exit 0), bake verified BEFORE
running anything, then `test_rtos_pubsub_e2e` FreeRTOS solo,
`--test-threads 1 --retries 0`, 3 rounds x 3 languages.

**9 of 9 PASS.** C, C++ and Rust, 12 published / 12 heard every run, ~35.4 s
each, zero message loss, no STALE verdict, no panic. **0877's symptom does not
reproduce.**

The bake, measured before and after:

| | before | after |
| --- | --- | --- |
| C++ fixtures | `10000` x12, mtime **2026-08-20 22:31** | `60000` x12, 19:08 |
| Rust fixtures | `10000`, mtime **2026-08-20 21:53** | `60000`, 19:02 |

So issue 1005 is confirmed independently: every live FreeRTOS zenoh fixture was
baked ten days before the 0906 fix and the probe never said so. (The `10000`
copies still on disk are dead fingerprint dirs from Aug 20; nothing links them.)

### RETRACTION: this is NOT attributable to 0906

The section above says the report "probably predates its own fix" and names
0906. **The green is real; that attribution is refuted**, and by measurement
rather than by doubt.

Counterfactual run: `Z_TRANSPORT_LEASE_MS` put back to `10_000`, fixtures
rebuilt, bake verified to read `10000`, C and C++ run 3x each.

    6 of 6 PASS, 12 published / 12 heard — identical to the fixed build.

The lease constant is **invisible to this cell**, and the arithmetic says why:

    listener t=0 -> stabilization_delay 20 s -> talker t=20
    -> wait_for_output(15 s) SIGKILLs it at t~35

The talker emits exactly **12** publishes at 1 Hz before it dies. The first
lease lapse is at ~20 s of session life. **The window closes before the lapse
can happen.**

So the rebuild fixed it and 0906 did not. Something else in the twenty days
between Aug 20 and Sep 3 is the real cause. Candidates, INFERRED and untested —
`7cb213c43 feat(lan9118): drive RX from the interrupt, not a 5 ms poll (#0917)`
is the best fit for "delivers by hand, nothing under the harness", with
`5e147bee3` (#0899/#0906 lease task freeing the transport under the publisher),
`34d0a22de` (#0924) and the lwIP FULLDUPLEX / per-task netconn changes behind
it. Bisecting is separate work and is NOT done.

### A corollary that outlives this issue

**This cell can never regression-test 0906.** It kills the publisher before the
lease can lapse, so the constant it was supposed to protect is unobservable
here. Whatever gate guards that fix, it is not this one — and nothing currently
does.

### The talker kill, now quantified

Real and independent of 0877. The 15 s `wait_for_output` window
(`rtos_e2e.rs:725`, `qemu.rs:448` `kill_process_group`) truncates the talker at
exactly 12 messages, every run, all three languages: the cell exercises twelve
seconds of a free-running publisher. It was not raised here — that escalation
was conditioned on the cell failing, and it passed.

**Correction to the timeline in the section above:** the predicted `verdict
t~65` no longer holds. Every run finished in 35.4 s. The ~65 s in the original
report is not what this cell does today.

### Disposition

The symptom is gone and the cell is green, but the CAUSE is unidentified, so
this is not being closed on "it passes now" — that is precisely the reasoning
that let the stale-binary story stand. It stays open pending either a bisect
across the Aug-20..Sep-3 range or a decision that a green cell with an
unattributed fix is acceptable to close.

## CLOSED 2026-09-04 — accepted green, with the cause UNATTRIBUTED

Owner decision, and the reason it is a decision rather than a conclusion: the
symptom is gone and reproducibly so, but **nothing here identifies what fixed
it**.

What is established:

* The cell passes 9 of 9, three languages, `--retries 0`, 12 published / 12
  heard, ~35 s each.
* The fixtures that produced this report were baked 2026-08-20 and the staleness
  probe called them FRESH (issue 1005).
* It is **not** issue 0906. The counterfactual — rebuild with the old 10 s lease
  — also passes 6 of 6.

What is NOT established, and is being accepted as unknown rather than quietly
dropped: **which change between 2026-08-20 and 2026-09-03 actually fixed it.**
The best untested candidate is `7cb213c43` (lan9118 RX driven from the
interrupt rather than a 5 ms poll), which fits "delivers by hand, nothing under
the harness". Nobody bisected the range.

### What that costs, stated plainly

If the real fix is later reverted or refactored, this symptom returns and this
issue will not be the thing that catches it — because the cell that would notice
is bounded (issue 1013) and the probe that would notice is blind (issue 1005).
Closing this is a judgement that the residual risk is acceptable, not a claim
that the risk is zero.

### The two findings that outlive it, filed so they do not close with it

* **Issue 1013** — the cell SIGKILLs its talker after ~12 publishes, so it can
  never see anything with a period beyond ~12 s of session life. That is what
  makes 0906 invisible to it.
* **Issue 1005** — the staleness probe cannot see a constant that lives in a
  build-script dependency crate. Together these leave `Z_TRANSPORT_LEASE`
  unprotected in both directions.

Reopen if the symptom returns; the bisect over Aug 20 -> Sep 3 is the first
thing to do if it does.
