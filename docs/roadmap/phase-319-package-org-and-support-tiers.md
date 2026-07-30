# Phase 319 — Package organization, board support tiers, and the cut list

**Status (2026-07-30): drafted, nothing landed.** Informs a follow-up RFC if the
group reorganization (W5) is accepted; W1–W2 stand alone and need no RFC.

**Why now.** Three complaints that turn out to be one problem:

1. `packages/` groups are named on inconsistent principles, so the tree does not
   encode [RFC-0001](../design/0001-architecture-overview.md)'s layer model.
2. Support level is asserted in hand-written prose and has already drifted into
   claiming coverage that does not exist.
3. Dead and superseded packages accumulate because nothing points at them and
   nothing fails when they rot.

All three share a root cause: **structure and status are written down by hand
instead of derived and gated.** That is the same failure mode as issue 0232's
false-green FVP lane (a license-walled test that always skipped, so four
walls "shipped invisible and were found by the ASI consumer") and issue 0341's
matrix divergence. This phase fixes the instances AND the mechanism.

Sequencing is deliberate: **W1 first (truth, no churn), W2 second (the gate that
keeps it true), cuts third, moves last.** The directory moves are pure churn that
conflicts with every parallel session, so they must not block the honesty fixes.

---

## W1 — Honesty fixes: stop claiming coverage that does not exist

No moves, no deletions. Each item is a lie the tree currently tells.

- [ ] **W1.a** `packages/testing/nros-tests/src/matrix.rs:454-455` marks the two
      FVP Cyclone cells `Runtime`. FVP cannot run unattended: the model is
      license-walled (`[gated.arm-fvp]`, `nros-sdk-index.toml:375-378`), and
      `fvp_smoke.rs` / `fvp_runtime_ws.rs` open with `skip!` preconditions whose
      first is "ARM FVP not resolvable". Demote to
      `BuildOnly("license-gated; runtime needs ARM_FVP_DIR")`.
      **This is the highest-value item in the phase** — it is the only place the
      matrix SSoT overstates reality, and overstating is exactly what made 0232
      expensive. Everything else in the matrix is honest: carve-out reasons are
      populated and `gap_tiers_carry_reasons` (`matrix.rs:657`) enforces them.
- [ ] **W1.b** `nros-board-rtic-mps2-an385` appears **zero** times in the root
      `Cargo.toml` — neither in `members` nor in `exclude`, unlike every other
      board crate. It is reachable only through `.cargo/config.toml` path patches
      from excluded RTIC examples, so cargo never errors. Add it to `exclude`.
- [ ] **W1.c** `nros-board-rtic-stm32f4/Cargo.toml:7` describes the crate as
      "Skeleton … `init_hardware` body is `todo!()`"; `src/lib.rs:65` says
      "nothing is `todo!()`". One of them is wrong and it will mis-tier the board.
      Reconcile, and account for the one residual `todo!` in the file.
- [ ] **W1.d** `nros-board-esp32-qemu` has a real two-way QEMU e2e
      (`esp32_emulator.rs`, 8 tests) but sits **outside** the `build-test-fixtures`
      fan-out (`justfile:1100-1101,1126`), so it silently escapes the fixture
      staleness gate — the museum-binary class (issues 0148/0164/0196). Add it.
- [ ] **W1.e** `just ci-matrix` prints a note and calls `just ci-full`
      (`justfile:1451-1456`); tier-2 cell selection exists (`ci_lane.rs`) but is
      not wired to the nextest filter (phase-318 W4.d). Until that lands, **any
      published "tier 2" is aspirational** — say so in the tier doc rather than
      implying a lane exists.
- [ ] **W1.f** `book/src/reference/supported-boards.md` marks ARM FVP
      "Tested (build)" under a legend where **Tested = boots in CI**, and
      advertises `build-fvp-aemv8r` / `run-fvp-aemv8r`, retired in issue #217.
      Also `supported-boards.md` and `arm-fvp.md:84` claim the FVP run recipes
      "skip with a clear hint" when the model is absent — they **fail**
      (`scripts/west_commands/fvp.py:70-72` calls `self.die`, and the recipes run
      under `set -e`). Fix the text or the recipes; W2 then generates this table.
