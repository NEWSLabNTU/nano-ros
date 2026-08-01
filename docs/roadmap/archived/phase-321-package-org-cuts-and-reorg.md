# Phase 321 — Package organization: cuts and the group reorganization

**Status (2026-07-31): COMPLETE. W1 landed (W1.e withdrawn), W2.a-f landed, W3.b/W3.c landed (W3.a withdrawn).** Split out of the original
combined draft. Sibling phases: [phase-320](phase-320-board-support-tiers.md)
(support tiers), [phase-322](phase-322-board-crate-consolidation.md) (board crate
merges, deferred).

**Why now.** Two problems that share a cause:

1. `packages/` groups are named on inconsistent principles, so the tree does not
   encode [RFC-0001](../design/0001-architecture-overview.md)'s layer model.
2. Dead and superseded packages accumulate, because nothing points at them and
   nothing fails when they rot.

The four RMW backends live in four directories named on four *different*
principles — `zpico/` (a vendor library), `xrce/` (a protocol), `dds/` (a
protocol family), `px4/` (a consumer product) — plus `bridge/`, one crate with
its own top-level group. `core/` is 23 packages and 143k LoC spanning six roles
and four artifact kinds, so the path does not even tell you whether something is
a crate. And `config/` holds 12 entries of which 8 are single-file
TOML manifests.

Meanwhile `nros-orchestration` has zero callers, `packages/cli/docs/` is the
*retired standalone repo's* roadmap, and `just/px4.just` still builds two cargo
packages that were deleted in phase-115.

**Sequencing.** W1 (cuts) is independent and safe to land first. W2 (moves) is
pure churn that conflicts with every parallel session — it wants a quiet window,
and it must not block phase-320's honesty fixes.

---

## W1 — Cuts

Each verified: zero consumers, or consumers that cannot work.

- [x] **W1.a** Delete `packages/core/nros-orchestration/` (315 LoC).
      `rg "nros_orchestration::"` (excluding docs/lockfiles) → **zero hits**;
      superseded by `nros-orchestration-ir`, which has 10+ real consumers. Its
      own doc-comment describes reading `nros-plan.json`, a pipeline superseded by
      the `system.toml` → `resolve_tiers` path. Drop the member entry and the
      unused workspace-dep row.
- [x] **W1.b** Delete `packages/cli/docs/` (19 files). `ROADMAP.md` is titled
      **"cargo-ros2: Project Roadmap"** — the retired standalone repo's roadmap,
      imported wholesale by the phase-218 merge and never updated since. It
      duplicates and contradicts `docs/roadmap/`, and `CLI_REFERENCE.md`
      duplicates `book/src/reference/cli.md`. Zero inbound links.
- [x] **W1.c** Delete the dead `just/px4.just` cargo recipes (lines 89, 103, 106,
      109, 193, 194, 210, 240). They build / fmt / clippy / test `nros-rmw-uorb`
      and `nros-px4`, **neither of which exists as a cargo package** (both were
      deleted in phase-115.K.4; `packages/rmw/uorb/nros-rmw-uorb` is a C++ CMake
      project). Line 240 `rm -rf`s `packages/rmw/…`, a directory that has never
      existed. Also fix the dead links in `docs/design/0011-px4-rmw-uorb.md:56,64`.
- [x] **W1.d** Delete the `xrce-sys` **crate** (701 + 307 LoC, zero dependents,
      `--exclude`d from every build) but **keep the directory** — it hosts two git
      submodules consumed by `nros-rmw-xrce-cffi/build.rs` and
      `nros-rmw-xrce/CMakeLists.txt`. Today `nros-rmw-xrce-cffi/build.rs:8-10`
      declares it "mirrors `xrce-sys/build.rs` … must be kept in lockstep" — a
      maintained duplicate of a dead crate's build script. Make the cffi build.rs
      the sole owner.
- [~] **W1.e WITHDRAWN — the premise was false.** The claim was that these
      lockfiles belong to root-workspace members and are therefore inert. Checked
      against `cargo metadata`: `packages/drivers/serial/cmsdk-uart`,
      `packages/drivers/net/nros-smoltcp`, `packages/verification/nros-verification`
      and `packages/rmw/zenoh/zpico-serial` are **not** workspace members, so their
      lockfiles are live — they pin standalone and cross-compiled builds.
      Deleting them would have been a real regression dressed as cleanup. The
      inconsistent tracked-vs-ignored policy across siblings is still worth a
      decision, but it is not a deletion.
