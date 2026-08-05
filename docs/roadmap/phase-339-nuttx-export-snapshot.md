# Phase 339 — NuttX consumers link the per-arch EXPORT, not the live tree

**Goal:** an arm fixture and a riscv fixture can be valid at the same time.
Today they cannot: both architectures build in one NuttX checkout, so whichever
built last owns `staging/`, and the other architecture's already-linked entries
are — correctly — reported stale.

**Implements:** the fix method recorded in
[issue 0433](../issues/archived/0433-nuttx-kernel-restaged-after-entries-so-freshness-never-converges.md).
**Related:** [RFC-0064](../design/0064-board-support-organization.md) (board
support organisation; phase-337 consolidated 27 → 17 board dirs and this phase
does not move any of them), [issue 0196](../issues/archived/) (a guard whose
coverage is narrower than its rule), [issue 0435](../issues/) (the sibling
build-graph gap phase-337 found — different edge, same family).

**Base:** `phase-337-board-support-matrix`. phase-337 reshaped the NuttX board
crates; this phase changes only what consumers LINK against, so it rebases on
that work rather than racing it.

## Problem

`third-party/nuttx/nuttx` is a single configured tree. NuttX builds in-tree and
holds one `.config` at a time — `scripts/nuttx/build-nuttx.sh` says so itself:
"the single in-tree `.config` can only hold one board at a time."

`just nuttx build-fixtures` runs both architectures through that one tree:

```
build-fixtures-arm    build-nuttx.sh  DEFCONFIG=nuttx-config/arm/defconfig
                                      → make → staging/*.a  (arm)          ①
                      cargo …         → entries LINK against staging/       ②
build-fixtures-riscv  build-nuttx.sh  DEFCONFIG=nuttx-config/riscv/defconfig
                                      → key differs → distclean + make
                                      → staging/*.a  REWRITTEN (riscv)      ③
```

After ③ the arm entries from ② are older than the archives they linked against.
Measured in one run: arm entry `20:42:48`, `staging/libc.a` `20:46:00`, and two
full `Building NuttX...` plus two "export up-to-date — skipping" in the same
invocation.

**The freshness probe is right, and that is the point.** `nuttx_ffi_build.rs`
scans `staging/` for whichever `lib*.a` exist and declares
`cargo:rerun-if-changed=<staging>` precisely so it never links "a lib list from
the other config's kernel" (its own comment). That declaration lands in each
entry's `.d`, which the test-side probe reads. Relinking an arm entry after ③
really would pull riscv archives.

So this is not a probe bug and must not be fixed there. Issue 0433 records the
rejected exemption and why: unlike the two exemptions
`dep_info_newer_source` already carries, the shared staging content CAN differ
semantically without an edited source in the dep graph.

The config INPUTS are already per-arch and correct —
`packages/boards/nros-board-nuttx-qemu/nuttx-config/{arm,riscv}/defconfig`,
230 differing lines, different `CONFIG_ARCH` / board / chip. Only the build
OUTPUTS are shared.

## The convention we are not following

NuttX's answer to "build once, link many" is `make export`: it packages a
configured build into `nuttx-export-<ver>.tar.gz` so external code links a
SNAPSHOT instead of the live tree. We already run it — and then link the live
`staging/` anyway, and wipe the export (`rm -rf nuttx-export-*`) on every run
while keying its directory on the NuttX version rather than the architecture.

Both of those are our choices, not NuttX's. Undoing them is this phase.

## Measured: the export is a superset of staging

Verified against a real arm export (`nuttx-export-12.12.0/`), not assumed:

| link input | today | in export |
| --- | --- | --- |
| staged archives | `staging/lib*.a` | `libs/` — **superset** |
| board lib | `arch/arm/src/board/libboard.a` (special-cased in build.rs) | `libs/libboard.a` |
| libgcc / libm | not staged | `libs/` |
| linker script | `boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld` | `scripts/dramboot.ld` |
| arch includes | `arch/arm/src/{chip,common,…}` | `arch/{arm,board,chip,common,…}` |
| startup | — | `startup/crt0.o` |
| **vector table** | `arch/arm/src/arm_vectortab.o` | **ABSENT** — the one gap |

Two consumers simplify as a side effect: the `libboard.a` special case and the
directory scan both collapse into "point at `libs/`".

## Work items

### W1 — Per-arch export snapshots

