---
id: 1409
title: "`nros codegen entry --lang rust` emits `::std::` into an entry the same
  tool writes `#![no_std]` on top of"
status: open
type: bug
area: [codegen, build]
severity: medium
found: 2026-09-21
related: [issue-1381, issue-0794]
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
