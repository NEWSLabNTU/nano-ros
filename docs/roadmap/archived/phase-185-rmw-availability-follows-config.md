# Phase 185 — RMW availability follows configuration (auto-provision embedded Cyclone)

**Goal.** Make "RMW selected in the config ⇒ RMW available" hold **uniformly**,
including the embedded-RTOS CycloneDDS targets. Selecting `cyclonedds` for a
FreeRTOS / ThreadX build must not require the user (or CI) to run an out-of-band
`just cyclonedds <rtos>-cross-probe` first. The build system provisions the
backend's dependency; the user only states intent in `nros.toml` / `-DNROS_RMW`.

**Status.** Done — archived 2026-06-11. All work items 185.1–.5 + every
acceptance criterion met (the acceptance boxes had been left unticked; each is
satisfied by a completed item — freertos Cyclone e2e PASS, the tier-gated
`test-all` `-E` filter, the sdk-tiers.md tiering doc). Original status line:
Complete (2026-05-28). 185.1–.5 done: freertos + threadx-rv64
`build-fixtures` auto-provision the cross Cyclone install; `test-all` filters
embedded-Cyclone tests out when out-of-tier (skipped, not failed); tiering
documented; cross-probe scripts deduped behind `cross-build-ddsc.sh`; doctor +
book note added. Verified: freertos Cyclone e2e tests PASS.

**Priority.** P2 — not an MVP blocker (zenoh + XRCE already satisfy the contract
on every target; native + Zephyr Cyclone already do too), but it is a real
workflow-correctness gap: a clean `setup → build-all → test-all` reports the
embedded-Cyclone tests as **failures** even though nothing is broken, and the
remediation ("run a cross-probe") is undiscoverable from the config.

**Depends on.** Phase 117 (Cyclone RMW), Phase 175 (native Cyclone CMake/
Corrosion path), Phase 184 (clean-room fixture/tooling gaps — surfaced this gap
via the `test_freertos_rust_cyclonedds_*` reds).

---

## Overview

The three RMW backends are provisioned inconsistently:

| RMW | How it becomes available | Explicit user step? |
|---|---|---|
| **zenoh-pico** (`zpico-sys`) | vendored C, compiled on-the-fly by cargo/cc during the build | none, any target |
| **XRCE** (`nros-rmw-xrce-cffi`) | vendored C, compiled on-the-fly by cargo/cc | none, any target |
| **Cyclone** (`nros-rmw-cyclonedds`) | `find_package(CycloneDDS REQUIRED CONFIG)` against a **pre-installed** `ddsc` (`packages/dds/nros-rmw-cyclonedds/CMakeLists.txt:37`) | depends on target — see below |

Cyclone never compiles `ddsc` itself; it always consumes an external install.
Whether that install exists "for free" depends on the target:

| Target | `find_package(CycloneDDS)` resolves to | Provisioned by | Step today |
|---|---|---|---|
| **native** | `build/install` (host x86 POSIX `libddsc.so`) | `just cyclonedds setup` — always in `setup all` | transparent |
| **zephyr** | in-tree Cyclone Zephyr module, built by the Zephyr build (NSOS includes available) | the zephyr build itself | transparent |
| **freertos** | `build/cyclonedds-freertos-install` (cross Cortex-M3 `libddsc.a`) | `just cyclonedds freertos-cross-probe` **only** | **manual** |
| **threadx-linux / threadx-rv64** | `build/cyclonedds-threadx*-install` (cross) | `just cyclonedds threadx-cross-probe` **only** | **manual** |

### Why the embedded targets need a separate cross install (not a bug in itself)

1. **Wrong-ABI host install can't be reused.** `build/install` is x86-POSIX and
   cannot link into a Cortex-M3 / RV64 image. Each embedded ABI needs its own
   cross-built `ddsc`.
