---
id: 1409
title: "`nros codegen entry --lang rust` emits `::std::` into an entry the same
  tool writes `#![no_std]` on top of"
status: resolved
type: bug
area: [codegen, build]
severity: medium
found: 2026-09-21
related: [issue-1381, issue-0794]
resolved_in: "fix(#1409): the CLI's Rust entry template emits `std` for a board that has none"
---

## What happens

`packages/cli/nros-cli-core/src/codegen/entry/packs/entry/rust/entry.rs.jinja`
ends with:

```jinja
#[cfg(not(target_os = "none"))]
fn main() {
    if let ::core::result::Result::Err(e) = __nros_entry_run() {
        ::std::eprintln!("{}: {}", ::core::env!("CARGO_PKG_NAME"), e);
        ::std::process::exit(1);
    }
}
```

The `#[cfg]` answers "is the TARGET hosted". The question the emitted code has
to survive is "does this CRATE have `std`", and the SAME tool answers that four
files away — `builder/entry.rs` writes `#![no_std]` at the top of the entry TU
for `EntryKind::BoardRun` and `#![no_std]` for `EntryKind::ZephyrStaticlib`, and
nothing for `EntryKind::HostedMain`.

So for a `board-run` board this template writes a `std::` path into a crate that
has declared `#![no_std]`. The guard does not save it: `no_std` is orthogonal to
the target OS, and a `#![no_std]` crate compiled for a hosted triple (which is
what a `cargo build` with no `--target` does) has no `std` in its crate root.
The `#[cfg]` is also narrower than the proc-macro's, which excludes
`target_os = "nuttx"` as well — NuttX entry leaves are `#![no_std]` since
phase-359 W7.

## Why this is filed rather than fixed

This is issue 1381 in the other Rust producer. 1381 fixed the canonical one
(the `nros::main!` proc-macro, which now emits every `std`-naming token from a
single `hosted_std_scaffold_ts` gated on
`nros_orchestration_ir::board_entry_links_std`) and added
`check-no-std-entry-emission` to keep it fixed. That gate lists this file as a
KNOWN-OPEN site carrying this issue id: the whole `codegen/entry/**` tree was
being rewritten by concurrent work (issue 0794) while 1381 landed, so editing
it would have collided.

## Not measured

Whether it BITES today. The CLI-baked entry is the mirror form that exists so
the two producers can be byte-diffed (issue 0302); which boards actually reach
it through `nros codegen entry --lang rust` — as opposed to through the
proc-macro — was not established here. Treat the severity as "a latent copy of a
defect that was not latent in the other producer", not as a reproduction.

## Fix shape

The same one 1381 took: ask
`nros_orchestration_ir::board_entry_links_std(board)` in `emit_rust.rs`, pass the
answer into the template as a flag, and wrap the hosted `fn main()` in it. Then
drop this file's row from `KNOWN_OPEN` in
`scripts/check-no-std-entry-emission.py` — the gate FAILS on a stale row, so the
exemption cannot outlive the defect.

## Resolution

### Answer to "Not measured": it does NOT bite today

And the reason is narrower than "nobody renders Rust entries".

Measured, three ways, all on this tree:

