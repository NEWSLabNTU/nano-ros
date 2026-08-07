# Phase 341 — The board's `cargo_config` is generated into the leaf, not mirrored by hand

**Amends:** [RFC-0032](../design/0032-entry-codegen-pipeline.md) §3 (the "third leg").
**Related:** issue 0440 (the drift this prevents), RFC-0048 W9 (`nros sync` owns
the leaf `.cargo/config.toml`), issue 0457 (the gitignored sidecar this reuses),
issue 0473 (withdrawn — established that tracked configs carrying sync-managed
rows are BY DESIGN, which is what makes this proposal cheap).

## Problem

RFC-0032 states the current contract plainly:

> The linker script is the third leg — the Entry pkg's `.cargo/config.toml`
> `link-arg=-T<board>.ld` **must track** the board descriptor's `cargo_config`
> (the board build.rs emits the script to `OUT_DIR`); a stale pin (`-Tlink.x`) is
> a **config-sync bug**, not a codegen one.

`nros-board.toml`'s `cargo_config` is the SSoT. The leaf's `[target.*]` block is
a hand-maintained copy of it, and `scripts/check-board-cargo-config-applied.sh`
says so:

> The leaf file is TRACKED and `nros sync` leaves it alone, so the two are kept
> in step **by hand**.

That is the mirror-drift class CLAUDE.md names, and it has already fired. Issue
**0440**: phase-338 W2 collapsed 18 `-entry` packages into their node packages
and kept the *node's* config, which carried only CPU flags. All six NuttX Rust
entries then failed with **~3680 undefined libc references** — valid TOML,
`cargo metadata` happy, `nros sync` happy, visible only at link time on the one
platform whose archives went missing.

## Measurements (2026-08-07)

| | |
| --- | --- |
| tracked `.cargo/config.toml` | 75 |
| carrying a `[target.*]` block | 59 |
| **distinct** `[target.*]` blocks among those 59 | **23** |
| target triples spanned | 6 |
| absolute/host-specific paths inside those blocks | **0** |

One block is copied into 12 leaves, another into 7, three more into 6 each.

**How much is board-derived**, comparing rustflags as SETS (formatting and line
splitting differ freely between the two copies, so a textual diff understates
this badly — an early attempt at this measurement did, and reported the opposite
conclusion):

| | leaves |
| --- | --- |
| leaf args **==** board args | 14 |
| leaf args **⊇** board args, plus extras | 40 |
| leaf args missing a board arg | 2 — **both out of scope**, see below |
| triple declared by no board | 3 |

So **54 of 56** in-scope leaves are the board's block verbatim or a superset. The
extras are three distinct arguments in total:

```
32  -C link-arg=--gc-sections
 1  -C link-arg=-Wl,--wrap=poll
 1  -C link-arg=--nmagic
```

`--gc-sections` appearing in 32 of 32 `thumbv7m` leaves is not leaf-specific
content; it is a board property that never made it into the descriptor.

**The two "missing" leaves are not drift.** `examples/workspaces/realtime-rust`
(a workspace-root config) and `nros-nuttx-ffi` (a library crate) declare no
`[package.metadata.nros.entry] deploy`, so neither is an entry leaf and neither
should carry a final link group. `check-board-cargo-config-applied` passes.
Recorded because the raw sweep flags them and the next person will re-derive it.

**Gate coverage.** That gate checks leaves whose `deploy` names a board with `-l`
args: **8 leaves**. Fifty-nine carry board-derived blocks. It is also
*representative, not exhaustive* by design — it catches a config that lost the
GROUP, not one that lost a single argument. So ~14 % of the surface is guarded,
against a failure mode that is invisible until link time.

## Direction

`nros sync` already has both inputs — the leaf declares `deploy = <board>`, and
the board declares `cargo_config` — and already writes managed content into these
files. Emit the board block as a generated include rather than expecting a human
to mirror it.

```toml
# leaf/.cargo/config.toml — TRACKED only if something is left below
include = [
  "…/nros-patch.toml",              # central host patches      (gitignored, #272)
  ".cargo/nros-managed-patch.toml", # host-specific patch rows  (gitignored, #457)
  ".cargo/nros-board.toml",         # NEW: the board's cargo_config (gitignored)
]
# …authored remainder, if any
```

`[build] target`, `[unstable] build-std*` and `[target.<triple>]` are all board
properties and move together. What remains authored is only what a board cannot
know: a leaf-specific QEMU port, a one-off patch.

## Consequences

* **23 distinct blocks collapse to ~8 board descriptors**, which already exist.
  The 59 copies stop existing.
* Leaves whose config becomes pure sync output are **already covered by the
  existing `**/.cargo/config.toml` ignore** — no new policy, and issue 0473
  established that the tracked/untracked split is decided exactly this way.
* `check-board-cargo-config-applied` becomes **unnecessary rather than
  strengthened**: generated content cannot drift, so 0440 becomes unreachable
  instead of merely detectable. Delete it with the last migrated family, not
  before.
* RFC-0032's "a stale pin is a config-sync bug" stops describing a live class.

## Work items

### W1 — Fold the strays into the descriptors

- [ ] Move `--gc-sections` into the `thumbv7m` boards' `cargo_config` (32 leaves
      carry it; none is the exception that would make it leaf-specific).
- [ ] Decide `-Wl,--wrap=poll` and `--nmagic`: board property, or genuinely
      leaf-local? One instance each — cheap to settle, and settling them is what
      determines whether the "authored remainder" concept is needed at all.
- [ ] Give the 3 uncovered triples a descriptor, or record why they have none.

**Acceptance:** every in-scope leaf's `[target.*]` args are a SUBSET of its
board's, measured by the set comparison above, not textually.

### W2 — Emit `.cargo/nros-board.toml` from sync

- [ ] `nros sync` resolves `deploy` → board → `cargo_config`, writes the block to
      the gitignored sidecar, and adds the third `include` entry.
- [ ] Removing the include target must FAIL LOUDLY, not silently drop the link
      args — issue 0463 established that cargo errors during manifest parse, and
      `_require-leaf-includes` is the existing seam that turns that into "run
      `nros sync`".

**Acceptance:** a leaf builds with its `[target.*]` block deleted from the
tracked config, and `just nuttx build-fixtures` links.

### W3 — Migrate, one board family at a time

- [ ] Per family: delete the mirrored block, verify the family links, untrack any
      config that is now pure sync output. One family per commit.
- [ ] Start with `thumbv7m` (32 leaves, one block, the largest single win) and do
      NuttX last (the 0440 family — most args, most to lose).

**Acceptance:** the family's fixtures build and its tests pass; the tracked
config count drops by the family's size.

### W4 — Retire the gate

- [ ] With the last family migrated, delete `check-board-cargo-config-applied`
      and its `check-fast` entry, recording in the commit that generation
      replaced it.

**Acceptance:** no tracked leaf config carries a `[target.*]` block that a board
descriptor also declares.

## Risks

**The link group is the thing you cannot lose quietly.** 0440 cost ~3680
undefined references and was invisible until link time on one platform. W3 must
verify each family by LINKING, not by building a host target — which is why the
acceptance names fixtures rather than `cargo check`.

**Sync must not become the only copy before it is trusted.** W2 lands the
generator while the mirrored blocks still exist; W3 removes them only after the
generated path is proven per family. The overlap is deliberate.

**Three leaves have no board descriptor for their triple.** Until W1 settles
them they cannot migrate, and a partial migration must leave them working —
which the include-based composition does naturally, since an absent sidecar
simply means the tracked block still governs.
