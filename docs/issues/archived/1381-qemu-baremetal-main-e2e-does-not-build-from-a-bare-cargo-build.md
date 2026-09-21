---
id: 1381
title: "`qemu-baremetal-main-e2e` cannot be built with `cargo build` after
  `nros sync` — `nros::main!()` emits a `std` path into a `#![no_std]` leaf"
status: resolved
type: bug
area: [build, codegen, testing]
severity: medium
found: 2026-09-17
related: [issue-1061, issue-0100, issue-1409]
resolved_in: "fix(#1381): the entry macro emits `std` for a board that has none, and the leaf could not name its own target"
---

## What happens

In a clean worktree with the CLI built:

```
cd packages/testing/nros-tests/bins/qemu-baremetal-main-e2e
nros sync
cargo build --release
```

```
error[E0433]: cannot find `std` in the crate root
  --> src/main.rs:14:1
error[E0433]: cannot find `std` in the crate root
  --> src/main.rs:14:1
error: could not compile `qemu-baremetal-main-e2e` (bin) due to 2 previous errors
```

Line 14 is `nros::main!(panic = "own");` in a leaf whose first two attributes
are `#![no_std]` / `#![no_main]`. So the expansion names `std::` twice on a
target that has none — CLAUDE.md's "`std` is being DELETED; write `core::`/
`alloc::`, never `std::`" rule, in generated code.

## The precondition this probably rides on

`nros sync` in that leaf does not complete cleanly either:

```
sync: source metadata — no producer for
  qemu_baremetal_main_e2e::qemu_baremetal_e2e
  (deploy-bound probe failed: metadata-mode harness failed (exit 101) for
   component 'qemu_baremetal_e2e': error: invalid instruction mnemonic 'bkpt')
sync: 1 component(s) are un-probeable, so pool budgets stay at the crate
  defaults (issue 1061)
```

The metadata-mode harness builds the component for the HOST to probe it, and a
Cortex-M leaf's `bkpt` does not assemble there. Issue 1061 already covers the
degradation ("budgets stay at the crate defaults"). Whether the `std` paths are
a SECOND defect or a consequence of the un-probeable path taking a different
expansion branch is **not established** — both were observed together and only
once.

## Why it matters

This fixture is the canonical bare-metal `nros::main!()` BoardEntry E2E image,
and it is the natural stand-in whenever someone needs a real 32-bit Cortex-M
ELF that links both `nros-rmw-zenoh` and a board's rlsf arena. The phase-392
amendment B measurement wanted exactly that and had to fall back to a
`thumbv7m-none-eabi` object probe for the allocator half.

`just qemu build-fixtures` presumably supplies whatever the bare `cargo build`
is missing, so the lane is green and the leaf is unbuildable by hand — which is
the gap worth closing either way: a standalone copy-out fixture that only
builds under one recipe is not standalone.

## What to establish first

1. Does `just qemu build-fixtures` build it today, and what does it pass that a
   bare `cargo build` does not?
2. Expand the macro (`cargo expand`, or the entry-lower output) and name the
   two `std::` paths. If they are unconditional, it is a codegen bug
   independent of 1061.

## Resolution

**Answer to the open question: a SECOND defect. The probe is not load-bearing —
it is not even an input.** Three measurements, all on the same tree with the
same failed probe:

1. **Host vs target, probe held constant.** `cargo +nightly rustc --
   -Zunpretty=expanded` in the leaf: the HOST expansion carries 6 `std::` paths
   and fails; the same expansion through the settings file
   (`--config build/mps2-an385-baremetal/nros-cargo.toml`, i.e.
   `thumbv7m-none-eabi`) carries **0** and compiles clean, `rc=0`. The variable
   is the TARGET.