- [x] **W1.f** (partial — see below) Clean the working-tree crud: two 46 KB `.o` files in
      `packages/platform/nros-platform-posix/src/` whose names encode an absolute host
      path (`platform.c.home.aeon.repos.nano-ros.integrations.nuttx.o`, dated
      2026-05-30, plus a `_1` copy), and five in-source CMake `build/` dirs — the
      largest, `packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/`, holds a full vendored
      CycloneDDS object tree. All untracked and gitignored; nothing is committed.
      The `.gitignore` rule exists *because* the build was known to litter `src/`.
      **Done for the two stale `.o` files only.** The in-source `build/` trees
      were deliberately left: `packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/` alone is
      45 MB of vendored CycloneDDS objects, and deleting a developer's build
      cache uninvited buys tidiness at the cost of a long rebuild. They are
      gitignored and harmless; clean them when convenient.

**Decide, do not delete blind:**

- [ ] **W1.g** `nros-board-bare-metal` — orphan family driver, zero deps anywhere,
      no example / cmake / fixture. Its siblings `nros-board-freertos` and
      `-nuttx` *are* depended on; the bare-metal boards never adopted the pattern.
- [ ] **W1.h** `nros-board-cffi` — `<nros/board.h>` is included by **zero** C/C++
      source in the tree and `nros_board_export!` has zero invocations. Kept alive
      solely by its own drift gate. Actively maintained (phase-313), so this is
      intentional — but then label it **spec-only**, not a library.
