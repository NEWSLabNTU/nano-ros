---
id: 57
title: host-integration-tests chronically red — fixture-build OOM + light-tier skip-gating regression
status: resolved
type: bug
area: testing
related: [phase-244, phase-248, phase-249, issue-0067]
resolved_in: "Cause-1 OOM cap + fa2ecb60a residue triage + 2026-06-15 local lane validation"
---

> **RESOLVED (2026-06-15).** The chronic red was **Cause-1 (fixture-build OOM)**
> — capped (`NROS_BUILD_JOBS=2 × CARGO_BUILD_JOBS=2` on both build steps + the
> nextest compile). The post-cap residue (11 real failures from phase-248/249
> churn) was fixed in `fa2ecb60a`; the exclude-leak (granular QEMU sub-groups) was
> closed by adding `binary(rtos_e2e|zephyr|phase_118_collapse)` to the
> `test-integration` exclude.
>
> **Local lane validation (2026-06-15)** — the CI lane can't complete under the
> current multi-agent main-push cadence (`cancel-in-progress: true` cancels every
> ~45-min run within ~10 min), so validated locally by mirroring it
> (`build-fixture-rust-core` + `build-workspace-fixtures` + `NROS_FIXTURES_OPTIONAL=1
> just test-integration`): builds green; **0 real failures in the CI-equivalent set**
> (72 `[SKIPPED]` reclassified). The only 5 real failures are **CycloneDDS extras
> tests CI does not build** (`build-fixture-extras` skipped on the light lane) → they
> `skip!` on CI, so NOT this lane's red. They are a distinct rust-typed-cyclone-
> publisher regression tracked as
> [issue 0067](0067-rust-typed-cyclonedds-publisher-creation-fails.md).
>
> Note: a clean CI green is structurally unreachable under the current per-push
> trigger + `cancel-in-progress` while agents push to `main` every ~10 min — a
> CI-policy tuning matter (cadence / concurrency), not a code defect; left to the
> maintainer. Lane correctness validated locally above.

`host-integration-tests.yml` (the native `nros-tests` integration lane on
`ubuntu-22.04`, 2 vCPU / 7 GB) is effectively red — green only flakily. Two
independent causes, found 2026-06-13 while trying to validate phase-244 D3.

## Cause 1 — fixture-build OOM (fix ready)

The fixture builder fans out `NROS_BUILD_JOBS` (default **8**) cargo frontends,
each spawning its own rustc jobs. Compiling the heavy codegen deps
(`toml` / `regex-automata` / `nom` / `mime_guess` / `indexmap`) across the native
example fixtures exceeds 7 GB and the kernel OOM-kills rustc mid-compile —
surfacing as `SIGSEGV` / `SIGILL` / `SIGABRT` (reproduced locally with the same
crashes). The two `Build … fixtures` steps are best-effort (`|| … [SKIPPED]`), so
the crash silently leaves fixtures unbuilt or partially written; the run then
fails downstream on missing/corrupt binaries. Flaky: green when the runner has
headroom, red when not (matches the green-07:52 → red-12:03 flapping).

**Fix (branch `fix-host-integration-oom`, not yet merged):** cap the two
fixture-build steps + the nextest compile to `NROS_BUILD_JOBS=2` ×
`CARGO_BUILD_JOBS=2` (≤4 concurrent rustc). Verified: the capped build now runs
to completion (~45 min, no OOM-skip) instead of fast-failing. This is the
CLAUDE.md "parallel-build memory pressure" mitigation.

## Cause 2 — NOT a skip-gating bypass (re-diagnosed 2026-06-15)

**The original "195 tests bypass the skip path" diagnosis is wrong.** Reproduced
locally (`NROS_FIXTURES_OPTIONAL=1`, extras absent, `native_api` + `logging_smoke`
+ `integration_zephyr`): the fixture-absent tests DO route through
`require_prebuilt_binary` and `skip!` correctly — they panic
`[SKIPPED] fixture binary not prebuilt: …` from `mod.rs:280`. The
`native_api` callback/interop cases use
`build_*_callback().unwrap_or_else(skip_missing_fixture)`, but the inner
`require_prebuilt_binary` `skip!`-panics `[SKIPPED]` *before* returning, so
`skip_missing_fixture`'s non-`[SKIPPED]` `panic!` arm is never reached.

