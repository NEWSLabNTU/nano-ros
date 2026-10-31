---
id: 2183
title: "A drifted Zephyr Rust leaf lock runs one doomed `cargo update` per off-pin crate and reports `could not realign 32 crate(s)` as a failure"
status: resolved
type: bug
severity: low
area: [build, cli]
related: [issue-1941, issue-1937, issue-2179, phase-485]
found: 2026-10-31
resolved: 2026-10-31
---

## Symptom

Building a Zephyr Rust example leaf through the fixture lane (seen while fixing
issue 2179, on the `rust/talker` and `rust/talker-serial` rows, Zephyr 3.7)
printed, in the row's driver log:

```
nros:   could not realign 32 crate(s) in .../examples/zephyr/rust/talker; first, `aho-corasick` 1.1.5 -> 1.1.4: error: no matching package named `zephyr-build` found
nros:   36 drift(s) in .../examples/zephyr/rust/talker the realign could not fix; re-seeded its Cargo.lock from .../locks/cargo-registry.lock
```

The build still succeeded. The message read like a failure to hold the lock
to the pin set. It was not one: the lock did end up on the pins.

## Measured cause

`scripts/build/zephyr-fixture-run-one.sh` runs `nros ws seed-lock` on the leaf
before `west build` (issue 1937). For an existing lock in a checkout that is
`drift::realign_or_reseed` (issue 1941). It first tries to move each off-pin
crate with `cargo update -p <name>@<have> --precise <want>`, run in the leaf
ALONE. Then it re-seeds from the pin set whatever is still off.

A Zephyr Rust leaf names `zephyr = "0.1.0"` and `zephyr-build = "0.1.0"` by
registry version. Only the west build's patch table supplies them, from the
workspace's `modules/lang/rust`, and the lock records them as path packages
(no `source`). Run alone, cargo cannot resolve the leaf at all, so **every**
move fails with the same error. Which crate is being moved makes no
difference. The realign could never succeed for this root. The re-seed was
always the only thing that brought the lock back onto the pins, and it was
reported as the fallback after a failure.

Measured on `examples/zephyr/rust/listener`, whose lock (Oct 7) predates the
pin set:

- `nros ws check-lock`: `36 drift(s)`. 32 are off-pin, which the realign
  tries to move. 4 are in no pinned range.
- `nros ws seed-lock`, old CLI: the two lines above. **16.3 s** wall, 32
  `cargo update` runs, all refused for `zephyr-build`. Then `0 drift(s)`.

It repeats only while a lock is stale. Each stale lock prints it once. Two
things make a lock stale: a lock resolved before #1937's seed (5 of the 6
Zephyr leaves and all 9 generated Zephyr workspace entries in this tree held
one, each at 36 to 39 drifts), and a pin set that moved since the lock was
written. After the re-seed and one `west build`, the next `seed-lock` is
silent (measured on `listener` and `talker`). The reported "on every build"
was NOT reproduced once the lock was clean.

`nros build` has the same blind spot after the build
(`drift::enforce_cargo_root`): its realign of a west-resolved root fails the
same way. It refused with the generic `could not realign` line and did not
say that the root cannot be moved here at all.

## Fix

- `drift::supplied_by_outer_build(cargo_err, lock)`: a failed move names a
  package (`no matching package named `X``), and the root's own lock
  records X as a path package with no `source`. Then the root resolves only
  inside its outer build. The check reads cargo's answer and the lock, so
  there is no list of crate names. A missing REGISTRY crate, a crate the lock
  does not hold, or a lockstep refusal is not this case.
- `drift::realign_root`: once one move shows that, cargo is not asked again.
  The other moves would fail identically, so the rest are recorded as stuck
  without running cargo.
- `realign_or_reseed` re-seeds such a root and says so once, with the reason,
  not as a failure:
  `re-seeded <leaf>'s Cargo.lock from <pins>: 36 drift(s), and the root
  resolves only inside its build (`zephyr-build` comes from the build's own
  patch table), so `cargo update` cannot realign it`.
- `enforce_cargo_root` (`nros build`) leaves its drift and its refusal as
  they were. It replaces `could not realign` with the same reason.

Real drift is not hidden. The re-seed still prints the drift count and runs
only when there was drift. A crate in no pinned range is still drift after
the re-seed, and `nros build` still refuses off-pin output.

## Sweep

Every root in the tree whose lock holds a package only an outer build patches
in (`find examples packages/testing packages/boards -name Cargo.lock`, then
grep for a sourceless `zephyr-build`) is a Zephyr Rust root. That is the 6
`examples/zephyr/rust/*` leaves and the generated workspace entries under
`examples/workspaces/*/build/zephyr-zenoh/*_entry`. Both go through the same
two functions. The check keys on the lock, not on Zephyr, so another
outer-patched crate is covered the same way. Measured with the fixed CLI:
`examples/zephyr/rust/action-client` and
`examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry` each went from 36
drifts to `0 drift(s)` with the one line. (One orphan lock,
`examples/workspaces/rust/src/zephyr_entry_robot1/Cargo.lock`, has no
manifest beside it. It is local residue, not a root.)

## Evidence

- **Before** (old CLI, `listener`'s stale lock): `could not realign 32
  crate(s) ... zephyr-build`, plus the fallback line, 16.3 s.
- **After** (fixed CLI, same lock): the one re-seed line, **1.2 s**,
  `0 drift(s)`, and the lock is byte-identical to `locks/cargo-registry.lock`.
  `NROS_ZEPHYR_FIXTURE_FILTER='build-rust-(listener|talker)-zenoh$' just zephyr
  build-fixtures` then passed (rc 0, `build-rust-listener-zenoh/zephyr/zephyr.exe`
  relinked). Its driver log holds only the re-seed line, both leaves are
  `0 drift(s)`, and a further `seed-lock` prints nothing.
- **Negative control**: `drift.rs` put back to `origin/main` and the CLI
  rebuilt. The same lock prints `could not realign 32 crate(s)` again, in
  16.3 s.
- Unit tests:
  `drift::tests::a_package_only_the_outer_build_patches_in_makes_the_root_unresolvable_alone`
  (the predicate, with three cases that must not match) and
  `drift::tests::a_root_only_its_build_resolves_asks_cargo_once_not_once_per_crate`
  (32 moves cost 1 call; any other refusal still costs one call per move).
