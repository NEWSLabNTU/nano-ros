---
id: 968
title: "Tier 2 has ~12 runtime e2e failures on main, unreproduced — nobody has run the tier in a long time"
status: open
area: testing
severity: medium
found: 2026-08-31
related: [0967, phase-410, RFC-0061]
---

# What was measured, and what was not

A full tier-2 run on a frozen tree with freshly rebuilt fixtures (8/8 modules
OK):

```
1795 tests run · 1595 passed (2 flaky) · 6 skipped
Real failures: 17
```

`Real failures` is the junit-rewritten count. Nextest's raw line said **200
failed** — the difference is `nros_tests::skip!` panics, which a bare read
counts as failures. Any triage starting from 200 is starting from the wrong
number.

Of the 17, **five were lane/justfile meta-tests** and are fixed (they failed
identically on a fresh `origin/main` worktree; the tier ladder had drifted from
the justfile and its own gate could not see module recipes).

The remaining **12 are runtime e2e**, and this issue exists so they are tracked
rather than remembered:

```
emulator        test_qemu_rtic_service_e2e
esp32_emulator  test_esp32_talker_listener_e2e, test_esp32_workspace_entry_e2e,
                test_esp32_to_native, test_native_to_esp32
native_api      test_threadx_linux_cyclonedds_{action,service,talker_to_native_listener}
zephyr          example_e2e::case_{21,24,27}_xrce_cpp_{pubsub,service,action}_e2e
logging_smoke   logging_smoke_esp32_qemu_emits_every_severity
```

They fail SOLO as well as in the sweep, so they are not the QEMU load-flakiness
CLAUDE.md warns about.

## They are not from the phase-405/407/410 work — proved by diff, not argued

Across every tree that feeds a built image — `packages/`, `examples/`,
`zephyr/`, `integrations/`, `cmake/` — that branch changes exactly two files,
both in `packages/testing/nros-tests/src/` (the test harness; compiled into no
target image). The esp32, zephyr, threadx and QEMU images are therefore
byte-identical to main's, so a runtime difference cannot originate there.

## Why nobody noticed

`post-submit`'s tier-2 job has **never run** — it is interlocked on
`vars.NROS_SELF_HOSTED_READY`, which is not set, and a skipped job does not
colour its run. `host-tests`, the only other lane that executes E2E, has been
red for 20 consecutive runs on issue 0967, dying before its tests start.

So between them, no fixture-backed test has run in CI for a long time. A
backlog of undetected runtime failures is exactly what that predicts.

## NOT DIAGNOSED — read this before acting

**No root cause is claimed for any of the 12.** One sample was examined
(`test_threadx_linux_cyclonedds_service`): the client printed `Locator:` empty
and the server produced no output at all, i.e. it did not start. Whether that
is one cause or several is unknown.

This issue deliberately stops at the list. Four issues (0859–0862) were filed
from a sweep in this repo and all four retracted, two of them carrying confident
wrong root causes — which is worse than the bogus filing, because it aims the
next person at a dead end.

## Work

1. Rebuild tier-2 fixtures and re-run the 12 — the artifacts from the measuring
   run were cleared by `runner-sweep.sh` afterwards, so nothing here is
   currently reproducible.
2. Triage per suite, not per test: esp32 (5 of 12) and threadx-cyclone (3) are
   the two clusters and likely share a cause each.
3. Check each against main at the commit the fixtures were built from —
   `stat -c '%y'` on the artifact against `git log -1 --format=%ci`, per the
   0859-0862 rule.
4. File per cause, once reproduced. Not before.

## Step 1 attempted 2026-09-04 — and it found TWO build failures before any test ran

`just build-test-fixtures lane=tier2` on `a1c6d0d22`. Seven of eight modules
built (zephyr, native, qemu, freertos, nuttx, threadx_linux). The other two are
new findings, both COMPILE/BUILD failures rather than runtime ones, and both are
this issue's own thesis arriving one stage earlier than it predicted:

