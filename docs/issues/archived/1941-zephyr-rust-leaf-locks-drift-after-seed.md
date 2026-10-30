---
id: 1941
title: "The six Zephyr Rust example leaves still drift from the pin set after #1937 seeds them, and the drift check stops the tier-2 nightly's fixture build"
status: resolved
type: bug
severity: medium
area: [build, ci, examples]
related: [issue-1937, issue-1924, phase-513]
found: 2026-10-17
resolved: 2026-10-31
resolved_in: "b28d9cd8e4 (phase-513 R3)"
---

**Home: [phase-513](../../roadmap/archived/phase-513-nightly-reds-with-no-home.md)** — R3.

## What happened

The first tier-2 nightly to run with #1937 (`ea58e7cb21`, "hold every fixture
leaf's lock to the pin set") failed in its fixture build, before any cell ran.

- Run: nightly 39005291404 (schedule, 2026-10-17 05:10 UTC, head `d48c50216`).
- Job: 117316394504, `tier 2 nightly (pairwise cover)`, 05:10–07:27.
- Step: the fixture build. `build-test-fixtures` stops at the new post-build
  ledger check (`nros-cli-core/src/cmd/ws.rs:706`):

```
Error: 234 dependency drift(s) from the pin set (RFC-0104 D9):
  examples/zephyr/rust/action-client: `aho-corasick` 1.1.5, pinned at 1.1.4 — ...
error: recipe `build-test-fixtures` failed with exit code 1
```

Every drift is in one of the six Zephyr Rust example leaves: 39 each for
`action-client`, `action-server`, `listener`, `service-client` and
`service-server`, and 38 for `talker`. No other leaf drifts. The drifts run in
both directions. Most crates are NEWER than the pin (`serde` 1.0.229 against
1.0.228, `syn` 3.0.9 against 3.0.6, `zerocopy` 0.8.63 against 0.8.50), but
`psm` is OLDER (0.1.24 against 0.1.28). The same three crates also appear
outside any pinned range: `core_detect`, `multiversion_no_op` and `simdutf8`.
#1937's own message says those three are pulled in only by the unpinned
`encoding_rs` 0.8.42. That is what these leaves resolve, against a pin of
0.8.35.

## Why this is not a one-line fix

#1937 added `nros_seed_leaf_lock "$src_dir"` to
`scripts/build/zephyr-fixture-run-one.sh`, run before `west build` for every
Rust leaf that is not a retargeted row. This run's head includes that change.
The lock the ledger check then reads still resolves off the pin set. Several
explanations fit, and each points at a different fix:

1. The seed never ran for these six leaves. That is either the `nros_image`
   guard, or these rows build through a path that does not call
   `zephyr-fixture-run-one.sh`. The job log shows no seed line for any Zephyr
   leaf, although the seed may not print one.
2. The seed ran, but zephyr-lang-rust's plain `cargo` re-resolved after it. It
   may have used a lock in a different place: a build-dir copy, or a workspace
   root above the leaf. Or it may have had no `--locked`, so a dependency
   missing from the seed was resolved fresh.
3. The seed realigned the lock, but `west build` never rebuilt the leaf
   (a persistent `build-*` dir is up to date). The lock the check reads is then
   the persistent tree's old one. The mixed newer/older versions fit a lock
   from an earlier resolve better than one fresh one.

## What it is NOT

- Not tracked-lock drift. `examples/zephyr/rust/*/Cargo.lock` is untracked on
  `origin/main`, as CLAUDE.md requires for leaves with message dependencies,
  and `check-cargo-registry-pins` is green.
- Not a pin-set bug. Every other leaf in the ledger checks clean in the same
  run, and #1937 measured the native async leaf going from 7 drifts to 0.
- Not 1158. Tier 2 got through provisioning and nearly the whole fixture
  build. Only the stamp, which the ledger check guards, was not written.

## What would close it

