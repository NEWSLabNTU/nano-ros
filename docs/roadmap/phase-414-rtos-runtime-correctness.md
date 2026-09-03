# Phase 414 — RTOS runtime correctness: the e2e failures that reproduce SOLO

**Status (2026-09-03). W2 and W4 CLOSED, W3 advanced, W1 rediagnosed.** Opened
as a HOME, not as a plan: five open issues had no phase that could hold them,
and the survey that found that is the whole reason this doc exists — an issue
with no home is an issue nobody is accountable for, the same shape as a gate
that sits in a lane no CI job runs.

**The phase's central question is answered: W2 and W3 do NOT share a cause.**
The phase does not shrink; but W2 turned out to be fixed already and W3's real
blocker is one layer below where three diagnoses had been aimed.

**Two of the five were not defects in the state the issues described them in.**
W2 was fixed on 2026-08-28 and never closed. W1's evidence predates the fix for
issue 0906 by one day. That is a second instance of this phase's own premise:
an issue with no owner does not merely sit, it goes STALE, and a stale issue
costs more than an open one because it is read as current.

## Why these five are one phase, and why the two neighbours could not take them

They are RUNTIME failures on a real RTOS: the image builds, links and boots, and
then does the wrong thing. That is a different activity from either neighbour:

* **[phase-349](phase-349-rtos-integration-shells.md)** is BUILD integration —
  "make FreeRTOS an imported library like the rest". It is about how the RTOS
  enters the build, not about what the image does afterwards.
* **[phase-358](phase-358-embedded-runtime-under-load.md)** is footprint,
  overrun and overload — failures that appear when you PUSH the runtime. These
  five fail at rest.

The distinguishing property, and the reason they are worth grouping: **each
reproduces SOLO.** CLAUDE.md's standing advice for a QEMU red is to retest it
alone before believing it, because full-sweep lanes flake under load. These
already survive that test, so they are not the flake class — they are defects
with a stable reproduction and no owner.

## Work items

Each is an existing issue. The item is "close it"; the issue holds the evidence.

* **W1 — [issue 0877](../issues/0877-freertos-pubsub-passes-by-hand-fails-under-harness.md),
  FreeRTOS pubsub delivers NOTHING under the test. REDIAGNOSED, not yet
  closed.** The issue's evidence is dated 2026-08-29; issue **0906** (every
  zenoh-pico session dropping every ~20 s because `Z_TRANSPORT_LEASE` was 10 s
  against a 30 s router keep-alive) was fixed on 2026-08-30, and its own
  reproduction is this exact FreeRTOS image pair, measured 19 heard of 77
  before and 77 of 77 after.
  **MEASURED: every built FreeRTOS fixture in the tree still bakes
  `Z_TRANSPORT_LEASE 10000` while the source says 60000** — museum binaries
  carrying precisely the defect 0906 fixed. Worse, the staleness probe cannot
  see it: the constant lives in `nros-zpico-build`, a build-script DEPENDENCY
  crate that never appears in `zpico-sys`'s recorded `cargo:rerun-if-changed`
  set, so the probe reports FRESH. That probe gap is issue-0196 class and
  wants its own issue.
  A second, harness-side defect is real regardless: `wait_for_output` KILLS the
  talker 15 s in (`rtos_e2e.rs:729`, `qemu.rs:448`) — it is a run-to-completion
  wait aimed at a free-running 1 Hz publisher — and 20 s settle + 15 s life +
  30 s listener wait is the reported ~65 s exactly. The service and action
  shapes use `collect_until` and let the server live, which is why they pass on
  the same host.
  **Next: rebuild the FreeRTOS pubsub fixtures, confirm the bake reads 60000,
  re-run the cell.** Do not trust a FRESH verdict from the probe here.
  The `queue.c:1673` assert this issue also records is **issue 0899, already
  resolved** — the same 0906 session churn one layer down.
