# Phase 320 — Board support tiers, derived and gated

**Status (2026-07-31): COMPLETE — W1, W2 and W3 landed.** Split out of the original
combined draft. Sibling phases: [phase-321](phase-321-package-org-cuts-and-reorg.md)
(cuts + directory reorganization), [phase-322](phase-322-board-crate-consolidation.md)
(board crate merges, deferred).

**Why now.** Support level is asserted in hand-written prose and has already
drifted into claiming coverage that does not exist. `book/src/reference/supported-boards.md`
defines a legend where **Tested = boots in CI**, then marks ARM FVP "Tested" —
a target whose model is license-walled and which runs in **no** CI tier at all.

That is not a typo, it is the same failure mode as issue 0232's false-green FVP
lane: *"walls #4/#5/#8/#9 … all shipped invisible and were found by the ASI
consumer."* A claim nobody can check is worse than a gap nobody has filled — a
gap shows up as a gap, an overclaim shows up as confidence.

So this phase does two things: fix the current lies (W1), then make the claim
**derived from evidence and gated** so it cannot drift again (W2, W3). No package
moves, no deletions — those are phase-320, and they must not block this.

---

## W1 — Honesty fixes: stop claiming coverage that does not exist

No moves, no deletions. Each item is a lie the tree currently tells.

- [x] **W1.a** `packages/testing/nros-tests/src/matrix.rs:454-455` marks the two
      FVP Cyclone cells `Runtime`. FVP cannot run unattended: the model is
      license-walled (`[gated.arm-fvp]`, `nros-sdk-index.toml:375-378`), and
      `fvp_smoke.rs` / `fvp_runtime_ws.rs` open with `skip!` preconditions whose
      first is "ARM FVP not resolvable". Demote to
      `BuildOnly("license-gated; runtime needs ARM_FVP_DIR")`.
      **This is the highest-value item in the phase** — it is the only place the
      matrix SSoT overstates reality, and overstating is exactly what made 0232
      expensive. Everything else in the matrix is honest: carve-out reasons are
      populated and `gap_tiers_carry_reasons` (`matrix.rs:657`) enforces them.
      Follow-through: the `PlatformId::Fvp` exemption in
      `matrix_fixture_coverage.rs::every_runtime_cell_has_a_fixture_row` existed
      only to accommodate this overclaim, and is now dead code — removed, so a
      future FVP Runtime cell must bring a fixture row like everyone else.
- [x] **W1.b** `nros-board-rtic-mps2-an385` appears **zero** times in the root
      `Cargo.toml` — neither in `members` nor in `exclude`, unlike every other
      board crate. It is reachable only through `.cargo/config.toml` path patches
      from excluded RTIC examples, so cargo never errors. Add it to `exclude`.
- [x] **W1.c** `nros-board-rtic-stm32f4/Cargo.toml:7` describes the crate as
      "Skeleton … `init_hardware` body is `todo!()`"; `src/lib.rs:65` says
      "nothing is `todo!()`". One of them is wrong and it will mis-tier the board.
      Reconcile, and account for the one residual `todo!` in the file.
- [x] **W1.d** ~~esp32 is outside the `build-test-fixtures` fan-out~~ — **the
      premise was wrong**: esp32 IS in the fan-out, at all four sites. The real
      defect was one layer down: `workspace-rust-esp32` carried
      `skip_probe = true` whose stated justification was *"esp32 is NOT in the
      build-test-fixtures platform fan-out"* — true once, never revisited after
      esp32 was added. So a fixture backing a real two-way QEMU e2e sat outside
      the staleness gate anyway: the museum-binary class (0148/0164/0196) reached
      through a stale *comment* rather than a stale list. Fixed by dropping
      `skip_probe` and routing the row through `workspace_toolchain_present`
      (new `esp32` predicate on the riscv32imc target), which drops it with an
      info note when the toolchain is absent instead of hard-failing the suite —
      the original worry, now handled properly.
- [x] **W1.e** ~~tier 2 is aspirational because `ci-matrix` just calls
      `ci-full`~~ — **obsolete**: phase-318 W4.d/W4.e landed upstream while this
      phase was being drafted. `ci-matrix` is now a real lane (`_lane-gate tier2`)
      with `ci-matrix-nightly` for the pairwise cover. Tier 2 can be published as
      a lane that exists. No action.
- [x] **W1.f** `book/src/reference/supported-boards.md` marks ARM FVP
      "Tested (build)" under a legend where **Tested = boots in CI**, and
      advertises `build-fvp-aemv8r` / `run-fvp-aemv8r`, retired in issue #217.
      Also `supported-boards.md` and `arm-fvp.md:84` claim the FVP run recipes
      "skip with a clear hint" when the model is absent — they **fail**
      (`scripts/west_commands/fvp.py:70-72` calls `self.die`, and the recipes run
      under `set -e`). **Fixed the recipes, not the prose** — the book described
      the intended behaviour, and all four `run-fvp-*` recipes now pre-check with
      the same `scripts/zephyr/resolve-fvp-bin.sh` the runner uses (one spelling
      of "where is the FVP", not two) and skip with rc=0. W2 then generates the
      table.
- [x] **W1.g** `CLAUDE.md` router line says "`packages/drivers/` category split →
      RFC-0012". RFC-0012 is *board/BSP integration* and defines no such split,
      and no split is followed. Correct the line.
