---
id: 480
title: "0471 made readiness waits strict but converted 4 suites — 101 literal waits remain, and ~23 of them now fail"
status: resolved
resolved_in: phase-342
type: bug
area: testing
related: [issue-0481, issue-0471, phase-277, issue-0157, issue-0164]
---

## Resolution (2026-08-10) — all 32 remaining sites mapped, baseline emptied

The audit was the work, and it is done. Every literal the gate could still see
was resolved to the binary its site spawns, then pointed at that binary's
`output::*` constant. `scripts/readiness-marker-literal-baseline.txt` is now
**empty**: the gate enforces the rule outright, with nothing exempted, so there
is no longer a backlog to shrink.

**Three were real defects, not ambiguity** — each an `.expect(…)` on a marker
its binary does not print, i.e. a test that fails outright now that 0471 made
the wait strict:

| site | waits on | prints |
| --- | --- | --- |
| `zephyr.rs:860` | `build_native_listener()` | `Subscriber created for topic:` only |
| `zephyr.rs:1443` | `build_native_listener()` | `Subscriber created for topic:` only |
| `interop_e2e.rs:383` | `ros2-string-interop` | matched only by the accident that its prose line starts `Waiting for` |

The other 29 were correct-by-luck: they named a literal that happened to be a
prefix of the marker their binary prints. Correct-by-luck is what this class
is; the same literal one binary over is 5–30 s of silence.

### Two spellings collapsed, rather than a constant per spelling

`serial-listener` and `custom-transport-listener` printed
`Subscriber created on /chatter` where every other subscriber prints
`Subscriber created for topic: …` — one fact, two spellings, and no shared
constant can cover both. A `SERIAL_LISTENER_READY_MARKER` was written and then
deleted: adding it made `"Subscriber created"` newly ambiguous and the gate
immediately flagged a 30th site (`custom_transport_loopback.rs:73`) that had
been invisible while only one constant started that way. Both binaries were
converged on the shared line instead, which is phase-342's listener convergence
one binary further. `ros2-string-interop` gained the shared line too.

