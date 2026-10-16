---
id: 1937
title: "Nothing holds an untracked leaf lock to the pin set: 13 of 165 fixture leaves resolve off it, and the keep list is taken on trust"
status: resolved
type: bug
severity: medium
area: [build, ci, examples]
related: [issue-1924, issue-1873]
found: 2026-10-17
---

## The gap

`check-cargo-registry-pins` reads the 24 locked roots, the projects whose
`Cargo.lock` is tracked. An example that depends on generated message crates
has an untracked lock (CLAUDE.md, "never commit their `Cargo.lock`"), so
nothing the gate reads says what that example resolves. That leaves two blind
spots:

1. **A missing pin goes unseen.** An example that starts using a crate no root
   holds keeps the gate green. The fixture builds run a bare `cargo build`,
   which no drift check reaches, so CI stays green too. The first to notice is
   a user's `nros build` on a fresh checkout. That is how issue 1924 shipped.
2. **A stale keep entry goes unseen.** `locks/leaf-only-pins.toml` entries are
   accepted on their hand-written `why`. Nothing checks that any leaf still
   resolves them.

## Measured (2026-10-17)

The self-hosted runner's persistent fixture tree,
`~/nros-runner/work/nano-ros/nros-fixture-tree` at `c43a9031`, was compared
against main's pin set. Every untracked `examples/**/Cargo.lock` was checked
(script: compare each registry entry's `(name, semver range)` with the pinned
version).

**165 leaf locks, 13 with drift:**

| Leaves | No pinned range | Off-pin |
| --- | --- | --- |
| `zephyr/rust/{talker,listener,action-*,service-*}` (6) | 9 each: `bindgen`, `cexpr`, `clang-sys`, `darling*`, `annotate-snippets`, `arrayvec`, `rustc-hash` | 36 each |
| `px4/rust/companion/{offboard-companion,px4-probe,px4-stub}` | 0 | 23 each |
| `mps2-an385-freertos/rust/talker` | 1 (`cortex-m-macros`) | 22 |
| `qemu-armv7a-nuttx/rust/talker`, `threadx-linux/rust/talker` | 0 | 21 each |
| `linux/rust/action-client-async` | 0 | 7 |

Each is a refusal `nros build` would print in that leaf.

**Of the 14 keep entries, 12 are resolved by some measured leaf.** Two are
resolved by none of the 165: `include_dir 0.7` and `include_dir_macros 0.7`.
Their `why` already says "UNVERIFIED".

## Three causes, not one

- **No leaf-side check at all.** `nros_seed_leaf_lock`
  (`scripts/build/cargo.sh`) seeds a lock from the pin set before a fixture
  build. Nothing compares the lock with the pin set after cargo has resolved
  it. That is the spot where blind spot 1 would have been caught.
- **A persistent tree never reseeds.** `nros ws seed-lock` writes a lock only
  when none exists (`an_existing_lock_is_never_reseeded`). The fixture tree
  above persists across runs. Its pins file was rewritten on 2026-10-16 at
  17:18Z and already holds the issue 1924 pins. Its
  `linux/rust/action-client-async/Cargo.lock` was written that day at 08:26Z
  and still resolves `futures-core` 0.3.34, against a pin of 0.3.32. A pin
  move never reaches an existing leaf lock, so on a persistent runner the pins
  do not govern the fixtures at all. How many of the 21 to 36 off-pin rows per
  leaf come from this, rather than from crates cargo had to move, was not
  separated here.
- **The Zephyr west lane does not seed.** No `seed-lock` call appears in
  any Zephyr file in the tree (`git grep`), so the six Zephyr Rust
  leaves resolve from scratch. That is where all nine unpinned `bindgen`
  crates come from. The keep list meanwhile pins `nom 7` "for bindgen's cexpr
  in the Zephyr Rust leaves": a dependency of a crate that is itself unpinned.

## Fix direction

All of it goes through one helper, never a per-site spelling. There are 14
`nros_seed_leaf_lock` call sites across seven files.

1. **Realign before the build.** `ws seed-lock` (or `nros_seed_leaf_lock`)
   runs the drift check's realign (`drift::enforce_cargo_root`, now a fixed
   point after issue 1924) on a lock that already exists, as `nros build`
   does. That closes the persistent-tree cause.
2. **Check after the build.** A new verb, `nros ws check-lock <dir>...`, runs
   `drift::check_cargo_root` on the resolved lock. The seed helper appends
   each seeded dir to a per-build ledger, and one call at the end of
   `build-test-fixtures` checks the whole ledger, so no builder needs a second
   edit. Drift fails the build, as `nros build` refuses. That closes blind
   spot 1, at tier 1, where fixtures are built.
