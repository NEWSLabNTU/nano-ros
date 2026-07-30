# Phase 321 — Package organization: cuts and the group reorganization

**Status (2026-07-31): W1 landed (W1.e withdrawn). W2/W3 open.** Split out of the original
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
a crate. And `packages/platforms/` holds 12 entries of which 8 are single-file
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
      deleted in phase-115.K.4; `packages/px4/nros-rmw-uorb` is a C++ CMake
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
      against `cargo metadata`: `packages/drivers/cmsdk-uart`,
      `packages/drivers/nros-smoltcp`, `packages/verification/nros-verification`
      and `packages/zpico/zpico-serial` are **not** workspace members, so their
      lockfiles are live — they pin standalone and cross-compiled builds.
      Deleting them would have been a real regression dressed as cleanup. The
      inconsistent tracked-vs-ignored policy across siblings is still worth a
      decision, but it is not a deletion.
- [x] **W1.f** (partial — see below) Clean the working-tree crud: two 46 KB `.o` files in
      `packages/core/nros-platform-posix/src/` whose names encode an absolute host
      path (`platform.c.home.aeon.repos.nano-ros.integrations.nuttx.o`, dated
      2026-05-30, plus a `_1` copy), and five in-source CMake `build/` dirs — the
      largest, `packages/dds/nros-rmw-cyclonedds/build/`, holds a full vendored
      CycloneDDS object tree. All untracked and gitignored; nothing is committed.
      The `.gitignore` rule exists *because* the build was known to litter `src/`.
      **Done for the two stale `.o` files only.** The in-source `build/` trees
      were deliberately left: `packages/dds/nros-rmw-cyclonedds/build/` alone is
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
  its own top-level group. `packages/px4/nros-rmw-uorb` is a CMake/C++ package,
  i.e. a backend hidden under a product name.
- **`core/` is a junk drawer**: 23 packages, 143k LoC, six roles (agnostic
  runtime, language bindings, façade, build tooling, C platform impls,
  orchestration) and four artifact kinds (cargo crates, header-only ABI
  `nros-rmw-abi`, C++-header-only `nros-diagnostic-updater` with 0 Rust LoC, and
  CMake C libraries). You cannot tell from the path whether something is a crate.
  Note the `core → backend` edges are **not** a layering violation — every one is
  an optional-feature or dev dep — but `nros-c` / `nros-cpp` do name all
  backends, which makes them aggregating *bindings*, not core.
- **Config dirs look like code dirs**: `packages/platforms/` holds 12 entries, 8
  of which are single-file `nros-platform.toml` manifests and 4 real crates; same
  in `packages/boards/{posix,zephyr}`. And `packages/platforms/zephyr/` (config)
  versus `packages/core/nros-platform-zephyr/` (C code) are different things with
  near-identical names. Phase-84 intended the OS-level crates to move here and
  only half landed; phase-290 then added a third, unrelated artifact class.

- [ ] **W2.a** **Blocker, do first:** `nros-cli-core/src/cmd/config.rs:147` uses
      the literal path `packages/platforms` as the **repo-root sentinel**.
      Renaming that directory breaks `nros config` silently. Replace the sentinel.
- [ ] **W2.b** Move `packages/reference/*` → `examples/`. Its own README says
      "Most users should use the examples instead"; `stm32f4-porting/` is not a
      workspace member and **nothing ever builds it** (`just/native.just:410`
      only runs `size` on a prebuilt binary `|| echo "build failed"`).
- [ ] **W2.c** Move `nros-diagnostic-updater` beside `cmake/compat/` — it is a
      C++-only `rclcpp` compat shim with 0 Rust LoC, loaded via
      `cmake/compat/stubs/Finddiagnostic_updater.cmake`. Unrelated to
      `nros-diagnostics` (the `no_std` reporter) despite the name; they meet only
      at the `/diagnostics` topic.
- [ ] **W2.d** Collapse `zpico/` + `xrce/` + `dds/` + `px4/` + `bridge/` into
      `rmw/`.
- [ ] **W2.e** Split `core/` into `core/` + `api/` + `platform/` + `tooling/`.
- [ ] **W2.f** Retax `drivers/`: it currently mixes real hardware drivers,
      kernel/stack `-sys` FFI, protocol-stack adapters (`nros-smoltcp`), a POSIX
      sockets shim (`nsos-netx`), and generic runtime support
      (`nros-baremetal-common`, `nros-transport-callbacks`) across two languages
      and two build systems. All 15 are alive; the problem is purely taxonomic.

## W3 — Documentation consolidation

- [ ] **W3.a** `packages/core/nros-c/docs/` and `nros-cpp/docs/` each hold 7
      files; five per side are prose duplicating the book (`c-api.md`,
      `first-node-c.md`, `ros2-interop.md`, `environment-variables.md`,
      `troubleshooting-first-10-min.md` and the C++ equivalents). Keep only
      `mainpage.md` + `groups.dox` next to the headers — Doxygen requires that —
      and de-duplicate the prose into `book/`. `nros-platform-cffi/docs/` and
      `nros-rmw-cffi/docs/` are already at the right scale (one `mainpage.md`);
      make the other two match.
- [ ] **W3.b** Relocate `packages/xrce/nros-rmw-xrce/KNOWN-LIMITATIONS.md` and
      `packages/cli/rosidl-codegen/PARSER_LIMITATIONS.md` into `docs/issues/`
      per the CLAUDE.md docs convention.
- [ ] **W3.c** Fix the stale FVP claims catalogued in W1.f, plus
      `docs/reference/cyclonedds-known-limitations.md:229-238`, which still says
      the FVP and S32Z boards are "not yet implemented" and Cyclone "works on
      POSIX only" — contradicted by phase-292/298.

---

## Acceptance

- `rg "nros_orchestration::"`, `rg -- "-p nros-px4"`, `rg "xrce-sys"` return only
  the intended residue.
- No in-source `build/` directory or stray `.o` remains under `packages/`.
- `nros config` still finds the repo root after the `packages/platforms` rename
  (W2.a) — verified by running it from a subdirectory.
- Every group in `packages/` holds exactly one role, and the RMW backends all
  live under one.
- `just ci` green after each work item, not just at the end.