2. **The ddsrt port compiles against the RTOS + netstack.** The cross build
   configures Cyclone `-DWITH_FREERTOS=ON -DWITH_LWIP=ON` (ThreadX: `WITH_THREADX`
   + NetX) plus include paths to the board config (`FreeRTOSConfig.h`,
   `lwipopts.h`, `arch/cc.h`), kernel headers, and netstack port headers
   (`scripts/cyclonedds/freertos-cross-probe.sh`).

### Why it is *manual* today (the actual gap)

The `*-cross-probe.sh` scripts began as **bring-up probes** — their own headers
say *"intentionally stops short of wiring any example fixture cells … proves how
far the pinned Cyclone tree gets with WITH_FREERTOS + WITH_LWIP."* They became
the de-facto install producer because the on-the-fly provisioning was never
wired into setup/build. So `just <rtos> build-fixtures` hits the
`[ -f build/cyclonedds-<rtos>-install/lib/libddsc.a ]` gate
(`just/freertos.just:130`), finds it absent, prints "cyclonedds skipped", and the
fixture is never built — then `test-all` runs the test anyway and **hard-fails
"not prebuilt"**.

## Architecture / principles

- **RMW selection is the contract.** Stating `cyclonedds` in `nros.toml` /
  `-DNROS_RMW=cyclonedds` is the user's whole responsibility. Mirrors real ROS 2
  (`RMW_IMPLEMENTATION=rmw_cyclonedds_cpp` "just works" because the RMW + its DDS
  are a provisioned package) and the existing zenoh/XRCE behaviour.
- **The build provisions the dependency, not the user.** This already matches the
  SDK-path contract in CLAUDE.md: *"Third-party SDKs (Cyclone DDS, NetX Duo,
  FreeRTOS-Kernel) pass `-DCMAKE_PREFIX_PATH=` to their own install prefixes"* —
  the build supplies the prefix; the example only `find_package`s it.
- **Keep `find_package`; do NOT `add_subdirectory(CycloneDDS)` per example.** One
  cross `ddsc` build per target ABI, cached and shared across all that target's
  example cells. Per-example `add_subdirectory` would rebuild Cyclone for every
  cell and diverge from the prefix-path contract.
- **A missing-but-out-of-tier SDK is a `skip!`, not a failure.** Cross-building
  Cyclone needs the cross toolchain and exceeds the default-tier cost budget
  (≤5 min / ≤500 MB / idempotent — `docs/development/sdk-tiers.md`). When the
  install is legitimately absent in a lighter tier, the test must
  `nros_tests::skip!`, not hard-panic.

## Work Items

### 185.1 — Auto-provision the cross-built Cyclone install during the build
Make the embedded-Cyclone fixture build produce its own `ddsc` install instead of
silently skipping. `just <rtos> build-fixtures` (and the
`build-fixtures`/`build-all` paths that reach it) must run the cross-probe build
as a **dependency** when the active RMW is `cyclonedds` and the cross toolchain is
present — exactly as host Cyclone is a dependency of `just cyclonedds setup`.

**Files**
- `just/freertos.just` (the `build-fixture-extras` gate at ~line 130 — replace the
  silent skip with "provision then build")
- `just/threadx_linux.just`, `just/threadx_riscv64.just` (same gate)
- `just/cyclonedds.just` (`freertos-cross-probe` / `threadx-cross-probe` recipes,
  lines 66–74 — make them idempotent + callable as provisioning deps)