`cargo nextest` has no native skip, so every `[SKIPPED]` panic shows as a raw
`FAIL` — that is the **195** figure. But the lane's pass/fail is decided by the
`test-integration` recipe's `_rewrite-skipped-junit` + `_count-real-failures`,
which reclassify `[SKIPPED]` junit failures to `<skipped>`. Verified: a 47-test
`native_api`/`logging_smoke`/`integration_zephyr` slice showed 13 raw nextest
FAILs → `_rewrite-skipped-junit` rewrote **10** to `<skipped>` → `_count-real-failures`
returned **2**. **Skip-gating is functioning; the "195" was the raw nextest count,
not the recipe's real-failure count.**

The chronic red is therefore **Cause 1 (OOM)** corrupting/partially-writing
fixtures → the binary then *exists* (so `require_prebuilt_binary` returns Ok, no
skip) → it crashes/misbehaves at runtime → a genuine non-`[SKIPPED]` FAIL. Cap the
build jobs (Cause 1) and the lane stops manufacturing those.

### Residual: 2 genuine failures were STALE FIXTURES (confirmed 2026-06-15)
`test_cpp_rust_service_interop` (`native_api.rs:1241`, "0 responses from C++
server") and `test_action_callback_interop_c_client_cpp_server` (`:850`, no "Goal
accepted!") FAILed even in isolation — both **C++ server ↔ non-C++ client**, with
local C/C++ fixtures dated **2026-06-12** (pre-phase-248-C5c). **Rebuilt the C/C++
fixtures fresh against current nros-cpp** (the C5c agnosticism churn even shrank the
binaries, 7.0 MB → 2.6 MB) and **both PASS**. Confirmed **stale fixtures, NOT a
regression**. These tests are not exercised by the light CI lane anyway (extras
absent → `[SKIPPED]`).

## Full-lane local triage (2026-06-15)

Ran the real `just test-integration` recipe with ROS2 Humble sourced + fresh
fixtures: **387 run, 96 real (non-`[SKIPPED]`) failures** after
`_rewrite-skipped-junit`. This is far more than the 2 residuals — but the bulk is
NOT CI-relevant. Breakdown:

- **~51 are QEMU/Zephyr e2e** (`rtos_e2e`, `zephyr`, `phase_118_collapse`):
  local-only artifacts. This box HAS `qemu-system-arm` but the firmware fixtures
  weren't built, so the tests pass the QEMU guard then fail on the absent image.
  On CI the runner has no QEMU → they `skip!` at the guard. **Latent bug surfaced:**
  the `test-integration` exclude lists *umbrella* groups (`group(=qemu-freertos)`,
  `group(=qemu-zephyr)`) but nextest assigns these tests to *granular* sub-groups
  (`qemu-freertos-pubsub`, `qemu-zephyr-pubsub-rust`, … — first-match-wins,
  `.config/nextest.toml:317+`), so the exclude never removes them. Masked on CI
  (no QEMU). Fix = exclude the e2e *binaries* (`binary(rtos_e2e)`/`binary(zephyr)`/…)
  or add every granular group to the exclude.
- **~45 are non-QEMU**, almost all env/fixture/CLI-coupled, not logic bugs:
  - `workspace_lints_check` (5/5, instant) — invoke the `nros check` CLI; behavior
    coupled to the phase-248/249 CLI churn.
  - `workspace_metadata` / `orchestration_*` / `migrate_workspace` / `bringup_scaffold`
    — workspace-codegen fixtures + orchestration tooling.
  - `cyclonedds_*` / `px4_xrce` / `*_ros2_interop` / `demo_nodes_cpp_interop` — need
    a live ROS 2 graph / PX4 tree / cyclone extras.
  - `native_api` (5) — remaining cyclone/callback fixture variants.
  - `legacy_files_forbidden` / `examples_canonical_shape` PASS → the reverted
    legacy templates did NOT break the tree lints.

**Conclusion:** the chronic CI red is **Cause-1 (OOM)** — fixed. The rest is a broad,
multi-subsystem triage that is NOT faithfully reproducible locally (CI lacks QEMU
+ has its own fixture/tool matrix) and overlaps the phase-248/249 CLI/workspace
churn. Triage it from the *actual CI run's* failures once the OOM cap lands green,
not from this box's env-mismatched numbers.

## Impact

The lane cannot gate native-rust/C/C++ example changes (e.g. phase-244 D3's
talker/listener fork-unification can't be CI-validated here). Validation is being
done locally meanwhile.

## Direction

1. **Cause-1 OOM cap — DONE** (`fix-host-integration-oom` never merged; reapplied
   to current main): `NROS_BUILD_JOBS=2` × `CARGO_BUILD_JOBS=2` on both
   fixture-build steps + `CARGO_BUILD_JOBS=2` on the nextest compile in
   `host-integration-tests.yml`. This is the actual lane-red driver (corrupt
   fixtures → runtime FAILs that aren't `[SKIPPED]`).
2. ~~Audit skip-gating bypass~~ — **not needed** (re-diagnosed above): skip routing
   + `_rewrite-skipped-junit`/`_count-real-failures` already work; the "195" was the
   raw nextest count.
3. Residual 2 `native_api` C++-server interop failures — **confirmed stale
   fixtures** (fresh rebuild → both pass). No regression; no separate issue needed.
4. **Exclude leak — DONE**: added `binary(rtos_e2e)` / `binary(zephyr)` /
   `binary(phase_118_collapse)` to the `test-integration` exclude (both the run and
   `_count-real-failures`), so the QEMU/Zephyr e2e tests can't leak past the
   umbrella-`group()` exclusion onto a QEMU-equipped runner. (No-op on CI today —
   CI has no QEMU — but makes the lane deterministic + removes the local noise.)
5. The broad non-QEMU residue (see "Full-lane local triage") is env/fixture/CLI-
   coupled + overlaps phase-248/249 churn; triage it from the actual CI run's
   failures once the cap lands green, not from this env-mismatched box.

## Cause-1 cap CONFIRMED + post-cap residue triaged & fixed (2026-06-15)

CI run `27525622918` (host-integration-tests, capped) **confirmed Cause-1 dead**: both
`Build rust core fixtures` and `Build workspace fixtures` ran to completion — no OOM,
no SIGSEGV/SIGILL/SIGABRT. The lane then failed `just test-integration` with
`_count-real-failures` = **11 real (non-`[SKIPPED]`) failures** (139 `[SKIPPED]`
correctly rewritten to `<skipped>` — skip-gating works, per Cause-2). Those 11 were
the actual post-cap residue. Triaged from the run's junit artifact → **5 root causes,
all fixed and validated locally**:

- **A — `cargo` resolution: `nros` feature `platform-posix` removed (phase-248 C5c)**
  (5 failures: `native_main_macro_misuse` ×4, `native_orchestration_misuse` ×1). The
  `n9_workspace` / `orchestration_tiers_native` / `o4` / `o5` fixtures + the
  `multi-node-workspace` template still requested the deleted `platform-posix` feature
  on the `nros` umbrella. Dropped it (15 manifests); `nros` is platform-agnostic now —
  platform comes from the board / `nros-platform-cffi`. Validated: misuse tests pass.
- **B — `cross_libc_precedence_gate`** (1): CI's `arm-none-eabi-g++` lacked libstdc++
  (`<type_traits>` absent) → the probe hard-failed instead of modelling the `div_t`
  clash. Added a `cxx_stdlib_available()` capability gate → `skip!` on a C-only cross
  (unmet precondition), not a false FAIL.
- **C — `non_goals_grep`** (1): read a hardcoded `docs/roadmap/phase-212-…md`; phase 212
  was archived. Pointed at `docs/roadmap/archived/`.
- **D — `workspace_metadata` cpp/mixed Entry not prebuilt** (3): phase-249 P4a removed
  the weak `nros_app_register_backends` default, but the LAUNCH-based `nano_ros_entry`
  (cmake) never wired the strong stub for native C/C++ entries (the 244.C4
  node-register carrier does, but its `NOT TARGET` guard skips the already-created
  entry). Fix: `nano_ros_entry` calls `nros_platform_link_app` for native C/C++.
  Unmasked a second latent bug — mixed (C+C+++**Rust**) workspaces double-link nros-c
  (standalone `libnros_c.a` + the runtime umbrella) → `multiple definition` of
  `nros_rmw_cffi_*` / `nros_rmw_<x>_register`. Fix: `nros_synth_runtime_umbrella` also
  repoints the C umbrella (`NanoRos`) `nros_c-static` → `nros_ws_runtime-static`
  (preserving its INTERFACE include dirs), mirroring the nros-cpp-headers swap.
  Validated: c/cpp/mixed workspaces all link; all 7 `workspace_metadata` tests pass.
- **E — `examples_canonical_shape`** (1): the §212.L.4 esp32-baremetal class/pkg-name
  mismatch — already fixed by the local (unpushed) commit `fix(#57): esp32-baremetal
  rust examples`; the failing CI run predated it. No-op once pushed.

**Remaining gate:** push + confirm the next CI run greens with all of the above. The OOM
cap is proven; the 11 real failures are fixed locally. Resolve once CI lands green.
