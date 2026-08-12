---
id: 488
title: "Second-build-path residue: per-leaf cargo dirs — residues 1-3 FIXED; only residue 4 (NuttX objects written into the source leaf) remains"
status: open
type: tech-debt
area: build
related: [phase-340, rfc-0070, issue-0475, issue-0393]
---

## Context

phase-340 P2 closed the second build path for the population that blocks item 7 /
P4: a `cargo build` whose cwd is a leaf under `examples/` and which writes the
leaf's own `target/`. That population is now empty and gated by
`scripts/check-example-leaf-target-dirs.py`.

The sweep that found it turned up more sites in the same CLASS that the gate
deliberately does not cover, because they do not block P4 and each needs a
consumer moved with it. They are listed here so the next pass has the measurement
rather than a re-sweep.

## Residue 1 — per-leaf `target/` under `packages/testing/**`

Same defect (bare in-leaf `cargo build`, no manifest row, no coordinate, no
group), different tree. These do not block P4: the 391 per-leaf `.gitignore`
files item 7 deletes are the ones under `examples/`.

| site | leaf | consumer that must move with it |
| --- | --- | --- |
| `just/freertos.just` `build-fixture-extras` | `packages/testing/nros-bench/wake-latency-cortex-m3` (and its `wake-latency-pub` sibling) | `tests/wake_latency_cortex_m3.rs` spells `…/target/thumbv7m-none-eabi/<profile>/…` by hand |
| `just/qemu-baremetal.just` `build` / `build-fixtures` | `packages/testing/qemu-smoltcp-bridge` | resolver in `fixtures::binaries` |
| `just/qemu-baremetal.just` `build-rtic-main-e2e` | `packages/testing/nros-tests/bins/rtic-run-plan-e2e` | resolver in `fixtures::binaries` |
| `just/ros-editions.just` `build-fixture` | `packages/testing/nros-tests/bins/ros-edition-pose-pub` | the recipe's own echoed path; docker-gated lane |

The wake-latency pair is the one worth doing first: it runs inside
`build-test-fixtures` (via `just freertos build-fixtures`), on a MIGRATED
platform, so it re-creates a per-leaf tree on every full sweep. Preferred fix is
a `[[fixture]]` row (it would inherit the `freertos` group the six Entry rows now
share); the two images would then also stop being enumerated in the recipe.

## Residue 2 — authored per-leaf `target-<variant>/` on migrated platforms

These pass P2's gate by construction — they DO pass a `--target-dir` — and they
are covered by the repo-root `examples/**/target-*/` ignore, so they are not a P4
blocker either. They are still R1 duplicates: an authored dir on a migrated
platform is precisely what phase-340 W2 (work-order item 5) decided should name a
GROUP rather than a directory, and the fixture lane already does that
(`nros_fixture_strip_authored_target_dir`). These call sites are outside the
fixture lane and never got it.

- `just/freertos.just` — `build-with-tracing`, `_run-qemu` (`target-zenoh/`)
- `just/threadx-linux.just` `_run` (`target-zenoh/`)
- `just/threadx-riscv64.just` `_run-qemu` (`target-zenoh/`)
- `scripts/build/fixture-make-driver.sh` — `examples/native/rust/<role>`
  (`target-cyclonedds/`); this one IS in the native fixture lane and is the
  `native 2 (target-cyclonedds)` line phase-340 recorded as surviving wave 2
- `just/ros-editions.just` `build-e2e-fixtures` — `examples/native/rust/*`
  (`target-ros-edition-<distro>-<rmw>/`), 6 dirs × edition × rmw
- `just/px4.just` `build-examples` — `examples/px4/rust/companion/*`
  (`target-xrce/`); `px4` is NOT a migrated platform, so this one is correct
  today and becomes residue only if px4 joins `NROS_FIXTURE_SHARED_PLATFORMS`

`nuttx`'s `_run-qemu` was in this list until P2: it wrote a plain `target/` AND
hand-spelled its `-kernel` path, the exact pair phase-340 item 7 hit on esp32, so
it was fixed there rather than deferred here.

## Residue 3 — dev tool

`scripts/stack-analysis.sh` builds an arbitrary example dir into that dir's own
`target/` and then reads the ELF back out of it. It takes a directory argument,
so it has no fixed coordinate; it should probably grow a `--target-dir` of its
own rather than join a group.

## Residue 4 — NuttX writes OBJECTS into the example source leaf (2026-08-10)

Every residue above is a per-leaf *directory*. This one is file-grained and was
found by reading `git status` on a tree after a NuttX fixture sweep: the NuttX
apps build compiles an example's sources **in place** and leaves its output
beside them.

