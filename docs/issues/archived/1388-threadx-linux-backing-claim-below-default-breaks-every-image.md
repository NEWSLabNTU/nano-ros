---
id: 1388
title: "Every ThreadX-Linux image fails to COMPILE on main — the board's
  `backing_u64s = 4494` is below the default executor sizing, and the gate that
  exists to catch exactly this reads Zephyr confs only"
status: resolved
resolved: 2026-09-20
type: bug
area: [core, boards, build]
severity: high
found: 2026-09-18
related: [1145, 1171, 1284, 0196, 1362, phase-392, phase-448]
---

## What happens

`just threadx_linux build-examples`, in a worktree at `origin/main`
(ac65af117) plus one commit that touches only a gate script and two docs, with
the session's own unrelated edits stashed so nothing under `packages/` differed
from main:

```
error[E0080]: evaluation panicked: NROS_EXECUTOR_BACKING_U64S is below the
default executor sizing, so the reservation can never be taken and is pure dead
weight; use 0 to decline the static entirely
error: could not compile `nros-node` (lib) due to 1 previous error
```

It is not only the Rust lane. `nros-cpp`'s size probe runs a nested cargo build
and dies on the same panic, so the C and C++ images fail too:

```
nros-cpp: size probe could not locate the `nros` rlib: cargo metadata failed:
  nested cargo build failed … error[E0080]: evaluation panicked:
  NROS_EXECUTOR_BACKING_U64S is below the default executor sizing …
```

The assertion is `packages/core/nros-node/src/executor/backing.rs:155`:

```rust
#[cfg(nros_executor_backing_static)]
const _: () = assert!(EXECUTOR_BACKING_U64S >= EXECUTOR_BACKING_DEFAULT_U64S, …);
```

## The claim that trips it

`packages/boards/nros-board-threadx-linux/nros-board.toml`:

```toml
[board.knobs.executor]
backing_u64s = 4494
```

with a comment recording how it was chosen on 2026-09-12 — per-role backing
measured with `nm -S` on x86_64, the heaviest role (`service-client`) at
35,952 B, and `4494 = 35,952 / 8`.

**MEASURED, this session:** setting `backing_u64s = 0` — declining the static —
makes the whole lane build clean (`rc=0`, "ThreadX Linux examples built!").
Restoring 4494 reproduces the failure. So the claim is the sole cause.

For scale, the C workspace's own generated header for this same board reads:

```
#define EXECUTOR_OPAQUE_U64S 11301
#define NROS_EXECUTOR_SIZE   90408
```

and the Zephyr confs claim 11069 on `native_sim/native/64`. 4494 is less than
half of either.

## Why nothing caught it

Two gaps, and it needed both.

1. **No merge-gating lane builds ThreadX.** `ci gate` is compile+unit with no
   fixtures; the PR lane is `check-fast` + smoke + cli-tests + workspace-all.
   A ThreadX image is built by `build-examples` / the nightly, so a break here
   is invisible to every required check.