3. **Seed the Zephyr west leaves** through the same helper. Then pin what they
   resolve (`nros update --crate bindgen ...` from a leaf), or record why not.
4. **Verify the keep list.** The same pass writes the union of
   `(name, range)` that the leaves resolved into a build artifact. A gate
   outside the fast line compares every keep entry with it. A ledger from a
   narrow lane cannot prove an entry unused, so the gate only reports
   NOT VERIFIED there (the `nros_check_skip` ledger), and fails only against
   a `lane=all` ledger.

Order matters. Step 2 on today's tree would turn tier 1 red on 13 leaves, so
steps 1 and 3 go first, and the remaining drift is pinned with `nros update`
before step 2 is made fatal.

## Resolution

Steps 1 to 4 landed together. There was no red tier-1 run in between, because
the pins and the keep list were corrected in the same change.

1. **Existing locks are realigned before the build.** `nros ws seed-lock`
   now runs the drift check's realignment (`drift::enforce_cargo_root`) on a
   lock that already exists, in a checkout, just as `nros build` does. What it
   cannot move is printed and left for the post-build check. Measured: the
   runner's stale `linux/rust/action-client-async` lock went from 7 drifts to 0
   (`realigned 7 crate(s)`).
2. **The build checks the locks after cargo resolves them.** Each call to
   `nros_seed_leaf_lock` appends its root to `$NROS_LEAF_LOCK_LEDGER`. The
   variable is set (and the file truncated) by `build-test-fixtures-leaves`,
   whose per-platform builds all run as child processes. Then
   `build-test-fixtures` runs the new `nros ws check-lock --ledger` once,
   before it writes the fixture stamp, so drift fails the fixture build.
3. **The Zephyr lane seeds.** `zephyr-fixture-run-one.sh` calls
   `nros_seed_leaf_lock` on a Rust leaf before `west build`. A retargeted row
   is skipped, because it goes through `nros build`, which checks its own
   roots. A fresh resolve of `zephyr/rust/talker`, done with the store's
   `modules/lang/rust` crates patched in, showed exactly the 9 unpinned crates
   the runner measured. They were pinned with `nros update --crate bindgen
   --precise 0.72.1` from that leaf, at the versions it resolved, and are named
   in `locks/leaf-only-pins.toml`.
4. **The keep list is checked.** On `lane=all`, the build calls `check-lock`
   with `--complete`, which also fails on a keep entry that none of the
   checked leaves resolves.

Working the keep list against fresh resolves corrected it in two places:

- `include_dir` and `include_dir_macros`: no Cargo manifest in the tree names
  them, and none of the 165 measured locks resolves them. Retired.
- `core_detect`, `multiversion_no_op` and `simdutf8` were kept for "encoding_rs
  in the embedded Rust leaves". They are dependencies of `encoding_rs` 0.8.42,
  which is unpinned. The pin is 0.8.35, which needs none of them. The leaves
  that resolved them were exactly the drifted ones (`cortex-m-macros` in the
  FreeRTOS talker was the same stale artifact). Retired.

Measured on four fresh leaves: `linux/rust/action-client-async`,
`linux/rust/talker`, `zephyr/rust/talker` and `mps2-an385-freertos/rust/talker`.
`check-lock --complete` reports 0 drifts and 0 unused keep entries.
`check-cargo-registry-pins` reports 501 pins, one per range, and no tracked
root lock moved.

**In a real fixture build.** `just linux build-fixture-rust` was run with the
ledger set, then `check-lock --ledger` was run on the result. It built 51 leaves
and wrote 51 ledger entries, and `check-lock` reported 0 drifts. It also showed
that step 1 matters more than the persistent-tree case suggested. The build's
codegen step resolves each leaf's lock BEFORE `nros_seed_leaf_lock` runs, so
on main the seed found a lock already present and did nothing, even in a fresh
checkout. 20 of the 51 Linux leaves came in 7 to 23 crates off the pins, and
`seed-lock` now realigns them before cargo builds. The full
`build-test-fixtures` recipe was not run here: its preflight requires 4
`[source.*]` trees this host's store lacks, including px4-autopilot.

**What this does not cover:**

- `check-lock` reads a lock as it stands. A leaf whose cargo never resolved
  still holds the seeded copy of the whole pin set, and that copy satisfies
  `--complete` trivially. A failed build stops the fixture build before the
  check, so this cannot happen there, but it can happen to a hand-run
  `check-lock`.
- The PX4, NuttX and ThreadX leaves were not resolved here. Their runner drift
  was off-pin only, which is what step 1 realigns, and the next tier-1
  `lane=all` build is the first full measurement.
