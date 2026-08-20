---
id: 737
title: "Both `freertos-posix` cells publish and deliver nothing — and no recipe
  built their fixtures, so the lane reported green on binaries that did not exist"
status: open
type: bug
area: testing, platform, rmw
related: [phase-370, issue-0405]
---

## Two defects, and the first hid the second

**1. The rows had no producer.** `examples/fixtures.toml` gained
`workspace-c-freertos-posix` and `workspace-cpp-freertos-posix` with
`platform = "freertos-posix"`, but `just/freertos.just` only ever calls
`workspace-fixtures-build.sh freertos …`. The manifest builder selects rows by
platform, so `freertos-posix` matched nothing: `just freertos build-fixtures`
finished green having built neither. This is issue #405's exact shape, and the
gate that names it (`matrix_fixture_coverage::
every_fixture_token_is_producible_by_the_module_that_owns_it`) was RED on main
saying so.

Fixed here — `just freertos build-fixtures-posix`, a sibling of
`build-examples` rather than a line inside it, because `build-examples` IS the
ARM QEMU lane (hard-fails without lwIP, cross-compiles every leaf) and this
board is host cc + host sockets with no emulator. `workspace-fixtures-build.sh`'s
lane-env guard now reads `freertos|freertos-posix)` so a direct call still fails
loud rather than deep-panicking.

**2. With the fixtures actually built, both cells fail.** The talker publishes;
nothing is received:

```
thread 'freertos_posix_c_entry_delivers_over_cyclonedds' panicked at
packages/testing/nros-tests/tests/freertos_posix.rs:79:9:
freertos-posix-c: no `Received:` line — nothing was delivered.
Waiting for messages
[talker_pkg] sent: 0
[talker_pkg] sent: 1
... through sent: 7
```

The C++ cell fails identically (`Published: 0..7`, no `Received:`). Both are
in-process entries, so this is a participant that publishes into a graph its own
subscriber is not in — the discovery/domain half, not the transport.

## Not caused by the change that found it

Found while landing phase-359 W10's clock ruling, which is not responsible:
stash the `packages/core` + `packages/api` edits, `just setup-cli`, rebuild the
two fixtures, run — **2 failed** on upstream `main` too.

## Why this contradicts the phase doc

`docs/roadmap/phase-370-freertos-posix-board-cyclone.md` says W3 LANDED and
that both cells "build on a plain Linux host and deliver `/chatter` end to
end". They build. They do not deliver. The likely explanation is that the
bring-up run was a hand-invoked build in a shell whose env differed from the
lane's (a domain id, a `CYCLONEDDS_URI`, or an interface selection) — worth
recovering, because whatever that env was is what the cell needs and the lane
does not supply.

The acceptance to add once fixed: the cell must pass through
`just freertos build-fixtures` + `cargo nextest run -p nros-tests --test
freertos_posix` and nothing else, so the next reader cannot repeat the
hand-build.

## Reproduce

```
just freertos build-fixtures-posix
cargo nextest run -p nros-tests --test freertos_posix
```

## Second host, 2026-08-21 — defect 2 does NOT reproduce; defect 1 is confirmed mine

**Defect 1 is exactly right and the diagnosis of how it happened is right too.**
The rows went in with no recipe, and phase-370's acceptance was recorded from a
hand-invoked `workspace-fixtures-build.sh freertos-posix c`. That is the
"hand-build in a shell whose env differed" this issue suspected — it was not an
env difference, it was a build nobody else could run. The gate that names the
class was red and I did not see it, because
`matrix_fixture_coverage` is a nextest target and I had been running `just
check`, which does not include it. A green `just check` said nothing about it.

**Defect 2 does not reproduce here**, through the recipe this issue added:

```
just setup-cli
rm -rf examples/workspaces/{c,cpp}/build-workspace-fixtures-freertos-posix
just freertos build-fixtures-posix                        # RC=0
cargo nextest run -p nros-tests --test freertos_posix --run-ignored all
  2 tests run: 2 passed, 0 skipped
```

Four arms, all delivering:

| arm | result |
| --- | --- |
| fresh fixtures via `build-fixtures-posix`, then the test | 2/2 pass |
| same, with the #740 cmake fix REVERTED | 2/2 pass |
| same, under 64 spinners on 48 cores (load ~68) | 2/2 pass |
| the binary bare: `env -u ROS_DOMAIN_ID -u CYCLONEDDS_URI ./freertos_posix_entry` | `Received: 0,1,2…` |

The second arm was a specific hypothesis worth killing: #740 (the config-header
mirror invisible to Makefiles) makes a TU compile against the in-tree STUB
header, and issue 0268 records that exact stale-sizes path producing "memory
corruption that surfaces as unrelated runtime failures (freertos C:
register_subscription -1)". It would have explained a publish-but-never-receive
perfectly. It is not the cause — reverted, still passes.

Host: `ROS_DOMAIN_ID`, `CYCLONEDDS_URI`, `RMW_IMPLEMENTATION` all unset;
`libddsc.so.0` from `/opt/ros/humble`; loopback plus three physical interfaces.

## What changed rather than an argument from "works for me"

The tests now print the inputs that could differ between hosts when they fail —
the domain asked for, the ambient `ROS_DOMAIN_ID` / `CYCLONEDDS_URI` /
`RMW_IMPLEMENTATION`, and the reminder that both nodes share ONE participant, so
a total absence of `Received:` is a participant/domain question and not a
transport one. An unreproducible red costs a second investigation unless the
first leaves its evidence behind.

**The `#[ignore]`s are LEFT IN PLACE.** Two passing runs on one host is not
grounds for lifting a park that someone set from a real failure on another, and
"works for me" is the weakest reason to restore coverage. What is needed is the
failing host's output with the diagnostics above.

## Parked, not left red

Both tests carry `#[ignore = "issue 0737 …"]`. That is not the usual "a red is
better than a skip" trade being dodged: these cells were never green in a lane —
their fixtures had no producer, so they SKIPPED, and turning that into a hard
tier-1 red for everyone would be charging the whole repo for a defect the fix
above merely made visible. Un-ignore with the delivery fix, in the same commit.
