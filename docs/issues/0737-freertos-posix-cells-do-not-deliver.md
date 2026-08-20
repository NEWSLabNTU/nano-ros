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

## Parked, not left red

Both tests carry `#[ignore = "issue 0737 …"]`. That is not the usual "a red is
better than a skip" trade being dodged: these cells were never green in a lane —
their fixtures had no producer, so they SKIPPED, and turning that into a hard
tier-1 red for everyone would be charging the whole repo for a defect the fix
above merely made visible. Un-ignore with the delivery fix, in the same commit.