2. **`check-executor-backing-arena-pairing` does not read this claim.**
   Its `--claims` output is Zephyr `.conf` files only — 13 rows, all
   `examples/zephyr/rust/*`. A board TOML's `[board.knobs.executor]
   backing_u64s` is not in its population at all, so the one gate whose subject
   IS this pairing has never looked at the ThreadX boards.

   That is issue 0196's shape, and it is the second gate in two days found with
   a reach narrower than the rule it enforces (issue 1362 was the first).

## What is NOT established

**Which fix is right.** Two candidates, and they differ in what the pairing was
for:

* **Re-measure and state the true number.** But the assertion compares against
  `ExecutorSizing::DEFAULT.u64_len()` — the DEFAULT sizing, not what any role
  actually uses. If that is ~11301 words, the board must reserve ~90 KB of
  `.bss` to use the static at all, which is 2.5x the heaviest role's measured
  35,952 B. The pairing subtracts the same number from the byte pool, so the
  image would give back 90 KB of pool for 90 KB of `.bss` — still a MOVE, but a
  much larger one than the comment describes.
* **Decline the static (`backing_u64s = 0`).** Builds today, restores the heap
  arm, and loses what phase-392 W6 bought: `mem-report` reads symbols, and a
  `Box::leak` has none.

The per-role measurement in the board comment answers "how much does this
executor need", and the assertion asks "does the claim cover the DEFAULT
sizing". Those are different questions, and the comment reads as though it
answered the second. Whether the default grew past 4494 after 2026-09-12 or
was already above it has NOT been established — `git log` on the sizing inputs
would say, and this issue does not claim it.

This is the hazard CLAUDE.md already records for this exact knob — *"The
subtraction is STATED, never measured … a subtrahend copied from `nm` drifts on
the next knob move"* — reaching its documented failure mode.

## Acceptance

1. `just threadx_linux build-examples` is green with the static NOT declined,
   or with a recorded decision that declining is correct for this board.
2. `check-executor-backing-arena-pairing` reads board-TOML `backing_u64s`
   claims as well as Zephyr confs, and would have failed on this one. Land that
   coverage WITH the fix: extending it first turns the fast line red for
   everyone while the defect is still open.
3. Same question asked of `nros-board-threadx-qemu-riscv64`, whose backing
   number issue 1145 records as still unstated.

### What the reproduction does and does not cover

It was NOT run from a pristine clone. The worktree was provisioned during this
session — `threadx/kernel` and `threadx/netxduo` initialised, `play_launch`
initialised, the CLI and `nros-launch-resolve` rebuilt — and the assertion first
fired only after the CLI rebuild, which re-derives the knobs and forces
`nros-node` to recompile. Earlier builds in the same worktree failed on other
things and may have been reading a cached `nros-node`.

So what is established is: with `packages/` identical to main and the knobs
freshly derived, the claim fails the assertion, and setting it to 0 is the only
change needed to make the lane build. What is not established is whether a
CI-shaped run from a clean checkout hits it on the first build or only after
something invalidates `nros-node`. Either way the claim and the default
disagree, which is the defect.

Found while trying to measure the ThreadX byte pool's undeclared 4 MiB base
(issue 1145's last open item) — which cannot be measured on an image that does
not build.


## RESOLVED 2026-09-20 — and the root cause is not what this issue guessed

`backing_u64s` 4494 -> **11069**, plus the gate coverage that makes it checkable.
`just threadx_linux build-examples` is green, rc=0, zero assertion failures.

### It was never drift

This issue leaned toward "the default grew past 4494 after 2026-09-12". It did
not. 4494 is a TRUE measurement of the WRONG POPULATION.

`nros sync` narrows each LEAF's executor knobs, so the derived default differs
per role — which is what the board comment measured with `nm -S`, heaviest role
35,952 B = 4494 words. But the rung is stated ONCE PER BOARD, and not every unit
compiled with it gets a leaf's narrowing. Measured in ONE build of this board,
`nros-node` resolves four different defaults:

```
2917   3626   4494   11069
```

The last is the synthesised `nros_ws_runtime` umbrella, compiled per CARGO ROOT
rather than per image, which therefore takes the unnarrowed
`ExecutorSizing::DEFAULT` — the same number the Zephyr confs state. The const
assertion is `stated >= default` in EVERY unit, so the claim has to cover the
largest, not the heaviest role.

The cost is real and is the price of one statement per board: a talker reserves
11,069 words where its own executor needs 2,917. It is not a memory cost — this
rung MOVES bytes between `.bss` and the byte pool, which gives back the same
number — so over-stating costs `mem-report` accuracy, not memory.

### Correction: "the gate never looked" was too strong

`check-executor-backing-arena-pairing` DOES have a ThreadX arm,
`check_threadx_site`, and it checks the MECHANISM in both directions: that the C
names a base, that the pool mentions the knob, and that the build-script
forwarder forwards it. What it did not check is issue 1284's other half — is the
stated number ENOUGH — which was built for the Zephyr CONF spelling and never
extended to the board-descriptor spelling. The claim was unvouched-for, not
unseen.

Fixed: `board_toml_claims()` emits `[board.knobs.executor] backing_u64s` as a
claim, `claim_half` merges it, and `executor_backing_claims.rs` compares it
against the measured default in `just check node-std-tests` (pull_request AND
merge_group). A claiming descriptor with no `BOARD_TOML_TARGETS` entry is
REFUSED rather than assumed, the same discipline `BOARD_TARGETS` already has.

Mutation tested — restoring 4494:

```
packages/boards/nros-board-threadx-linux/nros-board.toml states
[board.knobs.executor] backing_u64s = 4494 for `threadx`, but the executor's
default measured on this 64-bit host is 11069 words (6575 short). The image will
not compile. Restate it as 11069; the pool subtracts the same rung, so there is
nothing else to edit.
```

The remedy is per spelling: a Zephyr conf is told to re-pair its arena, a board
descriptor is not, because on ThreadX the subtraction is done by the C
preprocessor from this same rung and there is no arena to re-pair. Naming one
would send the reader to a file this port does not have.

### A second finding, measured and deliberately NOT fixed here

The `nros-c`/`nros-cpp` SIZE PROBE spawns a nested cargo build, and that build
inherits `NROS_BOARD_TOML` — so `env_opt_usize_laddered` picks the backing claim
off the BOARD rung — while the image's sizing knobs, which arrive through the
process env, do not reach it. Measured: 56 `NROS_*` vars in that build script's
environment, none of them a sizing knob. So the probe judged the claim against
an unnarrowed default too.

Two rungs of one ladder arriving by different routes, only one surviving the
process boundary. Setting `NROS_EXECUTOR_BACKING_U64S=0` for the nested probe
(the documented opt-out; the static is a `.bss` reservation, not a term in
`ExecutorSizing`) fixes that half, and was written and measured.

It is NOT in this fix, because it is not load-bearing for it: with the claim at
11069 the build is green WITHOUT the probe change, verified by reverting it and
rebuilding. Shipping it here would claim a necessity it does not have. It stays
worth doing — it is what would let a board state a per-image-correct number
rather than the unnarrowed maximum — and wants its own issue and its own
acceptance — filed as issue 1390.