`WS_C_LISTENER_READY_MARKER` was renamed `LISTENER_WAITING_BANNER`: four
non-workspace binaries print `"Waiting for messages"`, so the workspace-only
name had stopped being true and invited exactly the second-spelling problem
above. Two constants were added for markers nothing covered —
`PARAM_TALKER_READY_MARKER` (`param-chatter-talker` publishes Int32, so
`TALKER_READY_MARKER`'s `"Publishing:"` never matches it).

### What the gate is worth now

Verified both directions: green at 0 baselined / 0 new, and red when a single
`"Waiting for"` is reintroduced. The CORRECTION below still stands — this class
never explained the ci-matrix reds, which are fixture coverage and staleness.
What it explains is the silence.

## Duplicates #481 — read that first

While this was being written, another session filed **#481** for the same class,
found by MEASUREMENT rather than by a failing test: after phase-342 W1 split the
pubsub fold, `rust_cyclone` sat at 34.1 s against `cpp_cyclone`'s 5.2 s, because
a settle step greped `"Waiting for"` — which C/C++ print and the Rust listener
never does. Fixed there: 34.1 s -> 4.0 s.

That is the stronger evidence, and it makes the real cost visible: a wrong
marker does not fail, it **burns the whole timeout in silence** and the test
still passes. #481 is the authoritative write-up.

What THIS issue adds and should be kept for: the full site audit — 101 literal
`wait_for_output_pattern` calls, each mapped to the binary it waits on (table
below). #481 names 12 further call sites with the same literal and 4 suspects;
the audit here is the superset and the mapping is the work needed either way.

Both issues independently reached the same conclusion about the fix: **do not
replace blindly**, because most sites wait on C/C++ binaries that DO print the
string.

## CORRECTION (2026-08-08, before any fix landed)

**The premise below is wrong about scale, and the issue is kept only for the
101-literal audit — not as the explanation of the ci-matrix reds.**

Two mistakes, both mine:

1. **The solo retest used bare `cargo nextest`**, which counts
   `nros_tests::skip!` panics as FAILURES — only `just test-all`'s junit rewrite
   turns them into skips. CLAUDE.md warns about this explicitly. Several
   "FAIL-SOLO" verdicts are actually `[SKIPPED] … build-fixture emitted the
   offline Placeholder stub` (`board_agnostic_run_plan`, `nav2_compat`), i.e.
   not failures at all.

2. **The banner diagnosis generalized from ONE test.** Re-running all 27 with
   full output captured shows the real distribution:

   | reason | n |
   | --- | --- |
   | `Test fixture binary not prebuilt` | 10 |
   | unclassified / skip-stubs / other | 12 |
   | `Failed to build <fixture>` | 3 |
   | `Test fixture is STALE` | 2 |

   `test_zephyr_to_native_e2e` — the single test the banner claim rested on —
   reports `not prebuilt` in the second run. The fixture state differed between
   the two runs, so the `did not print \`Waiting for\`` panic was reachable only
   once the binary existed. Real, but nowhere near the dominant cause.

So the ci-matrix reds are overwhelmingly **fixture coverage and staleness**, the
same family as the tier-2-needs-`lane=all` gap — not stale greps.

**What survives:** the 101 literal `wait_for_output_pattern` calls are still a
genuine latent violation of a rule CLAUDE.md has carried since phase-277, and
0471's strictness makes any wrong one fail loudly rather than silently pass. The
audit and the proposed gate below stay valid. What is retracted is the claim
that this explains the 27 failures.

Fixing 101 sites on a premise this weak would have been churn justified by a
number I had not checked.

---

## Symptom

`just ci-matrix` reports 29 test failures on a green `lane=all` fixture build.
Retested SOLO, one nextest invocation each with no concurrent QEMU load:

| verdict | count |
| --- | --- |
| **FAIL-SOLO** — reproduces alone | **27** |
| PASS-SOLO — in-sweep flake | 4 |

So these are NOT the QEMU-under-load flake class (287-W7). They are persistent,
and the great majority share one cause.

## Cause

A test waits for a banner the binary under test does not print:

```
native listener did not become ready: ProcessFailed(
  "native-rs-listener did not print `Waiting for` within 5s. Output:
   [INFO] nros: session open
   [INFO] Subscriber created for topic: /chatter")
```

Delivery works. The listener is up. The GREP is stale — `examples/native/rust/listener`
prints `Subscriber created for topic:`, never `Waiting for`.

**Issue 0471 already documented exactly this**, in `output.rs`'s own doc comment:

> Several suites (`qos`, `multi_node`, `safety_e2e`, `nano2nano`) waited for the
> literal `"Waiting for"` instead, a banner this binary does not print. That
> wait could never succeed, and NOTHING noticed, because
> `wait_for_output_pattern` returned `Ok` on timeout as long as the process had
> printed anything.

0471 fixed two things: it made the wait STRICT (so a missed banner fails instead
of passing), and it converted the four suites it had found. The strictness is
correct and is what surfaced this. But the conversion stopped at those four —
**101 literal `wait_for_output_pattern("…")` calls remain** across the test tree,
and the ones aimed at Rust binaries now fail loudly where they used to pass
falsely.

This is CLAUDE.md's "fix the CLASS, not the reported site", and the recurrence
is the same shape as the sizes-header mirror chain: a real fix landed only where
the symptom was seen.

## Why a blanket replace is WRONG

`"Waiting for"` is not simply a bad string — it is correct for some binaries:

* `examples/native/c/listener` prints `Waiting for messages (Ctrl+C to exit)...`
* `examples/native/cpp/listener` prints the same
* `examples/native/c/service-server` prints `Waiting for service requests`
* `examples/native/rust/listener` prints **no such banner**

So the literal is right or wrong depending on WHICH binary the site spawns. A
sed across the tree would break the C/C++ sites while fixing the Rust ones.
Each site has to be mapped to the binary it waits on, then pointed at the
matching `nros_tests::output::*` constant (`LISTENER_READY_MARKER`,
`INT32_SINK_READY_MARKER`, `WS_C_LISTENER_READY_MARKER`, …), adding a constant
where none fits.

## Distribution of the 101

Top sites by count (`git grep -n 'wait_for_output_pattern("' -- 'packages/testing/nros-tests/tests/*.rs' | grep -v output::`):

```
12  native_api.rs                    "Waiting for"
 5  large_msg.rs                     "Ready: listening"
 4  zephyr.rs                        "Waiting for"
 4  safety_e2e.rs                    "Waiting for"
 3  services.rs                      "Waiting for service"
 3  large_msg.rs                     "Ready: listening"
 3  custom_msg.rs                    "All serialization tests passed"
 2  zero_copy.rs                     "Waiting for"
 2  xrce_ros2_interop.rs             "Action server ready"
 2  orchestration_tiers_freertos.rs  "Network ready."
 2  native_async_roundtrip_e2e.rs    "Waiting for"
 2  esp32_emulator.rs                "Waiting for messages..."
 …
```

Note `safety_e2e.rs` still has 4 literals despite being named as converted by
0471 — worth checking whether that conversion was partial.

## Fix

1. Map each literal to the binary its site spawns; replace with the matching
   `output::*` constant, adding constants where needed.
2. Then gate it: forbid a string literal as the pattern argument of
   `wait_for_output_pattern` in `tests/`, so site 102 cannot be written. The
   rule has been stated in CLAUDE.md since phase-277 and has now been violated
   101 times, which is what an ungated convention is worth.

## The 4 genuine flakes (passed solo, failed in sweep)

Real QEMU-under-load flake, not this class — retest before believing any of them:

* `nros-tests::emulator test_qemu_rtic_service_e2e`
* `nros-tests::native_example_reqresp_e2e native_example_reqresp`
* `nros-tests::rtos_e2e test_rtos_pubsub_e2e::…Nuttx::…Rust`
* `nros-tests::zephyr example_e2e::case_27_xrce_cpp_action_e2e`

## Not all 27 are confirmed to be this cause

The banner mismatch is confirmed for the zephyr/native-listener family by
reading the panic text. The rest were classified only as FAIL-SOLO; their panic
messages were captured truncated. Read each before assuming — some (e.g.
`cli_bringup_zephyr`, which reports `Test fixture binary not prebuilt:
build/west-fixtures/…`) are clearly a DIFFERENT problem: fixture coverage, not a
grep.

## Reproduce

```sh
source ./activate.sh
cargo nextest run -p nros-tests --test zephyr \
    -E 'test(=test_zephyr_to_native_e2e)' --test-threads=1
```

5 s, deterministic.