- `scripts/cyclonedds/freertos-cross-probe.sh`, `scripts/cyclonedds/threadx-cross-probe.sh`
  (already idempotent build+install; confirm re-run is a fast no-op when the
  install exists, matching `just cyclonedds setup`'s `[ -f … ] && echo already-built`)

- [x] When `cyclonedds` is selected for an embedded target and the cross toolchain
      is present, the install is built automatically (no separate user command).
      *(freertos: `just/freertos.just`; threadx-rv64: `just/threadx-riscv64.just`,
      under its experimental env opt-in.)*
- [x] Re-running with the install present is a fast no-op (idempotent — the
      provisioning is gated on `[ ! -f …/libddsc.a ]`).
- [x] The host `build/install` (idlc + POSIX ddsc) is still used for `idlc`
      typesupport generation; only `ddsc` is cross-built (`-DBUILD_IDLC=OFF`).
- [x] **Verified (freertos):** clean `build-fixtures` provisions
      `build/cyclonedds-freertos-install` + builds the cyclone cells;
      `test_freertos_rust_talker_cyclonedds_boot` + `_local_pubsub_e2e` **PASS**.

### 185.2 — Out-of-tier embedded-Cyclone tests should be *filtered* (skipped), not failed
**Premise correction (2026-05-28):** the embedded-Cyclone tests *already* call
`nros_tests::skip!` on a missing fixture (`freertos_qemu.rs:91`). But `skip!`
expands to `panic!("[SKIPPED] …")` (`nros-tests/src/lib.rs:53`), and nextest has
**no special handling for a `[SKIPPED]` panic — it counts as a failure.** That is
*intentional* per CLAUDE.md ("Tests must fail on unmet preconditions … `skip!`
panics with `[SKIPPED]` (OK)") — a missing fixture should be loud, the same way
the XRCE-Agent / patched-qemu tests fail when their SDK isn't installed
(Phase 184). So 185.1 is the real fix: provision the fixture in the tier that
runs `test-all`, and the test PASSES (no longer skips).

The only honest way to make an out-of-tier run *not* show a failure is to
**exclude those tests from the nextest run** (nextest "filtered" ⇒ counts as
`skipped`, the mechanism behind the existing 7 skips) when the cross toolchain /
provisioning tier is absent — a runtime `skip!` cannot become a nextest skip.

**Files**
- `.config/nextest.toml` and/or the `test-all` `-E` filter expressions in the
  justfile (add a tier/toolchain-gated exclusion for the embedded-Cyclone tests)
- `packages/testing/nros-tests/tests/freertos_qemu.rs` (already `skip!` — no
  change needed; keep as the loud signal when *in*-tier but unbuilt)

- [x] Audited: embedded-Cyclone tests already use `skip!` (loud), not a bare
      `eprintln!`+return — correct per CLAUDE.md. No softening needed.
- [x] When the provisioning tier/toolchain is absent, the embedded-Cyclone tests
      are **filtered out** of the `test-all` nextest run (report `skipped`, not
      `failed`) — `test-all` appends a `-E 'not (binary(freertos_qemu) and
      test(~cyclonedds)) and not (binary(threadx_riscv64_qemu) and
      test(~cyclonedds))'` exclusion, gated on each
      `build/cyclonedds-<rtos>-install/lib/libddsc.a` being absent (`justfile`
      `test-all`). Validated via `nextest list`: gated ⇒ 0 cyclone tests,
      freertos zenoh tests retained.
- [x] In-tier (toolchain present) the tests are included, provisioned by 185.1,
      and PASS (freertos verified).

### 185.3 — Tiering decision for cross-Cyclone provisioning
Decide and document which SDK tier carries automatic cross-Cyclone provisioning,
honouring the ≤5 min / ≤500 MB / idempotent default-tier policy. Likely: stays in
`all`/extended (carries the cross toolchains), default tier `skip!`s.

**Files**
- `docs/development/sdk-tiers.md` (record the tier each `cyclonedds-<rtos>`
  install belongs to + the toolchain gate)
- `justfile` `_orchestrate` switch (if provisioning is folded into a `setup` tier)

- [x] `docs/development/sdk-tiers.md` states the tier + toolchain gate for each
      embedded Cyclone install (new "Embedded CycloneDDS" subsection).
- [x] Default-tier `setup → build-all → test-all` stays within budget (embedded
      Cyclone tests SKIP, not fail) — 185.2's tier-gated `-E` filter delivers this.
- [x] `all`/extended tier (cross toolchains present) builds the installs and the
      tests PASS (freertos verified).

### 185.4 — Generalize + dedupe across embedded targets
The freertos and threadx cross-probe scripts duplicate the same configure/build/
install shape with per-target toolchain + RTOS/netstack flags. Factor the shared
logic so adding a future embedded Cyclone target is one small config block, not a
third copied script.

**Files**
- `scripts/cyclonedds/freertos-cross-probe.sh`,
  `scripts/cyclonedds/threadx-cross-probe.sh` (extract a shared
  `scripts/cyclonedds/cross-build-ddsc.sh <target-config>` helper)
- `cmake/toolchain/arm-freertos-armcm3.cmake` + ThreadX toolchain(s) (referenced,
  unchanged)

- [x] Shared `scripts/cyclonedds/cross-build-ddsc.sh` (sourced) owns the common
      boilerplate: `csb_check_file`/`csb_check_dir`/`csb_require_compiler`,
      `csb_parse_mode`, `csb_finalize_checks`, `csb_wipe_stale_lto` (LTO targets
      only), `csb_configure_build_install`.
- [x] Each probe is now a small per-target config (toolchain + RTOS/netstack
      include checks + `c_flags` + `cmake_args`), no copied control flow.
- [x] `freertos` + `threadx-rv64` cross-builds preserve their exact CMake args;
      both re-run `rc=0` and reproduce their installs (`libddsc.a` present).
      *(Only freertos + threadx-rv64 have cross-probes today; threadx-linux
      Cyclone is host-linked, no cross build.)*

### 185.5 — Doctor + discoverability
Surface provisioning state so a user who *is* out-of-tier understands why an
embedded Cyclone test skipped and how to enable it.

**Files**
- `just/cyclonedds.just` `doctor` (report each `cyclonedds-<rtos>-install`
  present/absent + the command/tier to provision)
- `book/src/` Cyclone/RMW page (one line: embedded Cyclone availability follows
  the tier; no per-user import step)

- [x] `just cyclonedds doctor` lists each embedded Cyclone install's status
      (report-only; absence ≠ `missing`) with the provisioning command.
- [x] The freertos test's `skip!` message already names `just freertos
      build-fixtures` — accurate now that build-fixtures auto-provisions (185.1).
- [x] `book/src/user-guide/rmw-backends.md`: embedded Cyclone availability
      follows the SDK tier; no per-user import step (links sdk-tiers.md + doctor).

## Acceptance

- [x] On a tier carrying the cross toolchains, a clean
      `setup → build-all → test-all` runs the embedded-Cyclone tests and they
      **PASS** with no manual `cross-probe` step.
- [x] On the default tier, the same flow **SKIPs** them (no hard failures) and the
      rest of `test-all` is green.
- [x] Selecting `cyclonedds` in `nros.toml` / `-DNROS_RMW=cyclonedds` for
      freertos/threadx requires **no** out-of-band user command — parity with
      zenoh / XRCE and with native / Zephyr Cyclone.
- [x] `find_package(CycloneDDS)` is retained (no per-example
      `add_subdirectory(CycloneDDS)`); one cross `ddsc` build per target ABI,
      shared across that target's example cells.
- [x] `docs/development/sdk-tiers.md` documents the tiering + toolchain gate.

## Notes

- The host Cyclone install (`build/install`, POSIX `ddsc` + `idlc`) is unaffected —
  it remains a `just cyclonedds setup` artifact and supplies `idlc` for typesupport
  generation on every target (`-DBUILD_IDLC=OFF` in the cross builds).
- This is a provisioning/workflow change only — no change to the wire protocol, the
  `nros-rmw-cyclonedds` backend logic, or the `find_package` consumption shape.
- Surfaced by Phase 184's clean-room run: `test_freertos_rust_cyclonedds_*` reds
  were "cyclonedds not prebuilt" (install absent), while `threadx-rv64` Cyclone
  passed because its install happened to exist from a 184.7 manual cross-probe.