A tier-2 nightly whose fixture build passes the ledger check, with the six
Zephyr Rust leaves checked rather than exempted. The job log should show that
each of the six leaves was seeded, and which lock file the check then read.
Before changing code, find which of 1–3 above is true. Run
`nros ws check-lock` on one of the leaves on the runner's persistent tree,
before and after a single `zephyr-fixture-run-one.sh` row, and compare the
lock's mtime and content with the seed's output.

## Second night: clean on another runner (2026-10-17)

The next tier-2 nightly passed the same check:

- Run: nightly 39012587878 (07:09 UTC, head `d06f951f3`).
- Job: 117338072093, on `nano-ros-runner-newslab-133`.
- Result: `check-lock: 112 lock(s), 0 drift(s)`.

The failing run's job (117316394504) ran on `nano-ros-runner-newslab-server-241`.
So the six drifted locks belong to one runner's persistent fixture tree, not to
the code on `main`. Both heads carry #1937.

That points to cause 3, or to cause 1 on that runner's tree only. A
`west build` that is up to date never rebuilds the leaf, so 241's older locks
were never realigned. The next nightly that lands on 241 shows whether the
drift persists there. The check on 241's tree described above settles it.

## Third sighting: newslab-server-241 again (2026-10-17)

- Run: run-matrix 39009801514 (schedule, head `86394e250`).
- Job: 117366348665, `tier 1 (cells)`, on `nano-ros-runner-newslab-server-241`.
- Result: `check-lock: 57 lock(s), 234 drift(s)`, then `build-test-fixtures` failed. That job's
  verdict job reads `tier 1 — NO VERDICT: stopped in the build`.

These are the same 234 drifts in the same six Zephyr Rust leaves, and the
crates and versions match this issue's first run. In the same run, tier 2 ran
on another runner and passed (`VERDICT: cells ran and passed`).

So far it is 2 of 2 on 241 and 0 of 1 elsewhere. The drift follows that runner's
persistent `nros-fixture-tree`. Until it is fixed, every lane that lands on 241
and builds fixtures stops before its cells, which makes that lane a
no-verdict lane on that runner.

## Cause (measured on 241, 2026-10-18)

Cause 1 above, in a form none of the three named: the seed DID run, and its
realign could move nothing. Read-only on 241's tree (head `86394e250`):

- Each of the six `build-rust-*-zenoh.log` files of the failing build prints
  `seed-lock: 39 entr(ies) of …/<leaf>/Cargo.lock stay off the pin set`.
  No log prints a `realigned N crate(s)` line, so not one move succeeded.
