# Phase 203 — clean-rebuild `test-all` status

**Goal.** Record the result of a full from-scratch validation — nuke the
build cache + deinit all submodules, then `just setup all` → `just check`
→ `just build-all` → `just test-all` — fix the setup/build gaps that only
surface on a clean tree, and triage the residual `test-all` failures.

**Status.** Done (2026-05-30). `build-all` is **green from clean**; `test-all`
is `773 run, 764 passed (5 flaky), 9 failed, 7 skipped`. All 9 failures are
either precondition-skips (a specific heavy fixture/SDK not staged) or
timing-sensitive runtime e2e — no new logic regressions.

**Priority.** P2 — the clean-tree build gaps are fixed (committed); the
residual test failures are pre-existing runtime/fixture items, several already
tracked by archived Phase 200.

---

## Clean-tree setup/build gaps fixed

A nuked tree (`git submodule deinit -f --all` + `just clean`) exposed several
from-scratch failures that a persistent working tree had masked. All fixed so
`just setup all` + `just build-all` succeed end-to-end:

1. **xrce agent (prebuilt, not source)** — `scripts/xrce-agent/build.sh` now
   uses the prebuilt MicroXRCEAgent that `nros setup … --rmw xrce` provisions
   into the store (store-first; publishes a forwarding wrapper since the store
   binary is a relocatable launcher). No source build / submodule on a
   provisioned tree; if unprovisioned it fail-louds pointing to `nros setup`
   (source build only if the submodule is already checked out — no silent
   init). Mirrors the zenohd path (#3).
2. **Retired codegen functions** — `build-all-jobserver` called
   `nros_cargo_fetch_codegen` (deleted) and `nros_cargo_build_codegen_c`
   (renamed). The codegen tool is the installed `nros` binary now; dropped the
   fetch call, switched to `nros_cargo_ensure_codegen_c`.
3. **zenohd from clean** — `scripts/zenohd/build.sh` hard-errored on the
   missing zenoh source submodule. Now prefers the prebuilt zenohd from the
   nros store (`nros setup … --rmw zenoh`); on a miss it fail-louds pointing
   to `nros setup` (source build only if the submodule is already checked out
   — no silent init).
4. **zephyr rust per-RMW features** — the canonical `cargo-features-patch.sh`
   (EXTRA_CARGO_ARGS pass-through, so xrce/cyclonedds rust examples compile
   their own backend instead of the default `rmw-zenoh`) was never wired into
   the build; prior trees relied on a stale persistent workspace. Wired it into
   the zephyr setup + `build-fixtures` patch set. (Removed a duplicate patch
   script that double-injected the args.)
5. **NuttX kernel pre-provision** — `just nuttx build-fixtures` ran the example
   cargo builds without first building the kernel, so each board `build.rs`
   failed on `#include <nuttx/config.h>` (cleared by `just clean`). It now runs
   the idempotent `just nuttx build` (config.h + staging/libc.a) first.

Commits: `05bd1a3d5`, `5fefe58a2` (+ the earlier xrce-agent auto-init).

## `test-all` residual failures (9)

### A. Precondition-skips — a heavy fixture/SDK not staged (6)

These `[SKIPPED]`-with-reason fail-louds need a one-off build/provision step;
they are not logic failures. Exact remediation (from each skip message):

| Test | Needs |
|---|---|
| ~~`emulator::test_qemu_bsp_{talker,listener}_starts` (2)~~ | **Resolved (2026-05-30):** replaced by `test_qemu_bsp_pubsub_e2e` — real ethernet pub/sub over QEMU **slirp** (no Docker/TAP; both instances reach host zenohd:7450 via 10.0.2.2). Gates cleanly (skips with reason when ARM toolchain / qemu / zenoh-pico-arm / fixtures absent); runs + passes when staged (`just qemu build-fixtures` + `just qemu build-zenoh-pico`). |
| `integration_zephyr::zephyr_integration_shell_smoke` | `ZEPHYR_BASE` env or an **in-tree** `zephyr-workspace` symlink (the workspace is the `../nano-ros-workspace` sibling; this test only checks the in-tree path) |
| `integration_px4::px4_integration_template_smoke` | a complete PX4 checkout — `just px4 setup` (fixed: nros ≥ 0.3.7 depth-1 fetch-by-SHA fallback resolves the lagging pin shallow + the `Makefile` is present; see the px4 checkbox below) |
| `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` | the make-fixture kernel restaged with nano-ros app symbols — `just nuttx build-fixtures-make` |
| `threadx_riscv64_qemu::test_threadx_riscv64_cyclonedds_two_qemu_pubsub` | the CycloneDDS ThreadX fixtures — `just cyclonedds threadx-cross-probe` |

Follow-up worth doing: the zephyr-shell + nuttx-make + threadx-cyclone fixtures
should be staged by `just build-all` (or their tests should resolve the sibling
workspace / build-fixtures-make output), so a clean `build-all` + `test-all`
needs no extra manual step. (The px4 template is fixed — nros ≥ 0.3.7 depth-1
fetch-by-SHA fallback; see the px4 checkbox below.)

### B. Timing-sensitive runtime e2e (flaky / known-incomplete) (3)

Service/action round-trips over a freshly-started daemon/agent, sensitive to
discovery timing under full-parallel `test-all` load. Several passed on retry
(counted in the "5 flaky"); the rest are the residual runtime items from the
archived Phase 200 (zephyr cyclonedds/xrce service+action data plane, the
`rtos_e2e` ThreadX-Linux C++ action / NuttX C service, `xrce`
`service_request_response`, `ros2_lifecycle_full_cycle`). No new regression;
these are runtime-completeness work, not clean-tree build gaps.

- `ros2_lifecycle_interop::ros2_lifecycle_full_cycle`
- `rtos_e2e::…ThreadxLinux::…Cpp` (action), `rtos_e2e::…Nuttx::…C` (service)
- `xrce::test_xrce_service_request_response`
- `zephyr::test_zephyr_{c_service_server_to_client,rust_service}_e2e`,
  `zephyr::test_zephyr_xrce_c_action_e2e`

## Correction (2026-05-30) — what actually fails `test-all`, and fixes landed

The "9 failed" above is nextest's **raw** count. `just _count-real-failures`
(which reclassifies `[SKIPPED]`-message failures as pass) reported **0** — so
**none of the nextest integration failures actually fail `test-all`**; they are
all precondition-skips counted as pass. The real `test-all` exit=1 came from two
blockers **outside the nextest run**:

1. **`test-miri`** — `nros-params` `persist::tests` do real filesystem I/O +
   read the realtime clock (`SystemTime::now`), which miri isolation forbids
   (`clock_gettime REALTIME not available`). **Fixed**: the four FS-touching
   persist tests are now `#[cfg_attr(miri, ignore)]` (`9fb894155`).
2. **Stale recipe** — `test-all` called `just native _test-orchestration-e2e`,
   removed with the codegen-submodule retirement. **Fixed**: dropped the call
   (`9fb894155`).

Also landed this round:
- **zephyr-shell** now resolves the sibling `../nano-ros-workspace` (the
  canonical workspace path), so it **passes** instead of skipping (`86e15bbab`).
- **nuttx-make**: `build-all-full` removed (the make path is now an explicit
  opt-in `just nuttx build-fixtures-make`). Attempting to fold it into
  `build-all` surfaced that the make fixture is **not buildable on a clean
  tree**: (a) it never generated `app_config.h` for config-less cpp examples —
  **fixed** (always generate, gen-app-config emits defaults); (b) it then fails
  to link — the C/C++ examples need `libnros_c.a` for `armv7a-nuttx-eabihf`,
  the **unsolved tier-3 nros-c-on-NuttX cross-build** (cmake skips `nros-c` for
  NuttX, Phase 160.D). So it stays opt-in; `nuttx_make_e2e` skips until that
  cross-build lands.

**Staleness caveat (important for valid runs):** the zephyr fixture resolver
treats a fixture older than its sources as a **hard failure** ("Zephyr fixture
binary is stale"), not a skip. Editing any source between `build-all` and
`test-all` therefore turns the whole zephyr suite red (~30 false fails). Run
`test-all` immediately after `build-all` with no edits in between.

## Per-platform CI (`platform-ci.yml`) — container findings (2026-05-30)

The same clean-tree validation, run **per platform in the `nano-ros-ci` container**
(Phase 196.9), surfaced a second class of from-scratch failures the local tree
masked — a *containerised* fresh checkout has no cargo cache, no host SDK state,
and builds the per-platform fixtures **in parallel**. All builds are now green
6/6 (qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32); e2e is
nightly (below). Fixes landed:

- **Parallel cbindgen-header race (nuttx, the headline).** `nros-c`'s
  `nros_generated.h` was gitignored, so on a fresh checkout the N parallel example
  cmake projects each ran nros-c's build.rs and raced creating/truncating the
  shared in-tree header → `fatal error: nros/nros_generated.h: No such file`.
  Fixed by **committing `nros_generated.h`** (symmetric with the already-committed
  `nros-cpp/include/nros/nros_cpp_ffi.h` — it's always present on a fresh
  checkout) **+ an atomic build.rs write** (temp-then-rename, content-idempotent)
  so parallel regen never truncates it. The local serial `build-all` never hit
  this (an earlier nros-c build always wrote the header first).
- **rustup `-Z build-std` cold-start race (nuttx).** Parallel `cargo +nightly`
  build-std invocations raced rustup's shared `downloads/` dir on first component
  fetch (`could not rename '…partial'`). `build-fixtures` now does one **serial
  `rustup component add rust-src cargo rust-std`** for the pinned nightly before
  the parallel builds (keeps them parallel).
- **CI base image deps** the container lacked: newlib arm-gcc 13.2 (apt's is
  headerless), `libslirp0`/`libpixman` (prebuilt qemu), `picolibc-riscv64-unknown-elf`
  (threadx rv64), `ros-humble-example-interfaces` (qemu service/action codegen),
  `unzip`/`flex`/`bison` (NuttX apps fetch), GNU `parallel`, `python3-tomli`,
  the pinned `nightly-2026-04-11` + cross targets (`riscv64gc`, …).
- **Misc per-platform build fixes:** freertos `.eh_frame`/`.eh_frame_hdr` FLASH
  slot in `mps2_an385.ld` (newer rust-lld emits them); qemu test profile-dir +
  `build-fixtures` dependency; freertos cyclone fixtures idlc-gated (skip without
  a host idlc); `nros setup --source px4-rs` before the e2e nextest metadata pass.

**e2e** runs nightly (`schedule: 0 7 * * *`) + on `workflow_dispatch`; push/PR are
build-only. The residual per-platform e2e failures are the **same triaged set as
the `test-all` Group-B items above** (timing-sensitive runtime — e.g. `rtos_e2e`
NuttX C service — cross-ref archived Phase 200 / 177.2), not container-specific
regressions.

## Acceptance

- [x] `just setup all` succeeds from a deinit'd tree
- [x] `just check` green from clean
- [x] `just build-all` green from clean (0 failures)
- [x] `test-all` miri step green (clock_gettime gated)
- [x] stale `_test-orchestration-e2e` call removed
- [x] zephyr-shell passes (sibling-workspace resolver)
- [x] `build-all-full` removed (make path is opt-in `just nuttx build-fixtures-make`)
- [x] make-fixture cpp `app_config.h` always generated (one of its gaps)
- [ ] nuttx-make linkable in `build-all` — needs the tier-3 nros-c-on-NuttX cross-build (cmake skips nros-c for NuttX); make path stays opt-in until then
- [x] px4 template — **fixed (shallow).** Root cause: the `[source.px4-autopilot]`
      pin `ecfe44a` (1.15.x) lags PX4's `main` and isn't an advertised ref, so
      `git submodule update --depth 1` (fetches the branch tip, not the SHA)
      couldn't reach it → empty checkout, no `Makefile`. Fix in two parts:
      (1) **nros ≥ 0.3.7** — on a failed shallow submodule update, fall back to an
      explicit depth-1 fetch-by-SHA of the gitlink commit (GitHub serves reachable
      SHAs; verified the PX4 top snapshot carries the `Makefile`). (2) index keeps
      `shallow = true` + `recursive = false` (PX4's ~50 own sub-submodules stay
      shallow via `just px4 setup`). Top is a depth-1 snapshot, not a full clone.
- [x] qemu-baremetal BSP — `test_qemu_bsp_pubsub_e2e` runs real ethernet pub/sub
      over QEMU slirp (no Docker), gates cleanly on the ARM toolchain / qemu /
      zenoh-pico-arm / fixtures; in the `qemu-baremetal-shared` group (port 7450).
      Replaced the two `_starts` blanket-skips. Verified: published>0, received>0.
- [ ] threadx-cyclonedds — experimental/opt-in (env `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1`); decide whether to enable by default
- [ ] Group-B runtime e2e stabilized (cross-ref archived Phase 200 / 177.2)

## Notes

- Baseline: 2026-05-30, `main`, nros 0.3.1, fresh `nros setup all`.
  `just test-all` → `773 run, 764 passed (5 flaky), 9 failed, 7 skipped`.
- The 5 "flaky" (passed on retry) overlap the Group-B set + the emulator BSP /
  qemu-serial e2e — all timing-sensitive, not deterministic failures.