- [ ] **W1.g** `CLAUDE.md` router line says "`packages/drivers/` category split →
      RFC-0012". RFC-0012 is *board/BSP integration* and defines no such split,
      and no split is followed. Correct the line.
- [ ] **W1.h** ARCHITECTURE §2's feature-axis diagram omits `rmw-cyclonedds`,
      `rmw-uorb` and `platform-esp-idf`, all of which exist. Align it with W2's
      generated axes rather than hand-editing again.

## W2 — Support tiers, derived and gated

The existing vocabulary (`Tested` / `Ready` / `Untested` in the book) failed
because it is prose. Tiers become a **field on the matrix SSoT** with a gate
asserting the declared tier matches evidence — the same lockstep shape as
`scripts/check-rmw-required-slots.sh` (issue 0349), which is now the house
pattern for "two things that must agree".

Definitions, all mechanically checkable:

| Tier | Predicate | Promise |
| --- | --- | --- |
| **1 — Supported** | `just ci` compiles it (link-check) **and** ≥1 asserted runtime test **and** a nightly GitHub lane | A regression fails before merge |
| **2 — Verified** | Asserted runtime test + fixture rows, nightly lane, but not in `just ci` | Works; breakage found within a day |
| **3 — Build-only** | Compile-proof only; hardware or gated SDK blocks running | It compiles. Nobody has booted it recently |
| **S — Scaffold** | No lane, no fixture, no matrix cell | Explicitly NOT supported |

- [ ] **W2.a** Add `tier` to the board/platform descriptor. Prefer the existing
      `nros-board.toml` / `nros-platform.toml` manifests (already read by
      `nros-board-common/src/platform_config.rs:241` and the CLI) over a new file.
- [ ] **W2.b** `scripts/check-board-tiers.sh` — recompute each board's tier from
      evidence (workspace membership, `rust-rtos-link-check` membership, fixture
      rows, matrix cell status, nightly platform token, gated-SDK entry) and fail
      on any disagreement with the declared tier. Mutation-test both directions:
      a board declared higher than its evidence AND one declared lower.
- [ ] **W2.c** Generate `book/src/reference/supported-boards.md` from the
      descriptors. Hand-maintained is what produced W1.f.
- [ ] **W2.d** Wire into `check-fast`.

Assignment on today's evidence (nightly lanes verified against
`.github/workflows/nightly.yml:99`: `qemu freertos nuttx threadx_linux
threadx_riscv64 esp32` + zephyr):

- **Tier 1** — `native` + `posix`; `mps2-an385-freertos`; `nuttx-qemu-arm`;
  `threadx-linux`; `zephyr`; plus infra `nros-board-common`, `nros-board-cffi`.
  The first four are the only board crates `just ci` compiles, via
  `rust-rtos-link-check` (`justfile:1414,1417,1425`).
  **Caveat to publish with the tier:** `zephyr` is only ever built for
  `native_sim/native/64`. No real Zephyr hardware board is built by anything.
- **Tier 2** — `threadx-qemu-riscv64` (Actions are BuildOnly by wall-clock
  choice); `mps2-an385` + `rtic-mps2-an385` + `bare-metal`; `esp32-qemu`;
  `nuttx-qemu-riscv` — the last with the honest label **"C runtime-proven,
  Rust/C++ build-only"** (its Rust and C++ Pubsub cells are explicit CarveOuts).