2. **Probe varied, target held constant.** Planting a `metadata/
   qemu_baremetal_e2e.json` sidecar (a successful probe's artifact), twice, with
   6 and with 40 slots, produced a **byte-identical** expansion both times —
   `diff` empty against the un-probeable run.
3. **Source.** The emissions sit in the `Framework::OwnedSpin` arm of `body_ts`
   with no metadata input anywhere in their construction.

So the two errors the issue reports are the whole of the codegen defect, and
1061's degradation was co-observed, not causal. It stays open and untouched.

### Defect 1 — the emitter asked the wrong question

All nine `std`-naming tokens (`std::env`, `std::time::{Instant,Duration}`,
`std::println!`, `std::eprintln!`, `std::process::exit`) were guarded by
`#[cfg(not(any(target_os = "none", target_os = "nuttx")))]`. That answers *is
the TARGET hosted*. The question is *does this CRATE have `std`*, and `#![no_std]`
is orthogonal to the target OS: a `#![no_std]` leaf built with no `--target`
compiles for the host, takes the hosted arm, and still has no `std` in its crate
root. There is no cfg predicate for the real question, so it cannot live in the
emitted code — it has to be a decision the EMITTER makes.

`nros_orchestration_ir::BOARD_PATHS` gained a third column, `links_std`, which
restates for the two Rust emitters what the board descriptor already says as
`entry_kind` (`hosted-main` ⟺ the entry links libstd; `board-run` /
`zephyr-staticlib` ⟺ `builder::entry` writes `#![no_std]` at the top of the TU).
A column rather than a second table, so a new board key cannot be added without
answering. `main_macro::hosted_std_scaffold_ts(links_std)` is now the ONE place
the macro writes a `std` path and returns an empty token stream otherwise;
`board_entry_links_std` returns `None` for an unknown key and callers read that
as "assume hosted", so an out-of-tree board keeps its `fn main()`.

No `core`/`alloc` spelling replaces those paths, and none should: a wall clock,
the process environment and an exit status are libstd surfaces. An entry that
cannot link libstd has no hosted spin to configure and no process to exit — the
fix is not to spell them differently, it is not to emit them.

### Defect 2 — the leaf could not name its own target

Fixing the expansion alone does NOT make the issue's command succeed, and
saying so is the point. A host build of this leaf can never work: after the
E0433s come `unwinding panics are not supported without std` (the leaf's
`[profile.release]` has no `panic = "abort"`) and then no `main` symbol to link.
The command was building for the host because phase-445 W4b moved the board's
triple into `build/<image>/nros-cargo.toml`, a file cargo reads only when a
`--config` flag names it — which `fixtures-build.sh` passes and a human does
not. That is the "only builds under one recipe" half of this issue, and it is
the whole reason the lane stayed green over a leaf nobody could compile.

`nros sync` now writes `include = ["../build/<image>/nros-cargo.toml"]` into the
leaf's own gitignored `.cargo/config.toml` — the file it already writes for the
central patch, and the file CLAUDE.md keeps for exactly this ("a leaf a plain
`cargo` or the metadata probe runs INSIDE"). It costs the lane nothing: the lane
runs cargo from the directory ABOVE the leaf, where that file is not on cargo's
discovery path, so phase-445 W6's doubled `rustflags` (`region 'FLASH' already
defined`) cannot come back through it. A leaf that states its own `[build]
target` or `[target.*] rustflags` is left on the `--config` road, because there
the doubling WOULD happen; zero of the 23 tracked `.cargo/config.toml` in the
tree sit beside a `system.toml`, so no in-tree leaf is in that shape.

### Acceptance

The issue's own command, verbatim, in
`packages/testing/nros-tests/bins/qemu-baremetal-main-e2e`:

```
nros sync            # rc=0
cargo build --release
#   Finished `release` profile [optimized + debuginfo] target(s) in 15.05s
# build/mps2-an385-baremetal/target/thumbv7m-none-eabi/release/qemu-baremetal-main-e2e:
#   ELF 32-bit LSB executable, ARM, EABI5 version 1, statically linked
```

Independently, the expansion forced to the HOST triple
(`cargo +nightly rustc --target x86_64-unknown-linux-gnu -- -Zunpretty=expanded`)
now yields **0** `std::` paths and **0** E0433 where it yielded 6 and 2.

`nros sync` still reports the component un-probeable — issue 1061, deliberately
untouched. Making this leaf probe-able would change what the image measures.

### Gate

`check-no-std-entry-emission` (`scripts/check-no-std-entry-emission.py`, on the
derived fast lane, buildless, 10 self-tests). It is a deliberate MIRROR of
`check-no-std-stdio` rather than an extension: that gate scans a crate's own
`src/` and says in its own docstring that a proc macro's emitted
`::std::println!` "is that crate's business, checked there" — 1381 is the hole
in *there*, because the caller's source is one line. This one scans the
PRODUCER, counts only text inside a `quote!` block (so the macro's own
`std::fs` / `std::env` host-side reads are out of scope), and allows a `std`
path only inside `hosted_std_scaffold_ts`.

A real-but-open site goes in `KNOWN_OPEN` with a tracked issue id and nowhere
else; a `KNOWN_OPEN` row whose file has no finding any more FAILS the gate, so
an exemption cannot outlive its defect.

### Sweep

```
grep -rn --include='*.rs' --include='*.jinja' -E '(^|[^A-Za-z0-9_:])(::)?std::' \
    packages/core/nros-macros/src \
    packages/cli/nros-cli-core/src/codegen/entry \
    packages/cli/rosidl-codegen/packs
```

Three emitters, three verdicts:

* `nros-macros/src/main_macro.rs` — 9 sites, all fixed, all now inside the one
  gated function.
* `codegen/entry/packs/entry/rust/entry.rs.jinja:57-58` — the SAME two lines
  (`::std::eprintln!` + `::std::process::exit`) with an even weaker guard
  (`#[cfg(not(target_os = "none"))]`, which does not exclude NuttX). That tree
  was being rewritten by concurrent work (issue 0794), so it is filed as **issue
  1409** and listed in the gate's `KNOWN_OPEN`, not silently edited.
* `rosidl-codegen/packs` — the `std::` uses are in the hosted ros2_rust-compatible
  packs (`rust/`, `rmw/`, `scaffold/lib.rs.jinja`). The `#![no_std]` pack
  (`nros/`, `scaffold/lib_nros.rs.jinja`) has **zero**, and the committed
  generated message crates under `packages/interfaces/*` have zero. Not this
  class.

### Tests

* `nros-macros`: `only_a_board_whose_entry_links_std_gets_the_std_scaffold`
  asserts on the TOKEN STREAM per board key, over both arms (a refactor that
  keeps the flag and emits the tokens anyway fails it);
  `an_unknown_board_key_is_assumed_hosted` pins the out-of-tree fallback.
* `nros-cli-core/tests/board_key_table.rs`:
  `the_links_std_column_agrees_with_the_descriptors_entry_kind` checks the new
  column against the shipped descriptors, so the restatement cannot drift.
  Mutation-tested: flipping `qemu-mps2-an385` to `true` fails it.
* `nros-cli-core` `cmd::leaf_settings`: five tests for the `include` — written,
  idempotent WITHOUT touching the mtime, existing entries preserved, and the
  authored-build-flags refusal in both its shapes.
