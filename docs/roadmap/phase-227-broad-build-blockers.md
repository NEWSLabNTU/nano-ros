# Phase 227 — Broad Build / Test-All Blockers

**Goal.** Restore a clean `just build-test-fixtures` → `just test-all`
on the maintainer host by clearing the pre-existing breakages that
Phase 226.F validation surfaced. These are independent of the Phase 226
fixture-orchestration changes; 226.F documented them and worked around
the lockfile one to finish its own validation.

**Status.** OPEN. Created 2026-06-08 from the Phase 226.F broad
validation run. None of these are regressions from Phase 226's
orchestration changes; the shared-target-dir + make-driver work is
verified sound (qemu 19 rows build + `just qemu test` 5/5).

**Priority.** P1 for #1 (it blocks every broad build); P2 for the rest
(they fail individual platform leaves, not the whole graph, once the ABI
guard is bypassed).

**Depends on.** Phase 218.J (version bundle bump), Phase 226 (fixture
orchestration — surfaced these).

---

## 1. Stale standalone lockfiles (218.J debt) — primary blocker

**Symptom.** `nros generate-rust` aborts via the `nros-cli-core`
`abi_guard` with `ABI version mismatch: CLI nros-core 0.5.0 vs workspace
nros-core 0.1.0`. This fails the `generate-bindings` preflight of
`build-all-jobserver.sh` / `just build-test-fixtures`, so no fixture
stamp is written and `just test-all` mass-fails on `_require-fixtures`.

**Root cause.** The Phase 218.J `0.1.0 → 0.5.0` bundle-version bump never
propagated to the standalone example/testing `Cargo.lock` files. ~56
`Cargo.lock` + 7 `Cargo.toml` still pin nano-ros crates at `0.1.0`. The
guard reads the lock in the dir `generate-rust` runs in; an
`examples/<…>/package.xml` dir with a stale own-lock trips it, while
dirs without a stale own-lock walk up to the root lock (correctly
`0.5.0`) and pass.

**This is a FALSE POSITIVE for actual builds.** The real `nros-core`
source is `0.5.0` everywhere — root `Cargo.lock` and `cargo tree -p
nros-core` both report `0.5.0`. Standalone example/testing locks are not
used for the actual workspace compilation. Source/runtime ABI is
consistent.

**Workaround (in use).** `NROS_SKIP_VERSION_CHECK=1` (the documented
`abi_guard` opt-out, `SKIP_ENV` const) for broad-build / generate-bindings
runs. Output targets `0.5.0` correctly.