```
examples/qemu-arm-nuttx/c/talker/.built
examples/qemu-arm-nuttx/c/talker/.depend
examples/qemu-arm-nuttx/c/talker/Make.dep
examples/qemu-arm-nuttx/c/talker/src/main.c.home.aeon.repos.nano-ros.examples.qemu-arm-nuttx.c.talker.o
examples/qemu-arm-nuttx/c/talker/src/main.c.home.aeon.repos.nano-ros.examples.qemu-arm-nuttx.c.talker_1.o
```

**60 files across the 12 `qemu-arm-nuttx/{c,cpp}` leaves, 1.1 MB**, sitting in
`git status` as untracked source-tree noise — which is how a blanket
`git add -A` commits build output, the thing CLAUDE.md bans a blanket add for.

**They were unignored only inside a window, and the window is closed.** The
per-leaf `.gitignore` files carry these exact rules —

```
# NuttX apps `make` build artifacts (objects emitted beside sources)
/.built
/.depend
/Make.dep
/src/*.o
```

— `53681ecbc` (phase-334 W2.c) folded 391 of those files into one root block
that did not carry them, and `8f5cb2d18` (phase-344 / RFC-0070, "R1 is scoped by
context; restore the example ignores") put them back. All 12 leaves are covered
again, verified leaf by leaf. A tree checked out between those two commits shows
the 60 files; a current tree does not. **No root-level rule should be added** —
one was written before the restore was noticed and dropped as a second spelling
of a rule that already exists.

That leaves the real defect below untouched: the files are still *written*, they
are merely ignored again.

Two properties make this worse than the directory cases:

* **The object name embeds the absolute build path** (`main.c.home.aeon.repos.
  nano-ros…o`), so it is not merely uncoordinated — it is not relocatable, and
  two checkouts of the same tree produce differently-named objects for the same
  translation unit.
* **It is inside the source directory, not merely inside the leaf**, so no
  `--target-dir`-shaped fix applies. NuttX's apps `Makefile` decides this, not a
  nano-ros recipe, which is why it is the one residue whose fix is a build-system
  change rather than a coordinate.

Interim disposition (2026-08-10): the files were deleted and four scoped
patterns added to the repo-root `.gitignore`, tagged as a symptom ledger that
gets deleted when the build moves out of tree. The ignore is scoped to
`examples/qemu-arm-nuttx/**` on purpose — no other builder writes loose objects
into a source leaf, and a new one that starts doing so should appear in
`git status` instead of being swallowed.

Also cleaned in the same pass, and worth recording because it is the same class
one level up: **`packages/boards/nros-board-orin-spe/`** held a `Cargo.lock` and
a **167 MB** `target/` with **no source at all** — the crate was deleted by
`53a3402d2` (phase-337 W7.b) and its build output outlived it. A per-leaf target
dir has no owner once its leaf is gone, and nothing notices: the dir is ignored,
so `git status` showed only the orphaned lock. Any sweep that counts per-leaf
target dirs (phase-340 W2, phase-343, phase-344's census) counts these too.

## Why not fix them under P2

P2's acceptance is "the per-leaf dirs stop being recreated" for the population
that blocks P4, verified by a REBUILD, not by a gate. Every entry above needs its
consumer moved in the same commit (that is the whole lesson of #393 and of
phase-340 item 7's esp32 pack step), and each consumer is a different test. Doing
them in one change would make the rebuild that proves P2 unattributable.

## Fix sketch

Same mechanism P2 used, in preference order:

1. a `[[fixture]]` row — the build gets a coordinate, and therefore a group,
   a lane and a staleness probe, for free;
2. failing that, `nros_fixture_target_dir_flag` for the build plus its inverse
   `nros_fixture_row_artifact_dir` for the lookup, from `ONE`
   `nros_fixture_group` call so the two cannot disagree.

Never a new literal, and never a third spelling of the group key.


## Status (2026-08-12) — residue 1's in-lane site and residue 2's in-lane site are FIXED

Both sites that ran INSIDE `build-test-fixtures` on a migrated platform, i.e.
the two that re-created a per-leaf tree on every sweep, are done. The rest are
still listed above and still true.

**Residue 2 — `fixture-make-driver.sh`'s `linux-cyclonedds-rust` leaf.** It
hand-rolled `cd examples/native/rust/$role && cargo build … --target-dir
target-cyclonedds`, a second spelling of a build the manifest already describes
(`linux`/`rust`/`cyclonedds` rows exist for both roles, and `linux` is a shared
platform). The two spellings did not merely duplicate bytes, they DISAGREED: the
test resolver reads the manifest row (issue 0517), i.e. the group dir, while this
wrote a leaf dir nothing read. It now emits
`fixtures-build.sh linux rust cyclonedds`, which is what the default leaf set and
the sibling `linux-cmake-rmw` leaf already did — so it stops being a special case
as well as a duplicate. Verified: `just native build-fixture-extras` completes
and creates no `examples/native/rust/*/target-cyclonedds`.

One consequence worth stating: the manifest route builds the 8 rows at that
coordinate rather than the 2 the hand-rolled command named. They share the group,
so the marginal cost is small, and they were being built by the fixture lane
anyway.

**Residue 1 — the wake-latency pair.** Now a `[[fixture]]` row
(`packages/testing/nros-bench/wake-latency-cortex-m3`, freertos/rust/zenoh — one
crate, two bins, one row), so it builds into the shared `freertos` group with a
coordinate and a staleness probe. The bare in-leaf `cargo build` is gone from
`just freertos build-fixture-extras`, and `tests/wake_latency_cortex_m3.rs`
resolves through the row via `groups::select_sole_row`. Verified: the lane build
puts both images in
`build/fixtures-cargo/freertos/thumbv7m-none-eabi/nros-minsizerel/` and leaves no
leaf `target/`.

**Two defects were hiding behind that hand-spelled path**, which is the argument
for deriving it:

* `bench_image()` said `release/` while the build writes the FreeRTOS carve-out
  profile `nros-minsizerel`. The image would never have been found.
* The file is `#![cfg(feature = "trigger-test")]`, and that feature **does not
  link** (six `undefined symbol: nros_platform_*`). So the test was not skipping,
  it was absent — the issue-0317 gate has been reporting nothing. Filed as issue
  0526; the path half is fixed here, the link half is not.

Still open, unchanged: residue 1's other three sites (qemu-smoltcp-bridge,
rtic-run-plan-e2e, ros-edition-pose-pub), residue 2's `just` `_run`/`_run-qemu`
sites and the ros-editions / px4 ones, residue 3 (`stack-analysis.sh`), and
residue 4 (NuttX in-source objects — a NuttX build-system change, not a
coordinate).


## Status (2026-08-12, second pass) — residue 2's `just` run-paths are FIXED

The three `_run`/`_run-qemu`-shaped sites on MIGRATED platforms now build into
the shared group and read back from it:

| site | platform |
| --- | --- |
| `just/freertos.just` `_run-qemu` + `trace` | freertos |
| `just/threadx-linux.just` `_run` | threadx-linux |
| `just/threadx-riscv64.just` `_run-qemu` | threadx-riscv64 |

Each takes its `--target-dir` from `nros_fixture_target_dir_flag` and its
artifact path from `nros_fixture_row_artifact_dir`, i.e. from ONE
`nros_fixture_group` call — which is the property that matters here, not the
byte saving. phase-340 item 7's esp32 failure was exactly a recipe that kept
building in one place and looking in another.

Verified: `--target-dir` and lookup resolve to the same dir
(`build/fixtures-cargo/freertos`, and the variant group
`threadx-linux-3263301353` for the `--no-default-features --features rmw-zenoh`
rows); a build through the derived flag lands the kernel where the recipe reads
it; and **zero** `target-zenoh/` dirs remain under the three platforms' leaves.

One naming note, because it cost a gate round-trip:
`check-example-leaf-target-dirs` recognises a derived flag only through a
LOWERCASE `*tdir_flag` variable, which is the in-tree convention
(`nros_example_tdir_flag`, `qemu_tdir_flag`, `nx_tdir_flag`). Uppercase names are
invisible to it, so it reported a converted site as unconverted. Renamed rather
than widening the gate.

### Still open after this pass

* residue 1: `qemu-smoltcp-bridge`, `rtic-run-plan-e2e`, `ros-edition-pose-pub`
  (each needs its resolver moved with it).
* residue 2: `just/ros-editions.just` `build-e2e-fixtures`
  (`target-ros-edition-<distro>-<rmw>/`), and `just/px4.just` — px4 is NOT a
  migrated platform, so its authored dir is correct today.
* residue 3: `scripts/stack-analysis.sh` (takes a directory argument, so it has
  no coordinate; wants a `--target-dir` of its own).
* residue 4: NuttX in-source objects — a NuttX build-system change, the one
  residue with no `--target-dir`-shaped fix.


## Status (2026-08-12, third pass) — residues 1, 2 and 3 are DONE

Every cargo-shaped residue in this issue is fixed. What is left is residue 4,
which is not cargo-shaped.

| residue | site | disposition |
| --- | --- | --- |
| 1 | wake-latency pair | `[[fixture]]` row + resolver on the row (second pass) |
| 1 | `rtic-run-plan-e2e` | already had a row; build + `-kernel` now use it |
| 1 | `qemu-smoltcp-bridge` | `[[fixture]]` row; both ad-hoc builds deleted |
| 1 | `ros-edition-pose-pub` | derived root, and it GAINED a coordinate (below) |
| 2 | freertos / threadx-linux / threadx-riscv64 run-paths | shared group (second pass) |
| 2 | `fixture-make-driver.sh` cyclonedds | routed through the manifest (first pass) |
| 2 | `ros-editions` `build-e2e-fixtures` | derived root per (edition, rmw) |
| 2 | `px4` | correct as-is — px4 is NOT a migrated platform |
| 3 | `stack-analysis.sh` | own derived root under `$NROS_BUILD_ROOT` |

`check-example-leaf-target-dirs` is OK, and a sweep for `cargo build` in `just/`
and `scripts/` now finds only builds that are out of scope by construction (a
vendored tool's own tree, workspace-root builds, the CLI bootstrap).

**Two bugs the hand-spelled paths were hiding**, both found by moving them:

* `rtic-run-plan-e2e` had a manifest row all along, so the lane compiled it into
  the group while the recipe built a second copy in the leaf and the test
  `-kernel`ed THAT — running an artifact the freshness gate never saw.
* `ros-edition-pose-pub` is rebuilt PER EDITION (its `generated/` messages are
  regenerated against that edition's defs) and was built into one leaf `target/`
  by a consumer that never mentioned the edition. Every edition overwrote the
  last; the test ran whichever ran most recently while believing otherwise. The
  coordinate now exists on both sides — the old path had no room for it.

Not everything joined a group, deliberately: the ros-editions builds regenerate
`generated/` per edition and `stack-analysis.sh` builds with
`-Z emit-stack-sizes` on nightly, so both take a dedicated
`<root>/<kind>/<coordinate>` instead. Sharing a cargo dir would have been the
wrong fix for a real reason, not a missing one.

## Residue 4 is what remains — and the "no fix applies" reading was WRONG

**Correction (2026-08-13).** This issue said, twice, that residue 4 has no
`--target-dir`-shaped fix because "NuttX's apps `Makefile` decides this, not a
nano-ros recipe". Reading `Application.mk` rather than reasoning about it shows
the opposite: **upstream documents an out-of-tree hook, and the file that must
set it is ours.**

```make
# third-party/nuttx/nuttx-apps/Application.mk
# Apps compilation can achieve out-of-tree intermediate products
# by specifying "PREFIX" to a directory in its own Makefile.
# Make sure the out-of-tree directory exists and ends with $(DELIM) when setting it.
PREFIX ?=
SUFFIX ?= $(subst $(DELIM),.,$(CWD))

COBJS  = $(CSRCS:%=$(PREFIX)%$(SUFFIX)$(OBJEXT))
$(PREFIX).built: $(AROBJS)
$(PREFIX).depend: …        # and $(PREFIX)Make.dep
```

That accounts for both observed properties exactly, and covers all four artifact
kinds the symptom listed:

* `PREFIX` is EMPTY, so objects, `.built`, `.depend` and `Make.dep` are written
  relative to `$(CWD)` — and `$(CWD)` is our source leaf, because
  `stage-external-apps.sh` symlinks the app dir into `apps/external/`.
* `SUFFIX` is `$(CWD)` with separators turned into dots, which is why the
  absolute build path ends up in the object NAME
  (`main.c.home.aeon.repos.nano-ros.examples.qemu-arm-nuttx.c.talker.o`).

`PREFIX` is `?=`, and the Makefiles that `include $(APPDIR)/Application.mk` are
`integrations/nuttx/Makefile` and
`integrations/nuttx/apps-external-template/Makefile` — both tracked here. So the
fix is the same shape as every other residue: set
`PREFIX := <build root>/nuttx-apps/<coordinate>/` (it must exist and end with the
delimiter) from `nros_build_dir`, exactly as residues 2 and 3 now do.

Two things to get right, which is why this is not being done in the same pass as
the cargo residues:

* **`PREFIX` moves WHERE, not the NAME.** The mangled `$(CWD)` suffix stays
  unless `SUFFIX` is also overridden — and that mangling is load-bearing
  upstream: it is what keeps objects from different app dirs unique when they are
  archived into one `libapps.a`. Overriding it needs a coordinate that is unique
  per app dir, not merely shorter.
* The out-of-tree dir must exist before `make` runs, so the staging script has to
  create it, and `make clean`'s `DELFILE .built` / `Make.dep` calls need to
  resolve to the new location too.

Interim disposition unchanged until that lands: the files are deleted, four
scoped patterns sit in the repo-root `.gitignore` tagged as a symptom ledger, and
the ledger is deleted when the build moves out of tree.

This issue stays open for residue 4 alone.
