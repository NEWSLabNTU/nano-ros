---
id: 1217
title: "The root workspace `exclude` list is unaudited: one host-buildable crate
  is excluded for no reason and 36 of its 174 entries name directories that do
  not exist"
status: open
type: tech-debt
area: [build, ci, rmw]
related: [0895, 0948, 0894, 0386, phase-451]
---

## What

`Cargo.toml` has 57 `members` (`:4-182`) and 174 `exclude` entries (`:185-529`),
no globs in either. Every exclusion in the tree satisfies one of five structural
reasons — own `[workspace]` table, own tracked `Cargo.lock`, a
`.cargo/config.toml` pinning a non-host `[build] target`, a cross-only dependency
set (`cortex-m`, `esp-hal`, `rtic`, `stm32f4xx-hal`), or a prose comment saying
"metadata only, no Rust targets".

Two things fail that audit.

### A. `packages/rmw/transport-callbacks` is excluded for no discoverable reason

`Cargo.toml:379`. The line sits between two comments that belong to its
neighbours (`:377-378` explains `nros-smoltcp`, `:380-381` explains
`nros-baremetal-common`), so at a glance it inherits an explanation it does not
have.

The crate satisfies **none** of the five tests:

- no `[workspace]` table, no `Cargo.lock`, no `.cargo/config.toml`
- `packages/rmw/transport-callbacks/Cargo.toml` has exactly one dependency,
  `nros-rmw = { path = "../../core/nros-rmw" }` — itself a member
- no `[target.*]` deps, no `required-features`, no `[package.metadata]` note
- not `#![no_std]`, one `src/lib.rs`, 163 LOC

Measured: `cargo check` in that directory succeeds on the host in 1.03 s. It is
ordinary host-buildable Rust.

Consequences: it is invisible to `just check`'s workspace clippy (which runs
`-D warnings` over members only), and it violates the leaf-lockfile invariant
`check-leaf-lockfiles` enforces — an excluded leaf with no message deps should
carry a tracked lock, and it has none. So it is excluded from the workspace *and*
exempt from the rule that applies to excluded crates, which is the worst of both.

Its only consumers are the excluded standalone examples
`custom-transport-{talker,listener}` and an in-repo patch row named at
`packages/cli/nros-cli-core/src/cmd/ws.rs:4045,6299`.

**Membership is what is wrong here, not placement.** It reads as an omission
during the `packages/rmw/` consolidation.

### B. 36 exclude entries name directories that do not exist

`Cargo.toml:254-259, 300-305, 309-314, 324-329, 465-470, 473-478` — the `*-entry`
siblings for `threadx-linux`, `qemu-arm-freertos`, `qemu-arm-nuttx`, and the
`rust/zenoh/*` sub-trees for `qemu-arm-freertos`, `threadx-linux`,
`qemu-riscv64-threadx`. All 36 directories are absent from disk.

Harmless to cargo, which ignores an exclude that matches nothing. Not harmless to
a reader: they are 21 % of the list, and their presence is what makes the list
look exhaustive. A survey that counts entries — the obvious way to ask "is
everything accounted for?" — reads 174 and concludes the list is complete, when
the live count is 138.

## Why it matters together

These are one class: **nothing checks the exclude list against the tree.** A
crate can be excluded with no reason and no one notices; an entry can outlive its
directory and no one notices. Both were found only by enumerating every
`Cargo.toml` under `packages/` and `examples/` and diffing against the two
arrays, which is not something any lane does.

The adjacent known-open case is issue 0895: 19 example-workspace leaves under
`examples/workspaces/{rust,features,realtime-rust,safety,sizing}/src/*` are in
**neither** array, so on a fresh clone cargo's walk-up reaches the root
`Cargo.toml` and fails with "current package believes it's in a workspace when
it's not". That issue argues (correctly) that adding 19 exclude lines is the
wrong fix, and archived issue 0948 records that the gate for it was written and
withdrawn the same day as unsound. So the leaf half of this class is knowingly
ungated and separately tracked — **this issue is about the other half**, the
entries that *are* in the list.

## Fix sketch (not applied)

1. Delete `Cargo.toml:379` and let `packages/rmw/transport-callbacks` become a
   member. It builds; `just check`'s clippy then covers it. If there is a real
   reason to keep it out, put the reason in a comment beside the line — the
   absence of one is the actual defect.
2. Delete the 36 dead entries.
3. Gate the residual: a script that (a) fails on an `exclude` path with no
   directory on disk, and (b) requires every remaining exclude to match one of
   the five structural reasons or carry an explanatory comment on its own line.
   Both halves are cheap and neither needs a build. This is deliberately *not* a
   gate on the 0895 leaf class, which is harder and separately owned.