- **Tier 3** — `stm32f4` + `rtic-stm32f4` (hardware-gated, #221);
  `fvp-aemv8r-smp` (gated SDK).
- **Scaffold** — `s32z270dc2-r52`; `esp32s3`; `embassy-stm32f4`.

**Do not evict FVP** despite Tier 3: it is the ASI reference consumer's target
(phase-292), a real downstream user the CI evidence cannot see. Tier is a
statement about *verification*, not about *worth*.

**Staleness note.** Git dates are a weak signal in this repo — 2026-07-28 is a
mass-refactor date touching 16 of 27 board crates. **Consumer count and lane
membership are the better proxies**, and by those the real orphans are
`s32z270dc2-r52` (zero cargo consumers, no fixture row, no consuming test, its
one build recipe `just zephyr build-s32z-board-import` has **no caller**, absent
from `PlatformId`) and `esp32s3` (no recipe, no fixture, no test, no matrix cell,
needs an Xtensa toolchain absent from `nros-sdk-index.toml`).
`embassy-stm32f4` has 14 commits in 90 days — it is *actively maintained
scaffolding*, which is precisely why it reads as more supported than it is.

## W3 — Tier as metadata, not as layout (the Rust model)

**Decision: do NOT create tier directories.** The first draft of this phase put
board crates in `boards/tier1|tier2|tier3|scaffold/`. Rust — which has run a
tiered platform-support system at far larger scale for a decade — deliberately
does not do this, and its reasons apply here.

What Rust does instead:

| Mechanism | Rust | Adopt here |
| --- | --- | --- |
| Layout | target specs are **flat**, one per target, no tier grouping | keep `packages/boards/` flat |
| Tier storage | a `tier` field in the target's **metadata**, next to `description` / `std` / `host_tools` | `tier` in `nros-board.toml` (W2.a) |
| Published table | `platform-support.md`, generated and checked | W2.c |
| Completeness | `tidy` fails when a target exists but is documented nowhere | **W3.a** below |
| Tier semantics | T1 = builds **and tests pass** in CI; T2 = guaranteed to **build**; T3 = no guarantee, may be removed | already matches W2's predicates |
| Enforcement | tier 1/2 *is* CI job membership — the tier is a consequence, not a claim | `rust-rtos-link-check` + the nightly platform list already are this |
| **Maintainers** | `target-tier-policy.md` requires a named person per target; unmaintained targets are demoted, then removed on a published schedule | **W3.b** — the piece nano-ros lacks |

Why the directories lose: they buy visibility a generated table already provides,
and charge a path change for every promotion or demotion — ~90 functional
references (56 `*.toml`, 15 `*.rs`, 9 `*.cmake`, 7 `*.sh`, 6 `*.just`) plus ~107
markdown, with **none** of it regenerated by `nros sync` (no board path lives in
a `nros sync`-managed `.cargo/config.toml`). Tier churn should cost one line.

The deeper point: **the scaffold problem is not a layout problem.**
`s32z270dc2-r52` has zero cargo consumers and a build recipe with no caller;
`esp32s3` has no lane at all. Moving them into a `scaffold/` directory would have
left them sitting there just as dead. What actually collects the garbage is a
named owner plus a demotion clause.

- [ ] **W3.a** Completeness gate (the `tidy` analogue): every board crate under
      `packages/boards/` must appear in the generated support table AND in
      `PlatformId`. Today four do not — `esp32s3`, `s32z270dc2-r52`, `orin-spe`,
      and `embassy-stm32f4` as a distinguishable target — plus the `esp-idf` and
      `px4` platforms. A board that exists but is enumerated nowhere is the
      failure this gate exists to catch.
- [ ] **W3.b** Add `maintainers` to `nros-board.toml`, required for tier 2 and
      below. Write the demotion clause into the tier policy: a tier-3 board whose
      maintainer is unreachable and whose lane has not been green within N
      releases is demoted, then removed, on a published schedule. Without this,
      tier 3 is where boards go to be quietly wrong.
- [ ] **W3.c** Add the fourth state Rust does not need: **`scaffold`** =
      structurally incomplete, as distinct from tier 3 = complete but unverified.
      `embassy-stm32f4` is the case that forces it — every `Board` /
      `EmbassyBoardEntry` method is `todo!()`, yet it has 14 commits in 90 days.
      Active work must not read as support.
- [ ] **W3.d** **Keep low-tier boards in-tree.** Rust keeps tier 3 in-tree
      because out-of-tree targets bitrot faster and nobody notices — the same
      false-green dynamic that made issue 0232 expensive. An honest in-tree
      "no guarantee, may be removed" beats an out-of-tree repo with an implied
      blessing. Revisit only if a board needs a license-incompatible dependency.

### W3.e — Board crate merges

Several boards are near-literal forks. **The empirical case for merging is that
the forks have already rotted, twice, in ways nobody noticed:**

- `packages/platforms/nros-platform-esp32s3/src/memory.rs` is **missing the #190
  `foreign_free_count()` fix** its C3 twin has (verified: 2 occurrences in
  `nros-platform-esp32-qemu/src/memory.rs`, 0 in the S3 copy).
- `nros-board-rtic-mps2-an385`'s `qemu_config()` silently diverges from
  `nros-board-mps2-an385`'s `base_config()` on default IP and locator
  (`10.0.2.10` vs `192.0.3.10`) — invisible until runtime.

**The objection that normally blocks these merges does not apply here.** No board
crate emits a reset vector: `#[entry]` is emitted by `nros::main!()` in the Entry
package and dispatched off a **deploy-key string match**
(`nros-macros/src/main_macro.rs:2888,2931`), not off crate identity. Board crates
only re-export the attribute. Verified: `rg '#\[entry\]' packages/boards/` → no
hits. And the board-key→crate mapping is already a table
(`nros-orchestration-ir/src/lib.rs:73`, its own doc calls it "the single source
of truth"), so a merge costs 1–3 string edits and breaks no Entry package.

| Cluster | Now | After | Verdict | Code lines removed |
| --- | --- | --- | --- | --- |
| NuttX qemu arm/riscv + façade | 3 | 1 | **MERGE** — highest confidence | ~1300 |
| ESP32 platform crates | 4 | 3 | **MERGE platforms only** | ~600 |
| STM32F4 plain/rtic/embassy | 4 | 2 | **MERGE** behind features | ~420 |
| MPS2-AN385 | 5 | 4 | **MERGE-PARTIALLY** (`rtic` feature) | ~150 |
| host native/posix | 2 | 1 | **MERGE** | ~114 |
| ThreadX | 3 | 3 | **KEEP SEPARATE** | ~50 |
| infra | 4 | 2–3 | delete `bare-metal` | ~230 |

~27 board crates → ~19, ~2900 code lines deleted.

- [ ] **W3.f** **NuttX — merge `nros-board-nuttx-qemu-{arm,riscv}` + absorb the
      façade.** Verified byte-identical between the two crates: `c/nuttx_run_tiers.c`
      (587 lines), `src/config.rs` (261), `src/entry.rs` (43),
      `c/nuttx_builtins_stub.c` (35) — **926 duplicated lines**. After filtering
      ZST renames and doc rewording, the *entire* semantic difference is one line:
      `SLIRP_DEFAULT_IP` `[10,0,2,30]` vs `[10,0,2,15]`, which the existing
      `DeployOverlay` machinery already overrides. Architecture is already
      externalised into `NUTTX_CROSS` / `NUTTX_PLATFORM_CFLAGS` env, so the crate
      fork buys nothing. Different target triples are not a blocker — one crate
      builds for many. Keep both ZST names as type aliases.
- [ ] **W3.g** **ESP32 — merge the two platform crates** into `nros-platform-esp32`.
      Per-file diff: `libc_stubs.rs` (283 lines), `clock.rs`, `random.rs`,
      `sleep.rs`, `timing.rs` differ by **zero** lines; the only semantic
      difference in 802 lines is `PlatformCriticalSection` (~25 lines, RISC-V
      `csrrci` vs Xtensa `rsil`) — textbook `#[cfg(target_arch)]`. **Keep the two
      BOARD crates separate**: different SoC, different transport (OpenETH+smoltcp
      vs serial-only), different target triples, and S3 has no `BoardEntry` at all.
      One wrinkle: `#![feature(asm_experimental_arch)]` must become
      `#![cfg_attr(target_arch = "xtensa", …)]`.
- [ ] **W3.h** **STM32F4 — one crate, features `rtic` / `embassy`** (mutually
      exclusive, `compile_error!` guard as at `nros-board-mps2-an385/src/node.rs:15`).
      `rtic-stm32f4`'s bringup is two lines delegating to the plain crate. Checked
      and cleared: same target triple, no `panic_handler` in any of the three, the
      `critical-section` feature is a no-op for stm32f4. **One real conflict**:
      `nros-board-embassy-stm32f4` enables `embassy-stm32`'s `memory-x` feature,
      which emits its own `memory.x` while `nros-board-stm32f4/build.rs` writes
      one too — merged, the linker picks by search order, i.e. a non-deterministic
      memory map. Drop `memory-x`; the board's `stm32f4.x` is richer anyway
      (defines `CCMRAM`, `_heap_start`/`_heap_end`). **Alternative honest
      verdict:** `embassy-stm32f4` is a self-declared skeleton that drops the
      peripheral handle (`let _p = embassy_stm32::init(…)`) and has no transport
      bringup — delete it and re-add as a feature when someone wires `embassy_net`.
- [ ] **W3.i** **MPS2-AN385 — fold `rtic-mps2-an385` into `mps2-an385`** behind an
      `rtic` feature; it already depends on the base crate and calls its
      `init_hardware` / `exit_success` / `enable_wfi_idle`. Deletes ~120 duplicated
      lines including a second `mask_to_prefix` and the divergent config defaults.
      **Keep `-freertos` separate** — different linker script, 727-line
      `startup.c`, its own `#[panic_handler]`, a 316-line build.rs compiling
      FreeRTOS+lwIP, and a different `nros_platform_*` symbol provider. Keep
      `mps2-an385-pac` (RTIC's `#[rtic::app(device = …)]` needs a nameable path).
- [ ] **W3.j** **host — fold `nros-board-native` into `nros-board-posix`.**
      `native`'s own doc says it delegates "one-for-one" to `PosixBoard` and that
      "there is nothing exotic about the 'native' target"; `board_path_for` already
      maps **both** keys to the same ZST, so `nros-board-posix` is never named by
      any generated entry. The only addition is a ~5-line `__FORCE_LINK_ZENOH`
      static → a feature. Pure ceremony: a crate existing to satisfy a naming spec.
- [ ] **W3.k** **ThreadX — KEEP SEPARATE, and copy its pattern.** Hard reasons:
      distinct `#[panic_handler]` ownership, distinct startup/trap/syscall C,
      hosted-vs-bare-metal link model, and different network drivers (AF_PACKET
      over veth vs NetX-Duo/virtio-net). This cluster is the model the others
      should follow: `nros-board-threadx` is a **real family driver** whose
      1120-line generic `entry.rs` both boards call into. Same for
      `nros-board-freertos`, where the MPS2 overlay is `pub use
      nros_board_freertos::Config;` and carries zero config code.
- [ ] **W3.l** **Root cause, worth more than any individual merge: there is no
      shared runtime `Config`.** Phase-313 deleted `nros-board-common`'s
      `board_init` module, leaving it a *build-helper* library (2180 of 2252 code
      lines behind `cfg(feature = "build-helpers")`). Result: **12 hand-rolled
      `Config` structs**, at least nine carrying the identical
      `{mac, ip, netmask, gateway, locator, domain_id}` core, and the
      `DeployOverlay`→`Config` merge written out at least four times. Add a shared
      `BaseConfig` + overlay-merge that boards extend. Without it the merges above
      will re-fork.
- [ ] **W3.m** `nros-board-bare-metal` — 161 lines of which **135 are doc comment**
      describing a `DirectExec` family driver **no board opted into**; `mps2-an385`,
      `stm32f4` and `esp32-qemu` each hand-roll `BoardEntry::run` instead. Either
      delete it (W4.g) or — higher value — make those three implement `DirectExec`,
      which absorbs the W3.i duplication too.

## W4 — Cuts

Each verified: zero consumers, or consumers that cannot work.

- [ ] **W4.a** Delete `packages/core/nros-orchestration/` (315 LoC).
      `rg "nros_orchestration::"` (excluding docs/lockfiles) → **zero hits**;
      superseded by `nros-orchestration-ir`, which has 10+ real consumers. Its
      own doc-comment describes reading `nros-plan.json`, a pipeline superseded by
      the `system.toml` → `resolve_tiers` path. Drop the member entry and the
      unused workspace-dep row.
- [ ] **W4.b** Delete `packages/cli/docs/` (19 files). `ROADMAP.md` is titled
      **"cargo-ros2: Project Roadmap"** — the retired standalone repo's roadmap,
      imported wholesale by the phase-218 merge and never updated since. It
      duplicates and contradicts `docs/roadmap/`, and `CLI_REFERENCE.md`
      duplicates `book/src/reference/cli.md`. Zero inbound links.
- [ ] **W4.c** Delete the dead `just/px4.just` cargo recipes (lines 89, 103, 106,
      109, 193, 194, 210, 240). They build / fmt / clippy / test `nros-rmw-uorb`
      and `nros-px4`, **neither of which exists as a cargo package** (both were
      deleted in phase-115.K.4; `packages/px4/nros-rmw-uorb` is a C++ CMake
      project). Line 240 `rm -rf`s `packages/rmw/…`, a directory that has never
      existed. Also fix the dead links in `docs/design/0011-px4-rmw-uorb.md:56,64`.
- [ ] **W4.d** Delete the `xrce-sys` **crate** (701 + 307 LoC, zero dependents,
      `--exclude`d from every build) but **keep the directory** — it hosts two git
      submodules consumed by `nros-rmw-xrce-cffi/build.rs` and
      `nros-rmw-xrce/CMakeLists.txt`. Today `nros-rmw-xrce-cffi/build.rs:8-10`
      declares it "mirrors `xrce-sys/build.rs` … must be kept in lockstep" — a
      maintained duplicate of a dead crate's build script. Make the cffi build.rs
      the sole owner.
- [ ] **W4.e** Delete the ~10 inert `Cargo.lock` files in leaf crates that are
      root-workspace members (`packages/drivers/*`, `packages/verification/*`,
      `packages/zpico/zpico-serial`). Cargo uses the root lock; these are
      pre-workspace leftovers. The tracked/ignored split across otherwise
      identical siblings has no discernible rule (archived issue 0012 fixed this
      once, partly).
- [ ] **W4.f** Clean the working-tree crud: two 46 KB `.o` files in
      `packages/core/nros-platform-posix/src/` whose names encode an absolute host
      path (`platform.c.home.aeon.repos.nano-ros.integrations.nuttx.o`, dated
      2026-05-30, plus a `_1` copy), and five in-source CMake `build/` dirs — the
      largest, `packages/dds/nros-rmw-cyclonedds/build/`, holds a full vendored
      CycloneDDS object tree. All untracked and gitignored; nothing is committed.
      The `.gitignore` rule exists *because* the build was known to litter `src/`.

**Decide, do not delete blind:**

- [ ] **W4.g** `nros-board-bare-metal` — orphan family driver, zero deps anywhere,
      no example / cmake / fixture. Its siblings `nros-board-freertos` and
      `-nuttx` *are* depended on; the bare-metal boards never adopted the pattern.
- [ ] **W4.h** `nros-board-cffi` — `<nros/board.h>` is included by **zero** C/C++
      source in the tree and `nros_board_export!` has zero invocations. Kept alive
      solely by its own drift gate. Actively maintained (phase-313), so this is
      intentional — but then label it **spec-only**, not a library.
- [ ] **W4.i** `nros-rmw-xrce-cffi-staticlib` — its stated purpose ("cmake /
      Corrosion consumers link this archive directly") is unrealized: its only
      references repo-wide are `--exclude` lines. Its zenoh twin has real
      consumers; this one does not.
- [ ] **W4.j** `s32z270dc2-r52` — either wire `build-s32z-board-import` into
      `west-fixtures.sh` + `board_import.rs` (cheap; the FVP twin already exists)
      or delete the board and its orphan fixture dir.

## W5 — Group reorganization

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

- [ ] **W5.a** **Blocker, do first:** `nros-cli-core/src/cmd/config.rs:147` uses
      the literal path `packages/platforms` as the **repo-root sentinel**.
      Renaming that directory breaks `nros config` silently. Replace the sentinel.
- [ ] **W5.b** Move `packages/reference/*` → `examples/`. Its own README says
      "Most users should use the examples instead"; `stm32f4-porting/` is not a
      workspace member and **nothing ever builds it** (`just/native.just:410`
      only runs `size` on a prebuilt binary `|| echo "build failed"`).
- [ ] **W5.c** Move `nros-diagnostic-updater` beside `cmake/compat/` — it is a
      C++-only `rclcpp` compat shim with 0 Rust LoC, loaded via
      `cmake/compat/stubs/Finddiagnostic_updater.cmake`. Unrelated to
      `nros-diagnostics` (the `no_std` reporter) despite the name; they meet only
      at the `/diagnostics` topic.
- [ ] **W5.d** Collapse `zpico/` + `xrce/` + `dds/` + `px4/` + `bridge/` into
      `rmw/`.
- [ ] **W5.e** Split `core/` into `core/` + `api/` + `platform/` + `tooling/`.
- [ ] **W5.f** Retax `drivers/`: it currently mixes real hardware drivers,
      kernel/stack `-sys` FFI, protocol-stack adapters (`nros-smoltcp`), a POSIX
      sockets shim (`nsos-netx`), and generic runtime support
      (`nros-baremetal-common`, `nros-transport-callbacks`) across two languages
      and two build systems. All 15 are alive; the problem is purely taxonomic.

## W6 — Documentation consolidation

- [ ] **W6.a** `packages/core/nros-c/docs/` and `nros-cpp/docs/` each hold 7
      files; five per side are prose duplicating the book (`c-api.md`,
      `first-node-c.md`, `ros2-interop.md`, `environment-variables.md`,
      `troubleshooting-first-10-min.md` and the C++ equivalents). Keep only
      `mainpage.md` + `groups.dox` next to the headers — Doxygen requires that —
      and de-duplicate the prose into `book/`. `nros-platform-cffi/docs/` and
      `nros-rmw-cffi/docs/` are already at the right scale (one `mainpage.md`);
      make the other two match.
- [ ] **W6.b** Relocate `packages/xrce/nros-rmw-xrce/KNOWN-LIMITATIONS.md` and
      `packages/cli/rosidl-codegen/PARSER_LIMITATIONS.md` into `docs/issues/`
      per the CLAUDE.md docs convention.
- [ ] **W6.c** Fix the stale FVP claims catalogued in W1.f, plus
      `docs/reference/cyclonedds-known-limitations.md:229-238`, which still says
      the FVP and S32Z boards are "not yet implemented" and Cyclone "works on
      POSIX only" — contradicted by phase-292/298.

---

## Acceptance

- `just check-board-tiers` passes, and is mutation-tested in **both** directions
  (a board declared above its evidence fails; one declared below fails).
- `book/src/reference/supported-boards.md` is generated, not hand-written.
- No matrix cell claims `Runtime` for a target that cannot run unattended.
- `rg "nros_orchestration::"`, `rg "\-p nros-px4"`, `rg "xrce-sys"` return only
  the intended residue after W4.
- Every board crate declares a tier and a maintainer, appears in the generated
  support table, and appears in `PlatformId` — enforced by W3.a.
- `just ci` stays green at every step; the moves in W3/W5 land in one quiet
  window, not interleaved with other sessions.
