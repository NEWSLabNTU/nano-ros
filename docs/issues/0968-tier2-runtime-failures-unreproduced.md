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
