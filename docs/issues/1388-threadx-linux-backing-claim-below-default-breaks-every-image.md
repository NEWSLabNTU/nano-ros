---
id: 1388
title: "Every ThreadX-Linux image fails to COMPILE on main — the board's
  `backing_u64s = 4494` is below the default executor sizing, and the gate that
  exists to catch exactly this reads Zephyr confs only"
status: open
type: bug
area: [core, boards, build]
severity: high
found: 2026-09-18
related: [1145, 1171, 0196, 1362, phase-392, phase-448]
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
