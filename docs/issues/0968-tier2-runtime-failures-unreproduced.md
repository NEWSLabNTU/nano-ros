---
id: 968
title: "Tier 2 has ~12 runtime e2e failures on main, unreproduced — nobody has run the tier in a long time"
status: open
area: testing
severity: medium
found: 2026-08-31
related: [0967, 0998, 0999, phase-410, phase-414, RFC-0061]
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

## REPRODUCED 2026-09-03 — all of them, and the list was one short

Fixtures rebuilt for `lane=tier2`, all eight modules OK, `EXIT=0`. Freshness
checked per the 0859–0862 rule before believing anything: fixture stamp
`started_at=2026-09-03T04:52:19Z` against HEAD `e8f7b93ff` at `04:24:23Z` — the
artifacts are NEWER than the commit, so these results are about this tree.

Getting there needed two blockers fixed first, both found by trying and neither
one of the twelve — which is this issue's own prediction arriving:

* **issue 0998** — the Cyclone backend had not cross-built since 2026-08-31
  (`nros_sertype.cpp` includes `<memory>` on a `-nostdinc++` board).
* **issue 0999** — `nros build` preflight told every nuttx build to
  `rustup target add` a Tier-3 target rustc does not distribute, so the nuttx
  module could never pass preflight on any host.

**Thirteen tests fail, not twelve.** The threadx cluster has a fourth with the
same signature, `test_threadx_linux_cyclonedds_cpp_talker_to_native_listener`,
which the list above omits.

### Four distinct signatures, not twelve problems

| cluster | tests | signature |
| --- | ---: | --- |
| threadx-linux cyclone | 4 | client starts, service created; **roundtrip produces no calls/requests** |
| zephyr XRCE C++ | 3 | **no delivery at all** — 0 samples / no reply / no result |
| esp32 | 4 | image boots, ethernet up, reaches `entering spin loop` — **never creates entities** |
| qemu rtic | 1 | `service client never logged a service result` |

The fifth, `logging_smoke_esp32_qemu_emits_every_severity`, is NOT a runtime
failure and does not belong with the others: it fails in 1.0 s on
`BuildFailed("Test fixture binary not prebuilt: .../logging-smoke-esp32-qemu.bin")`.
It has a row in `examples/fixtures.toml:3208`, so this is a lane-coverage
question — whether `lane=tier2` builds that row — not a defect in the image.

Zephyr and esp32 failures survived every nextest retry, so they are not the
QEMU load flakiness CLAUDE.md warns about.

### The one diagnosis this issue DID offer is wrong

The sample above says of `test_threadx_linux_cyclonedds_service`: "the server
produced no output at all, i.e. it did not start."

**The server starts.** Run with the test's exact environment
(`ROS_DOMAIN_ID=107`, its `LD_LIBRARY_PATH`), it prints 496 bytes:

```
Locator: tcp/127.0.0.1:7447
Domain ID: 107
Service created: /add_two_ints
Waiting for service requests (Ctrl+C to exit)...
```

The empty `server:` section is a HARNESS ARTIFACT.
`ManagedProcess::wait_until_pattern` reads bytes off the pipe and RETURNS them
without retaining them, and the test calls it as
`let _ = server.wait_for_output_pattern(...)` — discarding the startup banner.
The failure dump then shows only what arrived afterwards, which is nothing,
because the server is idle and waiting.

So "produced no output" is true of the DUMP and false of the PROCESS, and the
inference drawn from it sent the reading in the wrong direction. Two of my own
hypotheses died the same way and are worth recording so nobody re-runs them:
stdout buffering (wrong — `service-server/src/main.c:102` already sets
`_IOLBF`, and a plain pipe yields the same 496 bytes) and a mis-set
`LD_LIBRARY_PATH` in issue 0774's shape (wrong — it runs fine with the test's).

### Still NOT diagnosed

Four signatures, four unknown causes. Nothing here claims one. What has changed
is that each cluster now has a stable reproduction on a tree whose fixtures are
provably newer than its commit, a signature that distinguishes it from the other
three, and — for the threadx cluster — one wrong explanation removed from in
front of it.


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
