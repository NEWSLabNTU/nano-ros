---
id: 1937
title: "Nothing holds an untracked leaf lock to the pin set: 13 of 165 fixture leaves resolve off it, and the keep list is taken on trust"
status: open
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