* **[#1023](1023-sertype-hosted-includes-break-freestanding.md)** —
  `threadx_riscv64` cannot compile `nros_sertype.cpp`: it includes `<memory>`
  and `<string>` and the target is freestanding. The file is new in issue 0970's
  commit, and `examples/fixtures.toml:3146` declares the coordinate, so this is
  a supported cell that has been unbuildable since it landed.
* **[#1025](1025-esp32-flash-image-consumer-drops-the-row-variant.md)** — ESP32
  QEMU flash images cannot be packed. The ELF builds fine; the packer looks in
  `build/cargo-fixtures/qemu-esp32-baremetal/` while the build writes to
  `qemu-esp32-baremetal-4118800323`, because the packer asks
  `nros_fixture_row_artifact_dir` for the group dir with the row's env stripped
  (`"" ""`). Live since `41a7d8de7` on 2026-08-31 — hours before this issue was
  filed.

**1025 bears directly on this issue's list and does not close any of it.** Five
of the twelve are esp32 and all five need a flash image that cannot be produced.
That is a plausible single cause for the whole esp32 cluster; it is NOT
established as their cause, because what was reproduced is the BUILD failing,
not those tests failing for that reason. Establishing it means fixing 1025,
rebuilding, and re-running the five. Written this way on purpose — 0859-0862
were four issues filed from a sweep in this repo and all four retracted, two
carrying confident wrong root causes.

**Status of the twelve: still unreproduced.** The seven non-esp32 ones
(qemu-rtic 1, threadx_linux 3, zephyr xrce-cpp 3) have their modules built and
are ready to run; the five esp32 ones are blocked on 1025.

**Method note for whoever continues.** Run each candidate SOLO, not in a sweep —
CLAUDE.md's rule for QEMU reds, and this issue's list came out of a sweep. And
`cargo nextest` reports a `nros_tests::skip!` panic as a FAILURE while a filter
matching nothing exits 0, so a re-run needs four verdicts (pass / fail /
skipped-precondition / not-found), not two. A two-verdict harness would report
an unmet precondition as a regression and a renamed test as a pass.

## The threadx_linux cyclonedds cluster, DIAGNOSED 2026-09-04 (3 of the 12)

`test_threadx_linux_cyclonedds_{service,action,talker_to_native_listener}`.
Reproduced solo, on fixtures built at `a1c6d0d22`, and narrowed by elimination.
Every line below is a measurement; the two that turned out to be measurement
ARTIFACTS are kept, because both looked like findings.

### What this issue said, and why it was wrong

> the client printed `Locator:` empty and the server produced no output at all,
> i.e. it did not start

**The server starts.** Run directly it prints its whole banner through to
`Waiting for service requests (Ctrl+C to exit)...`. The empty `server` section
in the failure message is a HARNESS artifact:

```rust
server.wait_for_output_pattern(SERVICE_SERVER_READY_MARKER, 30s);  // consumes the banner
let server_out = server.collect_until("Incoming request", 2s);      // only what came after
```

`wait_for_output_pattern` has already consumed the startup output, so
`server_out` holds only what arrived afterwards — nothing. Anyone reading that
panic message concludes the server never started, which is what this issue did.

### Eliminated, each by measurement

| hypothesis | result |
| --- | --- |
| server does not start | starts; full banner to a file AND through a pipe |
| stdio block-buffering loses the output | 496 bytes reach a pipe; the harness also uses `stdbuf` |
| domain mismatch | server honours `ROS_DOMAIN_ID`; both bind 34150/34151 on domain 107 |
| isolated network stack (NetX/NSOS) | 4 host UDP sockets, one on the real LAN address |
| multicast group not joined | both threadx and native join the DDS group |
| stale binaries straddling 0970's sertype change | all three built the same night, well after it |
| discovery is merely slow | client TIMES OUT after a 75 s budget |
| the client or the Cyclone backend is broken | **CONTROL PASSES**: native client + native Cyclone server returns `Result of add_two_ints: 5` |

### What IS established

The native client never matches any endpoint of the threadx-linux server.
Comparing `Tracing/finest` from the working pair against the failing one, the
working trace carries `ACKNACK` traffic and
`ddsi_delete_proxy_{participant,reader,writer}`; the failing trace carries none
of them. ACKNACK is reliable-protocol traffic that only occurs with a matched
remote endpoint, so its absence is not a shutdown-path artifact the way the
`delete_*` lines partly are.

SPDP frames ARE seen on both sides (5-6 each), so packets flow. **Discovery does
not complete: the participant is never admitted.**

### NOT established — what the next person should do

WHY the SPDP is not admitted. That needs the announcement's CONTENT compared
between a threadx and a native participant (GUID prefix, locators, lease
duration, domain tag), which is packet-level work this stopped short of.

Do not assume it generalises to the other two of the three. `service` is what
was diagnosed; `action` and `talker_to_native_listener` share a fixture family
and a plausible cause, and that is a hypothesis, not a result.

### Two measurement artifacts, kept because both looked like findings

1. **"Zero proxy participants discovered."** `new_proxy_participant` is not the
   trace spelling — the WORKING control reports zero for it too. Caught by
   running the same grep against the pair that passes, which is the only reason
   it was not written up as the finding.
2. **Unequal time budgets.** The control ran 25 s and the first threadx attempt
   20 s, so "the client never completes" was not comparable until the 75 s run
   made it so.
