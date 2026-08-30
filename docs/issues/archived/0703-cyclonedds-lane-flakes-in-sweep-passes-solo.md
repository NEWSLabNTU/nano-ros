---
id: 703
title: "`check-rmw-cyclonedds` fails intermittently INSIDE `just check` and
  passes solo — two different tests, same session"
status: resolved
type: bug
area: testing, rmw
related: [phase-359, issue-0319, issue-0580, issue-0161]
---

## Observed

Two `just check` runs on 2026-08-19, same working tree, ~40 minutes apart,
each red at `check-rmw-cyclonedds` on a DIFFERENT test:

```
The following tests FAILED:
	  4 - nros_rmw_cyclonedds_pubsub_smoke (Failed)
...
The following tests FAILED:
	  5 - nros_rmw_cyclonedds_data_roundtrip (Failed)
```

Solo, immediately after each, the same lane passes:

```
just check rmw-cyclonedds
100% tests passed, 0 tests failed out of 17
```

Tally for the session: **2 red in ~5 in-sweep runs, 0 red in 4 solo runs**
(one after the first failure, three consecutive after the second). Both reds
re-ran green in the following full `just check` with no code change between.

## Why this is filed rather than dropped

Two different tests failing means it is not one test's bug, and passing solo
means it is not the code the sweep just built. What is left is the environment
the sweep creates — CPU load, and DDS discovery on a machine already running
other lanes. That is the shape CLAUDE.md records for the QEMU lanes ("six nuttx
lanes failed 3/3 in-sweep, passed solo — retest a QEMU red SOLO before filing"),
and this is the first time it is written down for the Cyclone lane.

Recording it so the next person who sees a red here does not spend the
afternoon bisecting a change that is not responsible. It cost part of one
already.

## What is NOT yet known

- Whether the two tests share a mechanism (both open a participant and expect
  discovery within a fixed window) or fail for unrelated reasons. Neither
  failure's ctest output was captured beyond the summary line — `just check`
  prints the tail, and the per-test log was not kept.
- Whether the domain ids collide. The Cyclone fixture pairs bake distinct
  domains (50–58) precisely so parallel SPDP does not interfere; the ctest suite
  is a separate set and its domain allocation has not been audited against the
  lanes running beside it.
- Whether it reproduces under deliberate load, which is the cheap next step:
  run the lane with a parallel `just check workspace` and see if the rate rises.

## Resolution — the domain picker walked into the OS's ephemeral port range

Neither hypothesis in this issue was right, and the answer was in neither test.

Cyclone derives its RTPS ports from the domain arithmetically — `7400 + 250*D`
for multicast discovery, `+10 + 2*participantIndex` for unicast. Linux hands out
ephemeral ports from 32768 (`/proc/sys/net/ipv4/ip_local_port_range`), and
`7400 + 250*102 = 32900` is inside that range. So **from domain 102 up, the port
a participant MUST have is one the OS may already have given to any other
process on the box.** The bind fails outright:

```
ddsi_udp_create_conn: failed to bind to ANY:44900: address in use
open failed
```

`open failed` is `create_session` returning non-OK — the test dies before it
does anything it was written to test, which is why the two reds were on
different tests and why neither implicated its own subject matter.

Issue 0580's assigner picked `getpid() % 232 + 1`. Domains 102..214 map into the
ephemeral range on a default Linux, so **~49 % of processes drew a domain whose
port the OS was free to hand to someone else** — and how often that actually
collided is set by how many ephemeral ports are in use, i.e. by machine load.
That is exactly a red that is ~2-in-5 inside `just check` and 0-in-4 solo, on
whichever test happens to run while the port is taken.

### Measured

Boundary, with UDP 32768–34000 held (the ports domains 100..105 need):

| domain | discovery port | result |
| --- | --- | --- |
| 100 | 32400 | rc=0 |
| 101 | 32650 | rc=0 |
| **102** | **32900** | **rc=2 — bind failure** |
| 103 | 33150 | rc=2 |

A/B on the real binary, holding all 339 RTPS ports of domains 102..232, same
host, same hog, consecutive PIDs:

| tree | `nros_rmw_cyclonedds_pubsub_smoke` |
| --- | --- |
| pristine (`% 232`) | **40 / 40 failed** |
| fixed (`% 101`) | **0 / 40 failed** |

Full suite with the fix under the same hog: 4 consecutive runs, `100% tests
passed, 0 tests failed out of 17`.

(Consecutive runs share a PID neighbourhood, so the 40 pristine failures are one
sample of the band rather than 40 independent draws. That does not weaken the
conclusion — it shows the band is reachable and unconditionally fatal once
entered.)

### The fix

`101` in all three assigners, which are one scheme in three languages:

* `packages/testing/nros-tests/src/lib.rs` — `TEST_DOMAIN_MAX`
* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/nros_test_domain.h` —
  `NROS_TEST_DOMAIN_MAX`
* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh` —
  `NROS_TEST_DOMAIN_MAX`

101 is the last domain with margin for the per-participant offsets
(`7400 + 250*101 + 11 + 2*9 = 32679 < 32768`), and it is exactly the range ROS 2
documents as safe on Linux — so a value the assigner produces is one a user
could legally have set by hand.

The API's `DOMAIN_ID_MAX = 232` is deliberately untouched: a user may legally
name any domain, and refusing one would be a different bug. The ceiling belongs
to what the tests CHOOSE, not to what the runtime accepts.

### Gate

`check-test-domain-assignment.sh` already enforced "assign, never name a
literal" (0580) and already knew the three assigners by path — so the ceiling
clause went there rather than into a second script. It asserts each assigner
folds into `1..=101` and that none folds modulo 232, because **three files
agreeing on 232 is precisely the bug** and the old clause (`grep -q 232`)
asserted exactly that. Mutation-checked: setting `TEST_DOMAIN_MAX = 232` fails
the gate naming the file, restoring it passes.

One wrinkle worth the line it costs: the gate strips comments before reading,
because all three files legitimately DISCUSS the old range — that record is why
the ceiling exists. `#` is a comment in the shell file and a preprocessor
directive in the header, so it is only stripped when what follows is not a
directive. The first version flagged its own documentation.

### Same message, different cause — twice

`ddsi_udp_create_conn: failed to bind … address in use` is the identical error
Phase 177.33 fixed by adopting nextest's `NEXTEST_TEST_GLOBAL_SLOT`. That was
two of OUR participants landing on one domain, and the slot cures it. This is our
port colliding with the OS's own ephemeral allocator, which no amount of
uniqueness among our processes can avoid. A recurring error string is not
evidence of a recurring cause.

## Next step, if it recurs (as filed)

Capture the failing test's own output rather than the ctest summary:

```
cd packages/rmw/cyclonedds/nros-rmw-cyclonedds/build
ctest --output-on-failure -R nros_rmw_cyclonedds_data_roundtrip
```

If it is discovery timing, the fix is a bounded wait on a condition rather than
a fixed window (the repo's `condition-based-waiting` rule); if it is a domain
collision, it is the 0161 class one lane over.
