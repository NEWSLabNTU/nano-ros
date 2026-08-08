---
id: 480
title: "0471 made readiness waits strict but converted 4 suites — 101 literal waits remain, and ~23 of them now fail"
status: open
type: bug
area: testing
related: [issue-0471, phase-277, issue-0157, issue-0164]
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
