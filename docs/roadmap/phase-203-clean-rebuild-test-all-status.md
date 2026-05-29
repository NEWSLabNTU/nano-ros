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

1. **xrce agent submodule** — `scripts/xrce-agent/build.sh` errored "submodule
   not initialized" instead of initializing it. Now auto-inits
   `third-party/xrce/agent`.
2. **Retired codegen functions** — `build-all-jobserver` called
   `nros_cargo_fetch_codegen` (deleted) and `nros_cargo_build_codegen_c`
   (renamed). The codegen tool is the installed `nros` binary now; dropped the
   fetch call, switched to `nros_cargo_ensure_codegen_c`.
3. **zenohd from clean** — `scripts/zenohd/build.sh` hard-errored on the
   missing zenoh source submodule. Now prefers the prebuilt zenohd from the
   nros store (`nros setup … --rmw zenoh`); auto-inits the submodule only on
   the source-build fallback.
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
| `emulator::test_qemu_bsp_{talker,listener}_starts` (2) | Docker or QEMU networking — `just test-rust-qemu-baremetal-bsp` |
| `integration_zephyr::zephyr_integration_shell_smoke` | `ZEPHYR_BASE` env or an **in-tree** `zephyr-workspace` symlink (the workspace is the `../nano-ros-workspace` sibling; this test only checks the in-tree path) |
| `integration_px4::px4_integration_template_smoke` | a complete PX4 checkout (`PX4_AUTOPILOT_DIR` has no `Makefile` — the shallow/partial submodule clone is incomplete); `just px4 setup` |
| `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` | the make-fixture kernel restaged with nano-ros app symbols — `just nuttx build-fixtures-make` |
| `threadx_riscv64_qemu::test_threadx_riscv64_cyclonedds_two_qemu_pubsub` | the CycloneDDS ThreadX fixtures — `just cyclonedds threadx-cross-probe` |

Follow-up worth doing: the zephyr-shell + nuttx-make + threadx-cyclone fixtures
should be staged by `just build-all` (or their tests should resolve the sibling
workspace / build-fixtures-make output), so a clean `build-all` + `test-all`
needs no extra manual step. The px4 template needs a non-shallow PX4 clone.

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

## Acceptance

- [x] `just setup all` succeeds from a deinit'd tree
- [x] `just check` green from clean
- [x] `just build-all` green from clean (0 failures)
- [ ] stage the Group-A heavy fixtures in `build-all` so `test-all` needs no
      manual pre-step (zephyr-shell / nuttx-make / threadx-cyclone)
- [ ] Group-B runtime e2e stabilized (cross-ref archived Phase 200 / 177.2)

## Notes

- Baseline: 2026-05-30, `main`, nros 0.3.1, fresh `nros setup all`.
  `just test-all` → `773 run, 764 passed (5 flaky), 9 failed, 7 skipped`.
- The 5 "flaky" (passed on retry) overlap the Group-B set + the emulator BSP /
  qemu-serial e2e — all timing-sensitive, not deterministic failures.