* **W2 — [issue 0867](../issues/archived/0867-nuttx-c-action-goal-send-times-out.md).
  CLOSED — it was fixed before this phase opened.** `bb0631e5f` (2026-08-28)
  landed `start_server_then_client`; the issue was simply never closed. Cause
  was harness ordering, not the image: `start_pair` launched both NuttX
  instances at once, keyed on the PLATFORM — right for pub/sub, wrong for
  request/response where the client asks ONCE. 3/3 failing at 72-92 s ->
  passing at 16.2 s.
* **W3 — [issue 0870](../issues/0870-nuttx-cpp-action-client-transport-tx-failed.md),
  NuttX C++ `create_action_client` fails. OPEN, and no longer blind.**
  **NOT shared with W2**, structurally: W2's fix covers all three languages
  (`rtos_e2e.rs:922`), so C++ has been starting after the server's banner all
  along and still fails ~2 in 3 — and the failure points differ, W2 at
  `send_goal` after the declarations succeeded, this one INSIDE them.
  Landed here: **NuttX had no `printk` arm in `zpico.c`**, so every shim
  diagnostic compiled away, including the two that name this fault. Fixed.
  Killed by measurement: the queryable-capacity and TX-buffer leads the issue
  was carrying — both leaves' shim constants are byte-identical and
  `ZPICO_MAX_QUERYABLES` is **32, not 8**.
  Why C++ and not C is still unexplained, and the issue now says so rather
  than offering a fourth guess.
  **EXPERIMENT RUN 2026-09-03: could not reproduce — 28 solo runs, retries
  disabled, 28 PASS, idle and under load.** At the reported 2-in-3 rate that is
  (1/3)^28, so the rate does not hold for the current build. NOT a fix: this
  issue records that removing the diagnostics restored FAIL/PASS/FAIL, so the
  fault is timing-sensitive and moves with image CONTENT, and today's image
  carries more code than any previously measured one.
  The instrumentation is now armed — both shim diagnostics are linked into the
  binaries (`strings`, absent from the Aug-21 ones), so the next FAILING run
  answers the 6-of-6 vs 1-of-6 question at zero cost. Verified by LINKAGE, not
  observation: every printk is on a failure path and nothing failed.
  First hard number on the asymmetry: **C 26.4-27.2 s, C++ 44-50 s** for the
  same round trip. A measurement, not a mechanism.
  Also: the Aug-21 binaries in this tree predate the Aug-29 instrumentation that
  produced this issue's quoted output, so "it passes now" is not a delta against
  "it failed then" — different images, not diffable.
* **W4 — [issue 0847](../issues/archived/0847-xrce-entity-drop-after-session-close.md).
  CLOSED.** The fix is a refcount (`live_entities` + `session_closed`), applied
  to all four entity destructors, with each checking `xrce_session_is_closed`
  before touching the session. The two shapes the issue left open were both
  rejected for stated reasons: the binding side protects Rust callers only on a
  C ABI, and the back-pointer sweep needs a fourth static pool because the
  session has slot tables for everything EXCEPT publishers — the entity the
  crash was reported on — on the backend whose current campaign is removing
  unpriced static RAM.
  Cyclone is immune because it stores validatable HANDLES, not pointers; that
  contrast is what settled the shape.
  Gated by `tests/entity_lifetime.c` under `just check rmw-xrce`, asserting the
  EXIT STATUS as the issue required, and mutation-checked.