- [ ] `scripts/nuttx/build-nuttx.sh`: derive the arch (it already greps
      `CONFIG_ARCH` for its run hint) and extract the export to
      `nuttx-export-<arch>-<ver>/`. Stop `rm -rf nuttx-export-*` from wiping the
      OTHER architecture's snapshot — scope the removal to this arch.
- [ ] Copy the vector-table object into the snapshot (`<snapshot>/startup/`)
      when the config has one. arm does; riscv sets `NUTTX_VECTORTAB=""` and
      skips it already.
- [ ] Keep the up-to-date short-circuit working per arch: the marker
      (`HEAD:sha256(defconfig)`) and the export-presence check must both become
      arch-aware, or a second arch's build will look "current" and skip.

**Acceptance:** building arm then riscv leaves BOTH snapshots on disk, each
containing that arch's archives. `nuttx-export-arm-*/libs/libc.a` is untouched
by a riscv build (mtime unchanged).

### W2 — Consumers link the snapshot

- [ ] `packages/boards/nros-board-common/src/nuttx_ffi_build.rs`: resolve
      `<snapshot>/libs` instead of `nuttx_dir.join("staging")`; drop the
      `libboard.a` special case; point `cargo:rerun-if-changed` at the snapshot.
- [ ] `packages/boards/nros-board-common/src/nuttx_image_link.rs:108`: same
      path; `NUTTX_LD_SCRIPT` and `NUTTX_VECTORTAB` resolve inside the snapshot.
- [ ] Update the env defaults that name live-tree paths — the arm defaults in
      `nuttx_image_link.rs` and the explicit riscv values in `just/nuttx.just`
      (`NUTTX_ARCH_INCLUDES` / `NUTTX_LD_SCRIPT` / `NUTTX_VECTORTAB`) and
      `nros-board.toml`.
- [ ] `scripts/build/fixture-inventory.py`: the `nuttx-kernel-export-preflight`
      row declares `shared_mutation: "$NUTTX_DIR/staging/libc.a; …"`. When the
      sharing is gone the declaration must go with it — a stale `shared_mutation`
      is worse than none.

**Acceptance:** `cargo:rerun-if-changed` names no path under the live tree; a
riscv build does not dirty an arm entry.

### W3 — Prove the thing this phase exists for

- [ ] `just nuttx build-fixtures` (BOTH arches, one invocation), then assert the
      arm entry is fresh relative to what it linked. That is the exact check
      issue 0433 used to demonstrate the bug, and it currently fails.
- [ ] `just nuttx test` / the nuttx action Runtime cells green for BOTH
      architectures from a single build.
- [ ] A regression test or gate so this cannot silently return. The natural
      shape: assert no NuttX fixture's `.d` names a path under
      `$NUTTX_DIR/staging`. Cheap, and it fails the moment a consumer reaches
      back into the live tree.

**Acceptance:** the interleaved build converges. Until W3 passes, the phase has
not delivered — W1 and W2 are the means.

## Risks

- **Blast radius.** This changes the link path for every NuttX fixture on both
  architectures. Sequence it arm-first: land the snapshot, repoint arm, verify,
  then riscv. A half-migrated state where one arch reads the snapshot and the
  other the live tree is FINE as an intermediate — they are independent.
- **The vectortab may not be droppable.** If `libarch.a` already carries the
  symbols the copy is unnecessary; if not, it must be snapshotted. A link test
  decides — do not reason about it.
- **Export layout vs `NUTTX_ARCH_INCLUDES`.** The arm export shows
  `arch/{arm,armv7-m,…}` while the board is `qemu-armv7a` (armv7-a). Confirm the
  include set the snapshot exposes matches what each arch expects before
  repointing, rather than assuming the names line up.
- **Disk.** Two snapshots instead of one wiped dir. Small next to the build
  trees, but the CI runner is already the constraint in issue 0200.

## Explicitly out of scope

- Board directory layout. phase-337 settled that; this phase moves no crates.
- The freshness probe. It is correct today and stays untouched — see the
  rejected option in issue 0433.
- Any other platform's staging/export handling. FreeRTOS, Zephyr and ThreadX
  have their own arrangements and no evidence of this defect.
- Issue 0435 (cmake fixtures miss the generated-RMW-header dep). Same family —
  a missing build-graph edge — but a different edge, and phase-337 already filed
  it.