- `cargo locate-project --workspace` names the leaf itself, so the seed
  reached the right lock. The talker lock is rewritten on every build (mtime
  05:42Z, one minute before the build's head), because zephyr-lang-rust
  always runs cargo.

`seed-lock` moves an existing lock with one `cargo update -p X@have
--precise want` per crate, run in the leaf ALONE. A Zephyr Rust leaf
registry-names `zephyr` and `zephyr-build`, and only the west build's patch
table (from the store's `modules/lang/rust`) supplies them. So every move
fails with `no matching package named zephyr-build`, and
`enforce_cargo_root` discarded that error. The lock that 241's persistent
tree had resolved before #1937, unseeded, therefore stayed. 133 passes
because its leaves had no lock: `seed` copied the pin set in and never had
to realign. That makes it a defect of any persistent tree that predates
#1937, not of one runner.

## Fix

- `drift::enforce_cargo_root` prints the moves it could not make, with
  cargo's first error line, for both `seed-lock` and `nros build`.
- `seed-lock` in a checkout calls `drift::realign_or_reseed`. Drift the
  realign leaves is replaced by a fresh seed from the pin set
  (`registry_pins::write_seed`). In a checkout a leaf lock is seeded build
  output, so this is what a leaf with no lock gets, and the build then
  resolves from the pins.
- `nros build` keeps refusing leftover drift as before. That refusal was
  already loud, and it now names cargo's reason.

What closes this issue is unchanged: a tier-2 nightly on 241 that passes
`check-lock` with the six leaves checked.

## Fourth sighting: other leaves, on newslab-118 (2026-10-27)

The same gate stopped the tier-2 nightly's fixture build again, after the Fix
above landed, and this time **no Zephyr leaf is in the list**.

- Run: nightly 40453933125 (schedule, 2026-10-27 05:11 UTC, head `5b93da7e2`).
- Job: 122022748220, `tier 2 nightly (pairwise cover)`, runner
  `nano-ros-runner-newslab-118-server`.
- Step: the fixture build, at the post-build ledger check
  (`nros-cli-core/src/cmd/ws.rs:786`):

```
check-lock: 114 lock(s), 28 drift(s)
Error: 28 dependency drift(s) from the pin set (RFC-0104 D9):
  examples/mps2-an385-freertos/rust/talker: `libc` 0.2.191, pinned at 0.2.186 — ...
  packages/testing/nros-tests/bins/contract-monitor: `serde_json` 1.0.154, pinned at 1.0.150 — ...
error: recipe `build-test-fixtures` failed with exit code 1
```

The 28 are two crates only, each one patch-level NEWER than its pin: `libc`
0.2.191 against 0.2.186, in the six `mps2-an385-freertos` and six
`threadx-linux` Rust examples, `nros-bench/loan-copy-count`,
`condvar-contract-freertos-mps2`, `declarative-safety-listener` and
`pool-exhaustion-threadx-linux`; and `serde_json` 1.0.154 against 1.0.150, in
two `nros-bench` leaves and ten `nros-tests/bins` leaves. None of these locks
is tracked on main. They are seeded build output, so the drift was resolved on
the runner, whose fixture tree persists between runs. This is the shape the
Fix's `realign_or_reseed` was meant to absorb, so either these leaves do not
reach `seed-lock`, or the reseed resolves past the pins for them.

The previous nights' tier-2 nightly builds failed earlier, at issue 2025's
Zephyr link inside the same fixture build, so they may never have reached this
post-build check. Whether this drift is new, or was hidden behind 2025, is NOT
established.

## Resolved, 2026-10-31

The acceptance above asked for a nightly on newslab-server-241. That host was
never the variable. Every runner runs the same container image, and 241 failed
only because its persistent fixture tree still held leaf locks resolved before
#1937. What has to be shown is that the fixture lane brings such a lock back
onto the pin set, on any host. That is now shown in two ways.

**The fault, reproduced and cleared locally** (head `de75865960`):

1. `examples/zephyr/rust/talker/Cargo.lock` was moved off the pin set by
   hand (`serde` 1.0.228 to 1.0.229, the first drift this issue listed).
   `nros ws check-lock` on the leaf reported `1 lock(s), 1 drift(s)`.
2. The fixture lane built that one leaf:
   `NROS_ZEPHYR_FIXTURE_FILTER='build-rust-talker-zenoh$' just zephyr build-fixtures`,
   with `NROS_LEAF_LOCK_LEDGER` set. The build passed and relinked
   `build-rust-talker-zenoh/zephyr/zephyr.exe`, and the ledger named the leaf.
3. `nros ws check-lock --ledger <ledger>` reported `1 lock(s), 0 drift(s)`.
   The lock was byte-identical to the one it had before step 1, so the west
   build's own `cargo` run kept what the seed wrote.

**The nightly ledger check is clean.** Tier-2 nightly 41018247377
(2026-10-30, newslab-118) built the six Zephyr Rust leaves and printed
`check-lock: 115 lock(s), 0 drift(s)`, as did both nightlies of 2026-10-28.
The fourth sighting's 28 non-Zephyr drifts (`libc`, `serde_json`) have not
come back in any of those runs.

The unit test
`drift::tests::a_leaf_cargo_cannot_resolve_alone_is_reseeded_instead_of_left_drifting`
pins the cause measured on 241: a leaf that cargo cannot resolve alone is
reseeded rather than left drifting.
