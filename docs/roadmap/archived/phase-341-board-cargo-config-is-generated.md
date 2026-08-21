# Phase 341 — The board's `cargo_config` is generated into the leaf, not mirrored by hand

**Amends:** [RFC-0032](../design/0032-entry-codegen-pipeline.md) §3 (the "third leg").
**Status (2026-08-21).** COMPLETE — **W1–W4 all landed, and the tier-2 sweep
they were waiting on has run.** The design was corrected after W1: the
projection is COMMITTED and gated, not gitignored — see "Correction" below.

**Closing audit (2026-08-21).** The header said "COMPLETE" from 2026-08-07 while
W3 and W4 carried unticked boxes, so the state was re-derived from the tree
rather than from either claim:

| | |
| --- | --- |
| tracked leaf `.cargo/config.toml` | 74 |
| carrying the generated board include | 42 |
| still declaring `[target.*]` | 20 — **19 out of scope, 1 known** |
| **leaves that LOST a block and gained nothing** | **0** |
| projections matching a fresh render (`check-board-projections`) | 41 |

Of the 20 still declaring a block, 19 have no
`[package.metadata.nros.entry] deploy` at all, so they have no board to inherit
from — W1's own rule for `-Wl,--wrap=poll` and `--nmagic`. The 20th is
`fixtures/multi_pkg_workspace_freertos/src/firmware`, the sync-unreachable leaf
recorded under "A trap W3 hit". So the in-scope migration is DONE, and the
`0 ungoverned` line is this phase's own audit, run again at close.