* **W5 — [issue 0741](../issues/0741-xrce-service-reply-history-payload-too-small.md),
  `test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the
  request.** INTEROP with a real ROS 2 peer.
  **ROUTING DECIDED: it STAYS here. Not encoding, so not phase-303.** The
  defect is wire FRAMING of the request/reply mapping, outside our serializer —
  not XCDR2/extensibility, which is 303's class and is parked besides.
  The issue's own premise is refuted: 15 bytes is the CORRECT reader history
  size for `AddTwoInts_Response` (`align4(4+8)` + 3), confirmed by five
  environments that accept the reply. The sample is oversized by 16, not the
  buffer undersized — the title is inverted.
  `28 = 4 + 24`: the Agent appears to consume only 8 of the 24-byte
  SampleIdentity our client prefixes and leak the other 16 into the DDS
  payload. MEASURED up to the agent boundary; INFERRED across it, since
  `third-party/xrce/agent` is uninitialised here.
  **Landed here: this issue's own mitigation was unreachable.** `ca224e271`
  built the agent against the sourced ROS's Fast-CDR, but `just xrce setup`
  short-circuited on file existence and never called the script that decides —
  so any host that had ever published an agent kept the skewed one. This host
  still carried the pre-mitigation wrapper nine days later. Fixed.
  **RE-MEASURED 2026-09-03 on a genuinely zero-skew agent (Fast-DDS 2.6.12 /
  Fast-CDR 1.0.29, the same library FILES as the ROS peer, verified by `ldd`):
  the failure SURVIVES.** 66 runs, `--retries 0`, 64 pass / 2 fail — batch A
  alone 14 of 15, the same order as the historical rate. So 0741 cannot be
  closed as "the mitigation was never applied"; it was not applied, and
  applying it does not fix this.
  **My own inference above is REFUTED.** In the failing run the Agent never
  received the DDS request and never wrote a DDS reply — `read_fn=0`,
  `write=0`, complete trace, truncation excluded. It did not mis-slice the
  SampleIdentity; it did nothing. The question changes shape: the request never
  reached the Agent AND something else wrote a 28-byte sample on the reply
  topic. That is an endpoint-matching/discovery anomaly, not a serialization
  one.
  **Also fixed here: the only non-root instrument was compiled out.**
  `build.sh` passed the logger profile OFF at both sites, so
  `NROS_XRCE_AGENT_VERBOSE` was silently inert against the agent the mitigation
  publishes. Now selectable, recorded in the stamp so it rebuilds, derived at
  file scope so both build paths see it, and the test side says what an empty
  log means.
  Next is a DDS capture (needs root, unavailable here): the writer GUID of the
  28-byte sample decides Agent-framing versus foreign peer.

## Acceptance

* Each of the five is resolved, or reassigned to a phase that fits better with
  the reason recorded.
* For W2/W3, an explicit statement of whether the cause was shared — that
  answer is worth more than either fix.

**Progress 2026-09-03.** W2 and W4 closed; W5's routing decided (stays);
W1 and W3 rediagnosed with their standing guesses killed by measurement.

The shared-cause answer: **NO.** W2 was harness ordering and is already fixed;
W3 fails inside construction, before any interaction with the server exists, so
server ordering cannot reach it. The phase does not shrink.

**What this phase actually found, and it was not in any of the five issues.**
Three of the five were unreadable rather than unfixed:

* W2 was fixed on 2026-08-28 and nobody closed it.
* W1's evidence predates issue 0906's fix by one day, and every built FreeRTOS
  fixture still bakes the pre-fix `Z_TRANSPORT_LEASE 10000` while the source
  says 60000 — with a staleness probe that reports FRESH, because the constant
  lives in a build-script DEPENDENCY crate that never enters
  `cargo:rerun-if-changed`.
* W3 could not be read at all: NuttX had no `printk` arm, so every shim
  diagnostic compiled away.
* W5's own mitigation had been unreachable for nine days behind a
  file-existence short-circuit.

The phase opened on the premise that an issue with no owner is an issue nobody
is accountable for. The sharper version, measured here: it goes STALE, and a
stale issue costs MORE than an open one, because it is read as current and its
recorded guesses get re-run.

## What this phase deliberately does NOT do

It does not add tests, lanes or gates. Every one of these already has a failing
test; the problem is that nothing was accountable for making them pass.
