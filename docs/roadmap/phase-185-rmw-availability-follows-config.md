# Phase 185 — RMW availability follows configuration (auto-provision embedded Cyclone)

**Goal.** Make "RMW selected in the config ⇒ RMW available" hold **uniformly**,
including the embedded-RTOS CycloneDDS targets. Selecting `cyclonedds` for a
FreeRTOS / ThreadX build must not require the user (or CI) to run an out-of-band
`just cyclonedds <rtos>-cross-probe` first. The build system provisions the
backend's dependency; the user only states intent in `nros.toml` / `-DNROS_RMW`.

**Status.** Not Started.

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

- [ ] When `cyclonedds` is selected for an embedded target and the cross toolchain
      is present, the install is built automatically (no separate user command).
- [ ] Re-running with the install present is a fast no-op (idempotent; ≤ a few s).
- [ ] The host `build/install` (idlc + POSIX ddsc) is still used for `idlc`
      typesupport generation; only `ddsc` is cross-built (`-DBUILD_IDLC=OFF`).

### 185.2 — Embedded-Cyclone tests `skip!` when the SDK is out-of-tier, not hard-fail
A missing cross-Cyclone install (lighter tier / no cross toolchain) is a skipped
precondition, not a test failure. Convert the hard panics to `nros_tests::skip!`
so `test-all` is honest: PASS when provisioned, SKIP when out-of-tier.

**Files**
- `packages/testing/nros-tests/tests/freertos_qemu.rs:~91`
  (`test_freertos_rust_talker_cyclonedds_boot`,
  `test_freertos_rust_cyclonedds_local_pubsub_e2e` — currently a hard panic whose
  message even says "[SKIPPED]")
- ThreadX Cyclone equivalents (`native_api.rs` /
  `test_threadx_*_cyclonedds_*` — audit for the same hard-fail-on-missing pattern)

- [ ] Missing cross-Cyclone install ⇒ `skip!` (reported `[SKIPPED]`, not failed).
- [ ] Present install ⇒ the test runs and asserts as before.
- [ ] No bare `eprintln!`+`return` (would report PASS — forbidden per CLAUDE.md).

### 185.3 — Tiering decision for cross-Cyclone provisioning
Decide and document which SDK tier carries automatic cross-Cyclone provisioning,
honouring the ≤5 min / ≤500 MB / idempotent default-tier policy. Likely: stays in
`all`/extended (carries the cross toolchains), default tier `skip!`s.

**Files**
- `docs/development/sdk-tiers.md` (record the tier each `cyclonedds-<rtos>`
  install belongs to + the toolchain gate)
- `justfile` `_orchestrate` switch (if provisioning is folded into a `setup` tier)

- [ ] `docs/development/sdk-tiers.md` states the tier + toolchain gate for each
      embedded Cyclone install.
- [ ] Default-tier `setup → build-all → test-all` stays within budget (embedded
      Cyclone tests SKIP, not fail) — no regression to the 5-min/500-MB policy.
- [ ] `all`/extended tier (cross toolchains present) builds the installs and the
      tests PASS.

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

- [ ] Shared cross-build helper drives all embedded Cyclone installs.
- [ ] Each target is a small per-target config (toolchain + RTOS/netstack flags +
      board-config include dir), no copied script body.
- [ ] `freertos` and both `threadx` installs still build identically (byte-for-
      byte CMake args preserved or intentionally changed + noted).

### 185.5 — Doctor + discoverability
Surface provisioning state so a user who *is* out-of-tier understands why an
embedded Cyclone test skipped and how to enable it.

**Files**
- `just/cyclonedds.just` `doctor` (report each `cyclonedds-<rtos>-install`
  present/absent + the command/tier to provision)
- `book/src/` Cyclone/RMW page (one line: embedded Cyclone availability follows
  the tier; no per-user import step)

- [ ] `just cyclonedds doctor` lists each embedded Cyclone install's status.
- [ ] A skipped embedded-Cyclone test's `skip!` message names the provisioning
      command/tier.

## Acceptance

- [ ] On a tier carrying the cross toolchains, a clean
      `setup → build-all → test-all` runs the embedded-Cyclone tests and they
      **PASS** with no manual `cross-probe` step.
- [ ] On the default tier, the same flow **SKIPs** them (no hard failures) and the
      rest of `test-all` is green.
- [ ] Selecting `cyclonedds` in `nros.toml` / `-DNROS_RMW=cyclonedds` for
      freertos/threadx requires **no** out-of-band user command — parity with
      zenoh / XRCE and with native / Zephyr Cyclone.
- [ ] `find_package(CycloneDDS)` is retained (no per-example
      `add_subdirectory(CycloneDDS)`); one cross `ddsc` build per target ABI,
      shared across that target's example cells.
- [ ] `docs/development/sdk-tiers.md` documents the tiering + toolchain gate.

## Notes

- The host Cyclone install (`build/install`, POSIX `ddsc` + `idlc`) is unaffected —
  it remains a `just cyclonedds setup` artifact and supplies `idlc` for typesupport
  generation on every target (`-DBUILD_IDLC=OFF` in the cross builds).
- This is a provisioning/workflow change only — no change to the wire protocol, the
  `nros-rmw-cyclonedds` backend logic, or the `find_package` consumption shape.
- Surfaced by Phase 184's clean-room run: `test_freertos_rust_cyclonedds_*` reds
  were "cyclonedds not prebuilt" (install absent), while `threadx-rv64` Cyclone
  passed because its install happened to exist from a 184.7 manual cross-probe.