**Why a clean regen is not a quick side-task.** Standalone locks
reference nano-ros crates as *patched registry* deps
(`[patch.crates-io]` → in-tree path, materialised by `nros ws sync` /
each crate's setup). A repo-wide `ws sync` + `cargo update -p <bundle
crates>` sweep across the tracked locks left **61 of 71 locks still
incomplete** (transitive nano-ros crates stayed `0.1.0`) and produced
**5500+ lines of registry-dep churn** (a fresh resolve of year-stale
locks pulls newer registry deps). Several dirs also need per-crate setup
first (e.g. `tests/simple-workspace` colcon/standalone patch config).

**Structural snag.** `packages/reference/stm32f4-porting/{polling,rtic}`
lack an empty `[workspace]` table, so `cargo locate-project --workspace`
folds them into the root workspace; updating them standalone errors and
would churn the ROOT lock. They need an explicit `[workspace]` table (or
`workspace.exclude` entry) before they can carry an isolated lock.

### Work items

- [ ] Decide the canonical fix: (a) regenerate all standalone locks to
      `0.5.0` and commit the (large) diff as a deliberate maintenance
      commit, or (b) drop committed standalone locks for copy-out
      examples that don't need a pinned lock, or (c) relax/retarget the
      `abi_guard` to read the root lock for in-tree dirs.
- [ ] Add the empty `[workspace]` table to
      `packages/reference/stm32f4-porting/{polling,rtic}` so they isolate.
- [ ] Re-establish `tests/simple-workspace` patch config so its lock can
      regenerate.
- [ ] Until resolved, keep `NROS_SKIP_VERSION_CHECK=1` documented for
      broad builds (it is a correct workaround given the false positive).

### Acceptance

- `just generate-bindings` exits 0 without `NROS_SKIP_VERSION_CHECK`.
- A fresh `git status` after a broad build shows no unexpected
  standalone-lock churn (i.e. committed locks already match what codegen
  produces).

---

## 2. stm32f4 `talker-embassy` fixture does not link

**Symptom.** `build-test-fixtures` fails at the stm32f4 leaf:
`stm32f4-rs-embassy-example` — undefined symbols (`__assert_func`,
`strncmp`, `strcmp`, `memchr`, `nros_platform_alloc`, `nros_platform_dealloc`,
`strncpy`, `strtoul`, `__errno`) on a standalone `cargo build`, and
duplicate `platform_aliases` symbols (`z_random_fill`, `z_clock_now`, …)
when built in the shared fixture target dir.

**Root cause.** `talker-embassy` is an incomplete example — it does not
link as a standalone fixture (missing board libc/platform glue + memory
layout). The pre-226 hard-coded stm32f4 recipe Rust list **deliberately
omitted** it; the Phase 226 Wave 3 migration to
`fixtures-build.sh stm32f4 rust` builds every manifest row, so it now
includes `talker-embassy` and surfaces the breakage. There is no manifest
`skip_build` field (only `skip_probe`).

### Work items

- [ ] Either fix the example (board libc/platform link glue + memory.x)
      so it links as a fixture, or
- [ ] Add a manifest `skip_build` / `exclude` flag (and honor it in
      `fixtures-build.sh` + `fixture-inventory.py` + the stale probe) and
      mark `talker-embassy`, restoring the pre-226 omission.

### Acceptance

- `just stm32f4 build-fixtures` exits 0 (modulo the separate pre-existing
  RTIC `_defmt_timestamp` link issue, tracked elsewhere).

---

## 3. `examples/templates/multi-node-workspace` missing generated dir

**Symptom.** `build-all-jobserver.sh` fails: `failed to load source for
dependency builtin_interfaces` —
`examples/templates/multi-node-workspace/generated/builtin_interfaces/Cargo.toml`
No such file or directory.

**Root cause.** The template workspace path-deps generated message crates
under `generated/` (gitignored), but the broad build does not run a
codegen / `nros ws sync` pass for `examples/templates/*` before resolving
it.

### Work items

- [ ] Add a codegen/`ws sync` preflight for `examples/templates/*` in the
      broad build (or exclude templates from the broad cargo resolve).

---

## 4. threadx-linux cpp `nros_cpp_ffi.h` regeneration race

**Symptom.** Intermittent `nros_cpp_ffi.h` "multiple definition /
conflicting declaration of `nros_cpp_qos_t`" during a cold threadx-linux
cpp fixture build. The on-disk header is clean (1 definition); the
duplication is transient when a cold workspace Corrosion target
regenerates the header mid-build. Also seen during Phase 226.E
measurement.

### Work items

- [ ] Serialize / guard the `nros_cpp_ffi.h` (re)generation so concurrent
      cold cpp builds cannot observe a half-written / duplicated header.

---

## 5. threadx-riscv64 `build-fixture-extras` rc=127

**Symptom.** `just threadx_riscv64` fixture extras exit 127 (command not
found) on the maintainer host during the broad build.

### Work items

- [ ] Identify the missing tool/env (`just threadx_riscv64 doctor`) and
      either provision it via `nros setup` or skip-with-hint when absent.

---

## Notes

- Cross-ref: `docs/roadmap/phase-226-fixture-build-orchestration-audit.md`
  (226.F "Wave 13" results + "226.F Follow-ups").
- The Phase 226 shared-target-dir + make-driver changes are verified
  sound and are NOT implicated in any blocker above.
