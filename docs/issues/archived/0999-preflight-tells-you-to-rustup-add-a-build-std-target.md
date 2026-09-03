---
id: 999
title: "`nros build` preflight asks rustup about a `build-std` target, so every
  nuttx build fails with a remedy that cannot work"
status: resolved
type: bug
area: cli, build
severity: high
found: 2026-09-03
related: [issue-0968, issue-0998, issue-0833]
---

## Symptom

`just build-test-fixtures lane=tier2`, the `nuttx` module, building a workspace
fixture:

```
  -> workspace-c-nuttx (c) examples/workspaces/c
     nros build demo_bringup:nuttx --workspace . --offline -- ...
Error: missing prerequisites for this build:
  - Rust target `armv7a-nuttx-eabihf` (board `nuttx`)
      run: rustup target add armv7a-nuttx-eabihf
```

The remedy cannot work. `armv7a-nuttx-eabihf` is not a distributed target:

```
$ rustup target list | grep -c armv7a-nuttx
0
```

## Cause

`builder/preflight.rs` asked one question — "does `rustup target list
--installed` name this triple?" — and printed one remedy for every no.

But the tree already distinguishes two kinds of target, and says so twice:

* `config/rust-targets.txt:43` — `armv7a-nuttx-eabihf   build-std`
* `scripts/lib/rust-targets.sh:10` — of that column, in as many words:
  *"Tier 3 / custom-JSON targets, nothing to install"*

A `build-std` target is compiled from source with `-Z build-std`. rustc does not
ship it, so `rustup target list --installed` can NEVER name it and `rustup
target add` can NEVER install it. The check therefore reported it missing on
every host, including a fully provisioned one, and sent the reader to a command
that fails.

**This is issue 0833's class**: a second idea of what the target list means,
held somewhere that does not read the list. It is exactly why the target set is
DATA in `config/rust-targets.txt` and why `just/workspace.just` carries the
comment "read from config/rust-targets.txt, NOT a second copy of the list".

## Fix

The board descriptor already carries the distinction and preflight already holds
the descriptor — it just was not asked:

```rust
/// Rust toolchain a generated package pins.
pub enum Toolchain {
    Stable,   // prebuilt target — rustup both knows it and can install it
    Nightly,  // pinned nightly + `rust-src` for `-Z build-std`
    Esp,      // xtensa espup toolchain
}
```

So the check now branches on it:

* `Stable` — unchanged; ask rustup, and `rustup target add` is the remedy.
* `Nightly` — the prerequisite is the SOURCE, not a distributed target. Probe
  the `rust-src` component; remedy `rustup component add rust-src`.
* `Esp` — skipped. The espup toolchain ships its own rustc and its own target,
  so a rustup query about either answers about the wrong toolchain.

Reading the descriptor rather than parsing `config/rust-targets.txt` keeps this
from becoming a THIRD reader of the target list, which is the defect it is
fixing.

## Verified

`a_build_std_board_is_not_told_to_rustup_target_add` — a `toolchain = "nightly"`
board pinning the real `armv7a-nuttx-eabihf`, asserting no
`rustup target add` remedy is produced, and that any `rust-src` finding carries
the remedy that works.

Proven non-vacuous: restoring the old check fails exactly that test
(`preflight.rs:223`) and leaves the other six passing.

## How it was found

Trying to reproduce issue 0968 (twelve tier-2 e2e failures, unreproduced). The
lane cannot build its nuttx fixtures at all, so nothing downstream of them has
run — the same shape as 0998, found in the same sweep: a lane nobody runs
accumulates blockers, and the blockers are invisible because the lane is red for
some earlier reason every time anyone looks.

## Acceptance

* [x] A `build-std` board is never told to `rustup target add`.
* [x] The remedy for a build-std board is one that works.
* [ ] `just build-test-fixtures lane=tier2` reaches the nuttx workspace rows —
      unverified here, because the nuttx module has other prerequisites this
      host may not satisfy; the next full run is the measurement.
