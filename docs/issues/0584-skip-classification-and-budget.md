---
id: 584
title: "Skips are tolerated rather than asserted: a lane cannot tell an expected skip from a test that silently did not run"
status: open
type: tech-debt
severity: medium
area: testing
related: [issue-0527, issue-0445, issue-0196, issue-0350, phase-340]
---

## Background

"Could not run" is several different facts, and the harness has historically
had one channel for all of them: `nros_tests::skip!` panics with `[SKIPPED]`,
nextest records the panic as `<failure>`, and `_rewrite-skipped-junit`
reclassifies it afterwards. (The panic is not a hack for its own sake: libtest
and nextest have no runtime-skip channel — `<skipped>` is reserved for the
static `#[ignore]` — so a runtime skip must either be a panic that is laundered
later, or a decision taken BEFORE the binary runs.)

The taxonomy that matters, because each row wants different handling:

| kind | example | correct outcome | decided |
| --- | --- | --- | --- |
| out of lane | tier 2 does not select this coordinate | deselect | before the binary runs |
| host capability | no `arm-none-eabi`, no docker, no FVP | skip | before the binary runs |
| build stage did not deliver | fixture missing / stale | **fail** | at the gate |
| assertion broke | — | fail | in the test |

## Already fixed (2026-08-15)

**Classes.** `skip_class!(lane, …)` emits `[SKIPPED:lane]`; plain `skip!` reads
as `capability` (what nearly all of its ~500 call sites mean).
`rewrite-skipped-junit` lifts the class onto `<skipped type="nros:…">` and
prints a per-class breakdown. Before this, a sweep's 170 skips could be
classified for **4** of them after the fact — the reason lived as prose inside a
panic body, and the `message=` that survived into junit held
`thread '…' panicked at …`.

**Fixture-absence is not a skip.** `require_prebuilt_binary_checks` — the shared
resolver every `build_*` helper funnels through — now PANICS without a
`[SKIPPED]` marker when a gate context is present (`NROS_TEST_SCOPE` /
`NROS_TEST_COORDS`), so no call site can launder it. A gated run has already
asserted at `_require-fixtures` / `check-fixtures-stale` that this lane's
fixtures exist and are fresh; a missing one is a broken promise, not an
environment fact. Ungated runs (a bare `cargo nextest` on a box that built
nothing) keep the recoverable `Err`. Both arms are asserted in
`fixture_absence_class_tests`.

## What remains

### 1. Prefer DESELECTION over an in-test skip

A deselected test uses the runner's native channel: no panic, no red console
line, nothing for the rewrite to launder. The harness already does this for
esp-idf, platformio, docker and the FVP (`env_exclude` in `test-all`), driven by
capability probes. Every `skip!` that can become a filter expression is one
fewer thing being post-processed, and the remaining `skip!`s become exactly the
cases genuinely undiscoverable until runtime (a port in use, a device absent, a
peer that failed to come up).

Worth measuring first: of the ~500 `skip!` sites, how many test a condition
knowable before the binary starts?

### 2. BUDGET the skips — the one that actually closes the hole

Classes make skips countable. They do not make them checkable. `170 skipped` is
indistinguishable from `170 tests silently did not run`, and no human eyeballs
that number — which is precisely how a lane greens over a coverage hole
(0445's absorbing STALE verdict, 0350's wholesale compile-check failure
reporting as skips).

The lane already knows which cells it expects to skip on this host: that is what
the coordinate machinery (`matrix::CELLS`, `interop::CELLS`, `NROS_TEST_COORDS`)
encodes, and `just fixture-staleness` already lists coordinates producing no
runtime result. So:

* derive the expected-skip set for the running lane;
* compare the actual `<skipped>` set against it;
* fail on a SURPRISE skip, and on an expected skip that unexpectedly ran.

That turns a skip from an escape hatch the harness tolerates into a claim the
harness checks. A `capability` skip on a machine that HAS the capability, or a
`lane` skip for a coordinate the lane selected, are both bugs that are currently
invisible.

### 3. Fold the remaining laundering sites

Three places still convert a resolver `Err` whose message contains
`"not prebuilt"` into a `[SKIPPED]` (`xrce_large_msg_test_binary` and two
siblings). They are now unreachable in gated runs — the resolver panics first —
but they encode the old rule and will be copied.