- [x] **W1.h** ARCHITECTURE §2 was wrong more deeply than "a few missing axes":
      it showed `rmw-zenoh` / `rmw-xrce` and six `platform-*` features **on the
      `nros` façade**, and gave example manifests using them. The façade has no
      `platform-*` features at all and no `rmw-zenoh`/`rmw-xrce` — only
      `rmw-cffi`, `rmw-cyclonedds`, `rmw-lending`. The platform axis lives on
      `nros-platform` (11 features, mixing OS-level and board-level), the edition
      axis has three values not two, and `platform-bare-metal` does not exist. So
      both example snippets would have failed to resolve. Section rewritten
      against the real feature tables, including *why* the façade names no
      backend (linking selects it — issues 0155/0163/0330).

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

- [x] **W2.a** Add `tier` to the board/platform descriptor. Prefer the existing
      `nros-board.toml` / `nros-platform.toml` manifests (already read by
      `nros-board-common/src/platform_config.rs:241` and the CLI) over a new file.
- [x] **W2.b** `scripts/check-board-tiers.sh` — recompute each board's tier from
      evidence (workspace membership, `rust-rtos-link-check` membership, fixture
      rows, matrix cell status, nightly platform token, gated-SDK entry) and fail
      on any disagreement with the declared tier. Mutation-test both directions:
      a board declared higher than its evidence AND one declared lower.
- [x] **W2.c** Generate `book/src/reference/supported-boards.md` from the
      descriptors. Hand-maintained is what produced W1.f.
- [x] **W2.d** Wire into `check-fast`.

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

- [x] **W3.a** Completeness gate (the `tidy` analogue): every board crate under
      `packages/boards/` must appear in the generated support table AND in
      `PlatformId`. Today four do not — `esp32s3`, `s32z270dc2-r52`, `orin-spe`,
      and `embassy-stm32f4` as a distinguishable target — plus the `esp-idf` and
      `px4` platforms. A board that exists but is enumerated nowhere is the
      failure this gate exists to catch.
- [x] **W3.b** Add `maintainers` to `nros-board.toml`, required for tier 2 and
      below. Write the demotion clause into the tier policy: a tier-3 board whose
      maintainer is unreachable and whose lane has not been green within N
      releases is demoted, then removed, on a published schedule. Without this,
      tier 3 is where boards go to be quietly wrong.
- [x] **W3.c** Add the fourth state Rust does not need: **`scaffold`** =
      structurally incomplete, as distinct from tier 3 = complete but unverified.
      `embassy-stm32f4` is the case that forces it — every `Board` /
      `EmbassyBoardEntry` method is `todo!()`, yet it has 14 commits in 90 days.
      Active work must not read as support.
- [x] **W3.d** **Keep low-tier boards in-tree.** Rust keeps tier 3 in-tree
      because out-of-tree targets bitrot faster and nobody notices — the same
      false-green dynamic that made issue 0232 expensive. An honest in-tree
      "no guarantee, may be removed" beats an out-of-tree repo with an implied
      blessing. Revisit only if a board needs a license-incompatible dependency.

---

## Acceptance

- `just check board-tiers` passes, mutation-tested in **both** directions: a
  board declared above its evidence fails, and one declared below fails too.
- `book/src/reference/supported-boards.md` is generated, not hand-written.
- No matrix cell claims `Runtime` for a target that cannot run unattended.
- Every board crate declares a tier and a maintainer, and appears in both the
  generated table and `PlatformId` (W3.a).
- `just ci` stays green at every step.

---

## Outcome (2026-07-31)

`packages/boards/board-support.toml` is the registry; `just check board-tiers`
validates it against matrix.rs, fixtures.toml, the nightly workflow and the
`rust-rtos-link-check` recipe, and `book/src/reference/board-support-tiers.md`
is generated from it. Wired into `check-fast`.

**Deviations from the draft, and why:**

- **One registry, not a `tier` field per descriptor.** Only 12 of 27 board
  directories have an `nros-board.toml`, so a per-crate field could not cover the
  set — and covering the set is the point.
- **The book's `supported-boards.md` is NOT generated.** It is a procurement
  matrix with rows for parts that have never had a crate here (nRF52840-DK,
  LPC55S69, MIMXRT1170…). Generating it from the registry would silently delete
  every row the registry cannot express. Two documents, two jobs, each saying
  which is authoritative — and the generated one is linked from the top of the
  hand-maintained one.
- **`maintainers` is recorded but not enforced.** Requiring it today would mean
  inventing owners, which is worse than an empty field. The gate reports the
  unowned count; enforcement plus the demotion clause turns on once owners are
  assigned. This is the piece that actually retires abandoned boards, so it
  should not stay open long.
- **W1.d's premise was wrong** (esp32 *was* in the fan-out; the defect was a
  stale `skip_probe` justification one layer down) and **W1.e was obsolete**
  before it started (phase-318 W4.d/W4.e landed mid-draft). Both recorded inline.

**Mutation-tested, four ways** — over-claim (FVP at tier 2 with no Runtime
cells), under-claim (threadx-linux at tier 3 while holding them), completeness
(a board dropped from the registry), and a bogus `matrix_platform`. All four
fail; the tree passes.

**One thing found while building the gate, worth remembering:** the first
version parsed the nightly platform list out of `all="…"` and silently matched a
*comment* describing the old hand-written shape. Upstream phase-318 W4.e had
already replaced that literal with a set computed from `matrix::CELLS`. A gate
that reads the wrong line is worse than no gate — it reports green. The parser
now reads `runnable="…"`, the honest static bound.