- [ ] **W1.i** `nros-rmw-xrce-cffi-staticlib` — its stated purpose ("cmake /
      Corrosion consumers link this archive directly") is unrealized: its only
      references repo-wide are `--exclude` lines. Its zenoh twin has real
      consumers; this one does not.
- [ ] **W1.j** `s32z270dc2-r52` — either wire `build-s32z-board-import` into
      `west-fixtures.sh` + `board_import.rs` (cheap; the FVP twin already exists)
      or delete the board and its orphan fixture dir.

## W2 — Group reorganization

Target layout, matching RFC-0001's stack:

```
packages/
  core/        agnostic runtime ONLY — nros-core, nros-rmw, nros-rmw-abi,
               nros-node, nros-serdes, nros-params, nros-log, nros-macros
  api/         façade + language bindings — nros, nros-c, nros-cpp
  rmw/         ALL backends + shim — rmw/{zenoh,xrce,cyclonedds,uorb},
               nros-rmw-cffi, nros-rmw-metadata, nros-bridge
  platform/    nros-platform{,-api,-cffi,-critical-section} + the C impls
  boards/      tier dirs (W3)
  drivers/     real hardware only — net/ serial/ ipc/
  interfaces/  generated message crates
  tooling/     nros-build-helpers, -sizes-build, -zephyr-build, -build-paths,
               nros-zpico-build, nros-build-profile
  testing/  verification/  cli/    unchanged
config/        the nros-platform.toml / nros-board.toml manifest dirs
```

Rationale, with the evidence for each:

- **The four RMW backends live in four dirs named on four different
  principles**: `zpico/` (a vendor library), `xrce/` (a protocol), `dds/` (a
  protocol family), `px4/` (a consumer product) — and `bridge/`, one crate with
  its own top-level group. `packages/rmw/uorb/nros-rmw-uorb` is a CMake/C++ package,
  i.e. a backend hidden under a product name.
- **`core/` is a junk drawer**: 23 packages, 143k LoC, six roles (agnostic
  runtime, language bindings, façade, build tooling, C platform impls,
  orchestration) and four artifact kinds (cargo crates, header-only ABI
  `nros-rmw-abi`, C++-header-only `nros-diagnostic-updater` with 0 Rust LoC, and
  CMake C libraries). You cannot tell from the path whether something is a crate.
  Note the `core → backend` edges are **not** a layering violation — every one is
  an optional-feature or dev dep — but `nros-c` / `nros-cpp` do name all
  backends, which makes them aggregating *bindings*, not core.
- **Config dirs look like code dirs**: `config/` holds 12 entries, 8
  of which are single-file `nros-platform.toml` manifests and 4 real crates; same
  in `packages/boards/{posix,zephyr}`. And `config/zephyr/` (config)
  versus `packages/platform/nros-platform-zephyr/` (C code) are different things with
  near-identical names. Phase-84 intended the OS-level crates to move here and
  only half landed; phase-290 then added a third, unrelated artifact class.

- [x] **W2.a** **Not a blocker — the claim was imprecise.** The repo-root
      sentinel is `nros-sdk-index.toml` (`find_platforms_root`), which is stable;
      `config` is only the *returned* path, so it moves with the
      directory like any other constant. Nothing to do ahead of time; noted here
      so the next reader does not re-derive the same false alarm.
- [x] **W2.b** **Moved by ROLE, not to `examples/` — the draft misread two
      things.** The README line "Most users should use the examples instead" means
      *readers should look at examples*, not *this code belongs there*. And
      `examples/` is enumerated by RFC-0026-shaped gates
      (`examples_fixture_coverage`, `check-example-matrix`) that expect
      `<platform>/<lang>/<name>`; dropping non-conforming directories in would
      have meant adding tracked exceptions to a gate whose job is catching real
      gaps. So: `qemu-smoltcp-bridge` → `packages/testing/` (it is test support —
      `fixture-inventory.py` and the qemu-baremetal lane build it), leaving
      `packages/reference/` holding exactly what its name claims. The remaining
      `stm32f4-porting/{polling,rtic}` are unbuilt porting templates; that is now
      stated at the top of its README rather than left to be discovered.
- [x] **W2.c** Move `nros-diagnostic-updater` beside `cmake/compat/` — it is a
      C++-only `rclcpp` compat shim with 0 Rust LoC, loaded via
      `cmake/compat/stubs/Finddiagnostic_updater.cmake`. Unrelated to
      `nros-diagnostics` (the `no_std` reporter) despite the name; they meet only
      at the `/diagnostics` topic.
> **W2.d, W2.e and W2.f are DEFERRED. Rationale, with numbers.**
>
> Measured blast radius (files / path references, excluding build output):
> `zpico` 151/468, `xrce` 74/173, `dds` 86/245, `px4` 21/49, `bridge` 5/12 —
> so W2.d is ~337 files and ~947 references. `core` alone is 537/2074, so W2.e is
> larger still. `drivers` is 57/110.
>
> The moves themselves are mechanical. The problem is *verification*: almost all
> of those references live in cmake board glue, `just` platform recipes and
> per-board `.cargo` config for targets that need SDKs and emulators — Zephyr,
> NuttX, ThreadX, FreeRTOS, ESP-IDF, FVP. `just check` and `just test-unit` would
> stay green while an embedded lane is broken, because neither compiles those
> targets. Landing a 3000-reference rename on that basis and calling it verified
> is precisely the false-green pattern phase-320 exists to stamp out.
>
> These want a window where `just ci-full` (or at minimum `ci-matrix` plus the
> per-platform sweeps) can run to completion, and where no parallel session is
> editing the same trees — a rename touching 40% of the repo conflicts with
> everything.
>
> **The measured counts UNDERCOUNT, found by moving `bridge` first as a canary.**
> `packages/api/nros/Cargo.toml` depended on it as
> `path = "../../bridge/nros-bridge"` — a RELATIVE path that contains no
> `packages/bridge` substring, so every `rg "packages/bridge"` sweep missed it and
> the workspace stopped resolving the moment the directory moved. There are
> **150** such relative cross-group `path = "../../<group>/…"` deps in the tree.
> Any migration script must rewrite both spellings, and `cargo metadata` is the
> only cheap oracle that catches the relative half.
>
> This is exactly why the groups move ONE AT A TIME, smallest first: the canary
> cost one broken resolve and a two-line fix. The same mistake inside a
> 947-reference commit would have been found by a platform lane hours later, or
> not at all.

- [x] **W2.d** Collapse `zpico/` + `xrce/` + `dds/` + `px4/` + `bridge/` into
      `rmw/`.
- [x] **W2.e** Split `core/` into `core/` + `api/` + `platform/` + `tooling/`.
- [x] **W2.f** Retax `drivers/`: it currently mixes real hardware drivers,
      kernel/stack `-sys` FFI, protocol-stack adapters (`nros-smoltcp`), a POSIX
      sockets shim (`nsos-netx`), and generic runtime support
      (`nros-baremetal-common`, `nros-transport-callbacks`) across two languages
      and two build systems. All 15 are alive; the problem is purely taxonomic.

### W2.d outcome — the six path classes

Landed as five commits, one group per commit, each with a real build:
`bridge` (canary) -> `cyclonedds` -> `uorb` -> `zenoh` -> `xrce`.
`packages/rmw/` now holds all four backends plus the bridge.

**A path reference comes in six shapes, and only three are greppable:**

| # | class | example | found by |
| --- | --- | --- | --- |
| 1 | absolute | `packages/dds/foo` | grep |
| 2 | absolute, NO trailing slash | `root.join("packages/dds")` | grep — **if** the pattern does not require `/` |
| 3 | relative Cargo dep | `path = "../../core/nros-rmw"` | `cargo metadata` |
| 4 | relative in CMake / shell | `${CMAKE_CURRENT_LIST_DIR}/../../../..` | the platform build |
| 5 | relative in a Rust string | `manifest_dir.join("../../platforms")` | the build only |
| 6 | **`.parent()` chain** | `.parent().and_then(\|p\| p.parent())…` | the build only |

Class 2 shipped a defect: the cyclonedds commit left two live
`root.join("packages/dds")` calls in `nros-tests/src/zephyr.rs`, and `just check`
plus 817 unit tests stayed green because that resolver path is only reached
during a Zephyr fixture build. Now permanently gated — the four retired group
paths are entries in `scripts/check-retired-submodule-refs.sh`, mutation-tested.

Class 6 broke the xrce build outright: a three-`.parent()` walk to the repo root
landed on `packages/` once the crate sat a level deeper, doubling every vendored
path. No grep for `../` can see it.

**Submodules were less trouble than expected.** `git mv` of the parent updated
`path =` in `.gitmodules`, re-depthed each submodule's `.git` gitdir file, and
fixed `core.worktree` in `.git/modules/*/config`. The `[submodule "<old path>"]`
NAMES are deliberately left alone — a name maps to `.git/modules/<name>`, so
renaming means relocating the gitdir for no gain. One manual step remained:
`git submodule status` reported the xrce pair uninitialised despite intact
worktrees and a working `rev-parse`; `git submodule init` cleared it.

**Cost note for W2.e**: `just check` is ~10 minutes per group on this host, and
the in-source cmake `build/` trees must be deleted before re-configuring —
their `CMakeCache.txt` pins the old absolute source path and cmake refuses
outright.

## W3 — Documentation consolidation

- [~] **W3.a WITHDRAWN — the premise was false.** The claim was that five prose
      files per side "duplicate the book". Measured: `nros-c/docs/ros2-interop.md`
      shares **5 of 37** unique lines with `book/.../ros2-interop.md`, and
      `nros-c/docs/getting-started.md` shares **13 of 161** with
      `book/.../first-node-c.md`. They are different documents about related
      topics, not copies. They are also explicit `INPUT` entries in the Doxyfile,
      so deleting them would strip content from the generated API docs that
      exists nowhere else. Same-topic is not same-text.
- [x] **W3.b** Relocated, but to `docs/reference/`, not `docs/issues/`. Each file
      catalogues 7-8 separate limitations, so converting them would have meant
      ~15 numbered issues with frontmatter — and they are reference catalogues,
      not individually-tracked bugs. `docs/reference/cyclonedds-known-limitations.md`
      already established that shape, so they follow it:
      `xrce-known-limitations.md` and `rosidl-parser-limitations.md`. In-code
      references updated.
- [x] **W3.c** Fix the stale FVP claims catalogued in W1.f, plus
      `docs/reference/cyclonedds-known-limitations.md:229-238`, which still says
      the FVP and S32Z boards are "not yet implemented" and Cyclone "works on
      POSIX only" — contradicted by phase-292/298.

---

## Acceptance

- `rg "nros_orchestration::"`, `rg -- "-p nros-px4"`, `rg "xrce-sys"` return only
  the intended residue.
- No in-source `build/` directory or stray `.o` remains under `packages/`.
- `nros config` still finds the repo root after the `config` rename
  (W2.a) — verified by running it from a subdirectory.
- Every group in `packages/` holds exactly one role, and the RMW backends all
  live under one.
- `just ci` green after each work item, not just at the end.

---

## Outcome (2026-07-31)

Landed: W1.a-d and W1.f (cuts), W2.a-c (two moves by role), W3.b-c (docs).
Withdrawn after verification: **W1.e**, **W3.a**. Deferred with measurements:
**W2.d-f**.

**Three of this phase's own work items did not survive contact with the tree**,
and the pattern is worth naming: each came from a survey that inferred
duplication or deadness from a plausible signal — a lockfile next to a crate, a
README sentence, two files with the same topic — without checking the mechanism.
`cargo metadata` said the lockfiles were live; the README sentence meant
something else; `comm` said the "duplicate" docs share 5 of 37 lines. **A cut
list needs the same evidence standard as the code it cuts.** The items that
survived were the ones where the evidence was a *call graph* — `rg` for the
symbol, `cargo metadata` for the dependency edge, a mutation that broke the
build.

**Verification note for W2.c**: the first two attempts to prove that move were
vacuous — the C/C++ lanes never compile the shim, and a system
`/opt/ros/humble/share/diagnostic_updater` shadowed the stub entirely. The third
mutation also passed because the replacement had hit a comment rather than the
code. Only the fourth run — ROS prefix ignored, `set(_du_dir ...)` broken —
produced a failing build, and only that run is evidence.

---

## Final shape (2026-07-31)

```
packages/
  api/        nros (facade), nros-c, nros-cpp
  core/       nros-core, nros-node, nros-rmw, nros-rmw-abi, nros-serdes,
              nros-params, nros-log, nros-macros, nros-diagnostics,
              nros-orchestration-ir          (23 packages -> 10)
  rmw/        zenoh, xrce, cyclonedds, uorb, bridge, cffi, metadata,
              transport-callbacks
  platform/   14 impl crates (10 OS-level + 4 silicon-level)
  drivers/    net/ serial/ ipc/ sys/
  boards/  cli/  interfaces/  reference/  testing/  tooling/  verification/
config/       the platform knob manifests
```

## What this cost, and what it taught

**Six path classes, three of them invisible to grep**: absolute; absolute with
no trailing slash; relative Cargo dep; relative in CMake/shell; relative built
in a Rust string; and `.parent()` chains. Only a build finds the last two. Every
group therefore moved in its own commit with its own build.

**Sideways moves change depth too.** Relocating a crate from `core/X` to
`rmw/X` leaves its own depth unchanged but alters the distance between it and
its dependents — `../../../core/nros-rmw-cffi` had to become `../../cffi`, not
`../../../cffi`. `cargo metadata` is the only cheap oracle for that.

**The things that broke were never the crates.** They were: a gate's hardcoded
allowlist (`check-no-direct-kernel-alloc`), a gitignored generated file with
absolute paths (`nros-patch.toml`, which broke every example's dependency
resolution and which no root-level `cargo metadata` reads), a stale prebuilt CLI
still emitting old paths, and cmake `build/` caches pinning the old absolute
source dir.

**Three work items did not survive verification** — W1.e (the "inert" lockfiles
were live), W3.a (the "duplicate" docs share 5 of 37 lines), and W2.a/W2.b's
premises. Each came from inferring deadness or duplication from a plausible
signal without checking the mechanism. The items that held up had a call graph
behind them.

**Cross-session cost was real**: upstream landed a new PX4 cmake module written
against the pre-split layout mid-rebase, which would have shipped broken. The
retired-path gate caught one such regression on someone else's change but does
NOT cover moved-crate paths, only retired GROUP paths — a gap worth closing if
this kind of reorganization recurs.