1. **The verb is gone.** `nros codegen entry --lang rust` does not reach this
   template: `run_entry`'s emit `match lang` has exactly one `Lang::Rust` arm
   and it is an unconditional `bail!` (`cmd/codegen.rs:428`, "`--lang rust
   entry is retired (phase-432 W2.4)`"). Stated as read rather than as run —
   reaching that line needs a valid `--model`, and producing one needs
   `nros-launch-resolve`, whose `play_launch` submodule is not checked out in
   the worktree this was measured in. What WAS run there confirms the binary
   under test is the right one (`nros --codegen-version` → 6 for this tree, 3
   for the parent checkout that `which nros` finds — the shadowing CLAUDE.md
   warns about, and the sole cause of this branch's one `check fast` red).
2. **The `#![no_std]` shell and this template never meet.** `builder/entry.rs`
   writes the shell, and the body it writes under it is `nros::main!(…)`, not
   this rendering — it delegates on purpose (its own doc comment: expanding
   here would freeze the node set). So the two halves of the defect are written
   by the same tool into two different files that are never concatenated.
3. **Every live caller of `emit_rust` is a test harness.** `git grep
   'emit_rust::'` over `packages/` returns exactly three lines: this file, the
   golden harness, and the parity harness. Both consume the output as TEXT;
   nothing compiles it.

So the issue's own framing was right — "a latent copy of a defect that was not
latent in the other producer". What was NOT latent is the artifact: the
template's output for a `board-run` board is unbuildable, and the committed
golden proved it. **1381's second defect does not have an analogue here**: the
"no `--target`" road it measured is a road into this template that no longer
exists. If the verb ever comes back, it comes back fixed.

### The bite, measured anyway

Taking the two halves the same tool writes and putting them together — the
`#![no_std]` / `#![no_main]` shell `builder/entry.rs` emits for
`EntryKind::BoardRun`, plus the committed parity golden for
`mps2-an385-freertos` (a `board-run` board) — and compiling for the HOST
triple, no `--target`, against stub crates so the only unresolved name is the
one under test:

```
$ rustc --edition 2024 --crate-type bin --emit=metadata -C panic=abort \
        --extern nros=… --extern nros_board_mps2_an385_freertos=… \
        --extern talker_pkg=… --extern sensor_driver_pkg=… before_board_run.rs
error[E0433]: cannot find `std` in the crate root
  --> before_board_run.rs:49:11
   |
49 |         ::std::eprintln!("{}: {}", ::core::env!("CARGO_PKG_NAME"), e);
   |           ^^^ could not find `std` in the list of imported crates

error[E0433]: cannot find `std` in the crate root
  --> before_board_run.rs:50:11
   |
50 |         ::std::process::exit(1);
   |           ^^^ could not find `std` in the list of imported crates

error: aborting due to 2 previous errors
```

The same two `E0433`s 1381 reported, from the other producer. After the fix,
the same command over the regenerated golden: **rc=0, no diagnostics.**

Regression direction, also measured: the `native` (`hosted-main`) golden still
carries its `fn main()` and still LINKS —
`ELF 64-bit LSB pie executable, x86-64`, one `T main`.

### The fix

`emit_rust::emit_lowered` reads
`nros_orchestration_ir::board_entry_links_std(&entry.board)` — the same call
`nros::main!` makes — and passes it to the template as `links_std`. The hosted
`fn main()` sits inside `{% if links_std %}` and is the template's only
`std`-naming text. No `core`/`alloc` respelling: an exit status is a libstd
surface, and an entry that cannot link libstd has no process to exit. That is
1381's reasoning unchanged.

`unwrap_or(true)` is present for symmetry and is UNREACHABLE here:
`board_path_for` two lines above refuses any key not in `BOARD_PATHS`, and
`board_entry_links_std` reads the same table. The "assume hosted for an
out-of-tree board" policy stays where it can fire — the proc-macro, which an
out-of-tree board reaches through `NROS_BOARD_FRAMEWORK`.

### Two things the fix had to take with it

**The `#[cfg]` was widened to the proc-macro's**,
`not(any(target_os = "none", target_os = "nuttx"))`. The issue named this; it
is belt-and-braces now (no `links_std = true` board is NuttX) and it is what
keeps the two producers diffable by eye.

**The `target_os = "nuttx"` C-ABI `main` was ADDED.** The template had two entry
arms where the macro has three, and gating the hosted one on `links_std` without
this would have traded an entry that does not COMPILE for one that does not
LINK — a worse failure, later and less legible. It is the macro's arm verbatim,
`(argc, argv)` signature included, for the reason phase-359 W7 gives: the NuttX
family is `no_std`, so libstd's `lang_start` is not there to wrap a Rust `main`.

### `EntryKind::ZephyrStaticlib` — checked, and it is a DIFFERENT defect

Both `zephyr` keys are `links_std = false`, so the fix covers them the same way.
But this template's output for a Zephyr board could never compile, for a reason
that is not 1409: it renders `<::nros_board_zephyr::ZephyrBoard as
BoardEntry>::run(…)`, and that crate implements `NetworkWait` / `BoardInit` /
`BoardPrint` / `BoardExit` and **zero** `BoardEntry` (measured: no `BoardEntry
for` in its `src/`). The macro routes a Zephyr board through
`Framework::Zephyr`, which emits a `rust_main` staticlib export and no
`BoardEntry::run` at all. That is the "strictly poorer than the macro's output"
fact that retired the verb, not a `std` path, so it is deliberately not touched
here. Nothing renders it, and the refusal to render an unknown board key does
not catch it because `zephyr` IS a known key.

### The narrower `#[cfg]` elsewhere — swept, not this class

```
git grep -n 'cfg(not(target_os = "none"))' -- packages/ examples/
```

Four sites outside this template, all asking about the TARGET's runtime
facilities rather than about the crate root, and none naming `std`:
`nros-rmw-cyclonedds/src/sync.rs` (×3, which `Mutex` backend) and
`rmw/cffi/src/section.rs` (does the loader fire `.init_array`). Whether the
second is right on NuttX is a separate question from this one and is left where
it is.

### Gate

This file's `KNOWN_OPEN` row is GONE and the table is now empty — which the
gate measures rather than tidies, since a row with no finding FAILS it.

`check-no-std-entry-emission` gained the template-side twin of
`HOSTED_ONLY_EMITTERS`: `HOSTED_ONLY_TEMPLATE_GUARDS`, one name (`links_std`),
matched EXACTLY. The rule it already stated — "a `std` path may appear only
where the producer has established that the entry links `std`" — did not
change; what changed is that the rule now has the second producer's vocabulary
in it, instead of only the proc-macro's. An `{% else %}` / `{% elif %}` CLEARS
the allowance, because the else-arm of `{% if links_std %}` is precisely where
this bug would live.

Self-tests: 10 → 16, the six new ones covering the guard, its else-arm, past
its `endif`, a guard by another name, a nested `if` inside it, and the
whitespace-control spelling. Negative control run by hand: deleting the two
jinja tags from the template takes the gate red naming both lines.

### Tests

`emit_rust`'s `only_a_board_whose_entry_links_std_gets_the_hosted_main` — the
mirror of `nros-macros`' `only_a_board_whose_entry_links_std_gets_the_std_scaffold`
— renders EVERY key in `BOARD_PATHS` and asserts
`src.contains("::std::") == links_std`, plus that both C-ABI arms survive on
every key (a board with no `main` at all is the hole this fix could have
opened). It also asserts both arms of the flag are exercised, so it cannot go
quiet. Mutation-tested: forcing `links_std` true fails it.

The parity gate is unaffected and still green — `entry_parity` compares the
per-node register block only, anchored between the first `runtime.` assignment
and the closure's `Ok(())`, which is why it never saw this divergence in the
first place.
