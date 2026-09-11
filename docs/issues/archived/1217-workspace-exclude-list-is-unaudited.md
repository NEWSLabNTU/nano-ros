---
id: 1217
title: "The root workspace `exclude` list is unaudited: one host-buildable crate
  is excluded for no reason and 36 of its 174 entries name directories that do
  not exist"
status: resolved
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

## Resolved (phase-451 W3, 2026-09-11)

Three changes, and the audit found more than the filing did.

**The 36 stale entries are gone.** They sat in six contiguous runs of six, each
under a comment describing only that run, so each run and its comment were
removed together: the `threadx-linux`, `mps2-an385-freertos` and
`qemu-armv7a-nuttx` Entry-package siblings, and the `threadx-linux` /
`rv-virt-threadx` zenoh example sets. 165 exclude entries to 128.

**`packages/rmw/transport-callbacks` is a member.** It builds clean on the host
(`cargo check --manifest-path packages/rmw/transport-callbacks/Cargo.toml`,
2026-09-11), and `cargo metadata` resolves with it in `members`. `Cargo.lock`
gains exactly one entry — `nros-transport-callbacks` and its single dep
`nros-rmw`, seven lines — and nothing else in the lock moves. (An earlier note
here said the lock was unchanged; that was read before the lock had been
regenerated. The change is minimal, not absent.)

**The five structural reasons this issue names were not enough — there are
seven.** Classifying all 129 surviving entries against the five left 17
unjustified, which is not "one crate excluded for no reason" and would have made
the gate unlandable. Two more reasons are real and derivable:

* **a package whose ANCESTOR carries the `[workspace]` table** — a member of a
  nested workspace. This is all 11 fixture leaf packages under
  `packages/testing/nros-tests/fixtures/*/`, and it collapsed 17 to 7.
* **a package declaring no Rust target at all** — no `src/`, no `[lib]`, no
  `[[bin]]`. `packages/interfaces/rcl-interfaces` and `lifecycle-msgs` are this:
  metadata shells whose real crates are the generated ones underneath.

That left 4: `nros-board-{freertos,threadx,nuttx}` and
`nros-baremetal-common`. These are genuinely cross-only in a way no manifest
fact states — no cross-only dependency, no pinned target; the kernel build glue
is simply not a cargo fact. They are DECLARED in
`.config/workspace-exclude-reasons.txt`, a shrink-only ratchet, rather than
given an invented derivation. A gate asserting a reason it did not measure is
the defect phase-450 exists for.

**Gate:** `just check workspace-exclude-list`
(`scripts/check-workspace-exclude-list.py`, registered in `just/check/cargo.just`
beside its sibling `nested-workspace-excludes`). It self-tests its three
classifier arms on every run, and both failure arms were mutation-tested: a
fabricated stale entry and re-excluding `transport-callbacks` each exit 1, and
the restored tree exits 0.

