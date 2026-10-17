---
id: 1941
title: "The six Zephyr Rust example leaves still drift from the pin set after #1937 seeds them, and the drift check stops the tier-2 nightly's fixture build"
status: open
type: bug
severity: medium
area: [build, ci, examples]
related: [issue-1937, issue-1924]
found: 2026-10-17
---

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