A false alarm worth recording: a first pass of that audit asked "declares
`deploy` but has neither include nor `[target.*]`" and flagged the six
`threadx-linux/rust` leaves. They are correct — `nros-board-threadx-linux`'s
descriptor declares NO `cargo_config` (it is a Linux-userspace port whose Entry
is a host binary, deliberately without `[build] target`), so there is nothing to
project and nothing to include. The doc's own condition — HEAD HAD a block and
the worktree has neither — is the one that means anything, and it returns 0.
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
a hand-maintained copy of it, and the shell gate of that name said so (deleted
2026-08-21 when W4's replacement made it dead code):

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
33  -C link-arg=--gc-sections     (thumbv7m 25, riscv64gc 6, riscv32imc 2)
 1  -C link-arg=-Wl,--wrap=poll
 1  -C link-arg=--nmagic
```

**Two corrections to the first pass of this measurement**, both found while
executing W1:

* `--gc-sections` spans **three triples and four boards**, not "32 thumbv7m
  leaves" as first written. It is 25 of 32 `thumbv7m`, 6 of 7 `riscv64gc`, 2 of
  4 `riscv32imc` — carried by most leaves of every embedded board, which makes it
  a board property whose absence in the remainder is drift rather than intent.
* A reported "6 × `third leg`" extra on the NuttX arm leaves was a **parser
  artifact**: the regex read words out of a comment sitting inside the
  `rustflags` array. Those leaves carry no extra at all. Any future run of this
  measurement must strip comments before tokenising.

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

## Correction 2026-08-07 — the projection is committed, not gitignored

W2 originally routed the board block to a **gitignored** sidecar, by analogy with
the `nros-managed-patch.toml` split issue 0457 established. That analogy is
wrong, and the error is worth stating because it is easy to repeat.

**Gitignoring the block means a fresh clone cannot LINK an embedded leaf** until
`nros sync` has run — the leaf would be missing its link args, not merely its
patches. That is issue 0463's lesson pointed at a worse target: 0463 was a leaf
being unreadable before sync; this would make the *image* depend on sync. Today
those args are tracked and a clone works, so the sidecar route is a regression.

The 0457 analogy fails on the property that actually decided that split:
**host-derivedness.** Sync's patch rows name `generated/` trees built from the
user's ament install, so committing them asserts one host's resolution — which is
why they are gitignored. A board's `cargo_config` is the opposite: a fixed string
in a committed descriptor, identical in every checkout. Issue 0473 (withdrawn)
established exactly this distinction, and it points the other way here.

**The repo already has the right pattern for "derived, but must exist in a
clone": generate it, COMMIT it, and gate staleness.**
`packages/core/*/src/generated.rs` is bindgen output, committed, with
`check-abi-bindings` failing when it does not match a fresh regeneration from the
C-header SSoT (RFC-0054). Same shape as this problem — one SSoT, a mechanical
projection, and a gate that makes drift impossible to commit.

| | clone builds? | drift possible? |
| --- | --- | --- |
| today — hand-mirrored, tracked | yes | **yes** (issue 0440) |
| gitignored sidecar (original W2) | **no** | no |
| **generated + committed + gated** | **yes** | **no** |

The third row is the design. It keeps what tracking is for and removes the
hand-mirroring 0440 punished.

**Consequences for the work items**, which are rewritten below to match:

* the projection is a TRACKED file, so `.gitignore` does not change and leaves do
  NOT become "pure sync output" — the untracking consequence originally claimed
  here does not happen;
* **W4 replaces the gate rather than deleting it**: `check-board-cargo-config-
  applied` gives way to a regeneration check in the `check-abi-bindings` mould,
  trading representative-arg matching for exact comparison.

## Direction

`nros sync` already has both inputs — the leaf declares `deploy = <board>`, and
the board declares `cargo_config` — and already writes managed content into these
files. Project the board block into a generated, **committed** include rather
than expecting a human to mirror it.

```toml
# leaf/.cargo/config.toml — TRACKED, authored content only
include = [
  "…/nros-patch.toml",              # central host patches      (gitignored, #272)
  ".cargo/nros-managed-patch.toml", # host-specific patch rows  (gitignored, #457)
  ".cargo/nros-board.toml",         # NEW: board cargo_config   (GENERATED + TRACKED)
]
# …authored remainder, if any
```

`[build] target`, `[unstable] build-std*` and `[target.<triple>]` are all board
properties and move together into `nros-board.toml`. What remains authored is
only what a board cannot know: a leaf-specific QEMU port, a one-off patch.

The generated file carries a "DO NOT EDIT — regenerate with `nros sync`" header
naming its descriptor, like every other committed projection in the tree.

## Consequences

* **23 distinct blocks collapse to ~8 board descriptors**, which already exist.
  The 59 copies become 59 generated files with no hand-maintained content — a
  projection each, not a mirror each.
* **A clone still links.** The projection is committed, so nothing about
  build-from-clone changes; this is what separates it from the sync-owned patch
  rows, which are host-derived and must not be committed (#457, #473).
* `check-board-cargo-config-applied` is **replaced, not deleted**: a regeneration
  check in the `check-abi-bindings` mould compares the committed projection
  against a fresh render of the descriptor. Exact comparison replaces the current
  representative-arg probe, so a leaf that loses ONE argument fails, not only one
  that loses the whole group.
* RFC-0032's "a stale pin is a config-sync bug" stops describing a live class —
  drift becomes uncommittable rather than merely detectable.

## Work items

### W1 — Fold the strays into the descriptors — **LANDED 2026-08-07**

- [x] `--gc-sections` hoisted into all FOUR boards that own an affected triple:
      `nros-board-mps2-an385`, `nros-board-mps2-an385-freertos`,
      `nros-board-threadx-qemu-riscv64`, `nros-board-esp32-qemu`.
- [x] `-Wl,--wrap=poll` — **stays leaf-local.** It is on
      `nros-nuttx-riscv-ffi`, a library crate with no
      `[package.metadata.nros.entry] deploy`, so it is not an entry leaf and has
      no board to inherit from.
- [x] `--nmagic` — **stays leaf-local.** One leaf
      (`logging-smoke-mps2-baremetal`) carries it while the other 31 `thumbv7m`
      leaves do not. Hoisting would change section layout for all 32 to serve
      one, and the leaf declares no `deploy` either. It becomes the "authored
      remainder" this design leaves room for — and is the only such case in the
      tree, so the concept is load-bearing exactly once.
- [x] The 3 uncovered `thumbv7em-none-eabihf` leaves (`stm32f4-porting/{polling,
      rtic}`, `stm32f4-smoltcp-echo`) — **no descriptor, deliberately.**
      phase-337 demoted stm32f4 out of the support matrix to be the book's
      customization example; there is no `nros-board-stm32f4` crate to declare
      one. They stay fully authored and never migrate.

**Result:** **54 leaves now carry exactly their board's args** (was 14 equal /
40 superset). The 2 remaining non-matches are the two leaf-local decisions
above, both out of the entry-leaf scope. Board gates re-verified green
(`check-board-cargo-config-applied`, `check-board-manifest-drift`,
`check-board-tiers`).

**A W3 obligation this creates.** Ten leaves currently LACK `--gc-sections` and
will gain it when the board block starts governing:

```
thumbv7m    nros-bench/{large-msg-baremetal,wake-latency-cortex-m3,wcet-cycles-qemu}
            nros-tests/bins/{cdr-roundtrip-qemu,lan9118-qemu,
                             logging-smoke-freertos-mps2,logging-smoke-mps2-baremetal}
riscv64gc   nros-tests/bins/logging-smoke-threadx-riscv64
riscv32imc  nros-smoke/esp32-hello-world, nros-tests/bins/logging-smoke-esp32-qemu
```

All ten are test/bench binaries. `--gc-sections` is safe for the `__NROS_SIZE_*`
and FORCE_LINK anchors because they are `#[used]` (CLAUDE.md states this), but
"safe in principle" is what 0440 also looked like. **W3 must link these ten
specifically**, not merely build the families they sit in.

### W2 — Emit `.cargo/nros-board.toml` from sync (committed) — **LANDED**

- [x] `nros sync` learned board resolution (`cmd/ws.rs::project_board_configs`,
      running after the patch pass). `BoardCatalog::resolve_deploy` is new:
      `resolve(board, target)` cannot serve, because the target is what the
      projection derives.
- [x] `deploy` → descriptor is **`names` first, `platform` second, each
      requiring a UNIQUE hit**; anything else writes NOTHING and is reported.
      `names` is what separates the two NuttX descriptors (same `platform`, same
      crate, different triple); `platform` is the bash gate's mapping, kept as
      the fallback.
- [x] `.cargo/nros-board.toml` carries a DO-NOT-EDIT header naming its
      descriptor; the `include` entry follows `render_patch_config_with`'s
      evict-then-re-add discipline and is written ONLY when the file was.

**Two findings that change W3.**

1. **The include is withheld while the leaf still mirrors the block.** Cargo
   JOINS `rustflags` arrays across merged config files — measured: a leaf
   carrying its mirror *and* the include hands the linker `-Tdramboot.ld`
   **twice**. So sync writes the projection but adds the include only when the
   leaf declares none of the same keys (compared at key depth 2). W3's migration
   is therefore a pure DELETION: drop the mirrored tables, re-run sync, the
   include appears. Today that means every resolvable leaf gets a projection and
   zero get an include — deliberately inert, exactly the overlap this phase asked
   for.
2. **`${workspace}` keys are withheld from the projection.**
   `nros-board-nuttx-qemu` patches `libc` at
   `${workspace}/third-party/nuttx/libc`; rendered it is an absolute host path,
   which a COMMITTED file must not carry (issue 0457), and cargo resolves a
   config `[patch]` path against the invocation CWD, so the descriptor cannot
   express it relatively either — only the leaf's own depth can. Those top-level
   keys are dropped from the projection, named in its header and in sync's
   summary; the leaf keeps declaring them (it already does, relatively). A board
   whose whole `cargo_config` is placeholder-bearing projects nothing.

**A W3 obligation this creates — four deploy tokens resolve to no descriptor:**
`qemu-mps2-an385`, `rtic-mps2-an385`, `threadx-qemu-riscv64`,
`qemu-esp32-baremetal` (≈20 leaves). They are legitimate board identities —
`nros_orchestration_ir::board_path_for` maps all four — but the descriptors'
`names` lists do not carry them, so the deploy vocabulary has two SSoTs. W3 must
add them as `names` aliases (a data edit, cross-checked against `board_path_for`)
before those families can migrate. Sync names them in a summary line each run.

**Acceptance:** a leaf builds with its `[target.*]` block moved out of the tracked
config into the generated one, and `just nuttx build-fixtures` LINKS.
Verified so far (W3 owes the link): on a NuttX-deploying leaf with the mirror
removed, `cargo config get` reads `build.target`, `unstable.build-std`,
`target.armv7a-nuttx-eabihf.{linker,rustflags}` — the full 24-arg group — through
the generated include, beside the leaf's own authored `libc` patch.

### W3 — Migrate, one board family at a time

- [x] Per family: DELETE the mirrored tables from the leaf's tracked config,
      re-run `nros sync` (which then adds the include), commit the projection
      alongside, verify the family LINKS. One family per commit. Nothing is
      untracked — the projection is committed.
- [x] Start with `thumbv7m` (32 leaves, one block, the largest single win) and do
      NuttX last (the 0440 family — most args, most to lose). **Final: 42 leaves
      carry the include; 0 ungoverned.**
- [x] **First, the four unclaimed deploy tokens** (W2 finding above) — without
      the `names` aliases, `qemu-mps2-an385` / `rtic-mps2-an385` /
      `threadx-qemu-riscv64` / `qemu-esp32-baremetal` leaves get no projection at
      all, which is most of `thumbv7m`.
- [x] **Then `check-cargo-config-tracked`'s `has_authored_content`**: it treats
      the whole `include = ` line as sync output, so a leaf left holding only the
      board include reads as "pure sync output IS tracked" and the gate demands
      untracking it — which would delete the very include a clone needs. Either
      that rule or W4's replacement has to account for the board include.

**Acceptance:** the family's fixtures build and its tests pass; the tracked
config count drops by the family's size.

### W4 — Replace the gate with a regeneration check

- [x] Swap `check-board-cargo-config-applied` for a check that re-renders each
      board's `cargo_config` and compares it to the committed
      `.cargo/nros-board.toml`, in the `check-abi-bindings` mould. Exact
      comparison, not a representative arg.
- [x] Keep it in `check-fast` — it is buildless, like the gate it replaces.
- [x] The comparison must re-render through the SAME withholding rule W2 uses
      (`committable_board_config`), or every NuttX projection reads as drift
      against a descriptor whose `${workspace}` row can never be committed.

**Acceptance:** the check fails on a hand-edited projection (tripwired, not
merely observed passing), and no tracked leaf `config.toml` carries a `[target.*]`
block a descriptor also declares.

## Two more traps W3 hit — the tables a projection may take are per-BOARD

**A leaf keeps whatever the descriptor does not declare, and the set differs by
board.** Deleting a fixed list of tables is wrong; the projection's own tables
are the list.

* **mps2** declares only `[target.*]` → the leaf keeps `[build]`.
* **threadx** declares `[build]` + `[target.*]`, and its `[env]` is WITHHELD
  (`${workspace}` renders to an absolute host path). Deleting only `[target.*]`
  left the leaf's `[build]` conflicting with the projection's, so W2 correctly
  withheld the include and **all six leaves went ungoverned** until `[build]`
  went too. The conflict check did its job; the migration step was wrong.
* **esp32** declares `[build]`, `[target.*]`, `[env]`, `[unstable]` — but its
  `[env]` is ONLY `ESP_LOG`, while the leaf's carries three more keys that are
  genuine per-leaf tuning:

  ```toml
  NROS_EXECUTOR_ARENA_SIZE     = "16384"
  NROS_SMOLTCP_MAX_UDP_SOCKETS = "2"
  ZPICO_SUBSCRIBER_LARGE_SIZE  = "4096"   # or `.bss` overflows DRAM by ~54 KiB
  ```

  Deleting the whole `[env]` produced exactly that: `rust-lld: section '.bss'
  will not fit in region 'DRAM': overflowed by 50476 bytes`. The leaf's own
  comment had predicted the number. **A table name matching is not permission to
  delete the table** — only the keys the projection actually supplies may go.

## A trap W3 hit, recorded for the remaining families

**Two fixture leaves have no `package.xml`, so `nros sync` never visits them** —
`fixtures/multi_pkg_workspace_freertos/firmware` and
`fixtures/orchestration_tiers_freertos`. Deleting their mirrored block therefore
removed the link args and put NOTHING in their place: no projection, no include,
no `[target.*]`. Caught by auditing every leaf for "lost a block, gained
nothing" and reverted; they stay mirrored until sync can reach them.

This is issue 0440's exact shape — a silently lost link group — reintroduced by
the change meant to prevent it. **Before migrating a family, confirm sync
actually visits every leaf in it.** The audit is cheap:

```
for each tracked .cargo/config.toml:
    if HEAD had [target.*] and the worktree has neither [target.*] nor an
    nros-board.toml include -> UNGOVERNED, revert it
```

Final state of the thumbv7m migration by that audit: 22 migrated, 37 still
mirroring, **0 ungoverned**.

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

## W4 as landed — the replacement, and where it lives

The `check-board-cargo-config-applied` RECIPE was rewritten in place to call
`nros ws check-board-projections`, which re-renders each descriptor through the
same `project_board_configs_with` the writer uses (CHECK mode) and compares it to
the committed projection. Sharing the renderer is the point: a second
implementation in shell would be the drift this phase removed.

The old shell script sat unreferenced beside it until 2026-08-21 and is now
deleted. It is worth knowing it misled a reader: its docstring still asserted the
leaf and descriptor "are kept in step **by hand**", the sentence this phase
falsifies, so judging W4 from the file's existence read as "not started" when the
recipe had not called it for two weeks. **A replaced gate is not replaced until
the old implementation is gone.**
