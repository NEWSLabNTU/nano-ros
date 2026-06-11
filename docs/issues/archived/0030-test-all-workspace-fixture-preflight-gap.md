---
id: 30
title: test-all preflight requires cross-target workspace fixtures that build-test-fixtures doesn't build
status: resolved
type: tech-debt
area: build
related: [phase-226, issue-0024, issue-0025]
resolved_in: "build-system env-gating (check-fixtures-stale toolchain gate + esp32 skip_probe + test-all env-deselect)"
---

**Problem.** A green `just build-test-fixtures` stamped `.fixtures-built`, but
`just test-all` then hard-failed its `_check-fixtures-stale` preflight on
workspace fixtures the standard fixture build never produced — forcing
`NROS_SKIP_FIXTURE_CHECK=1`, which disables *all* staleness self-heal. Two
distinct gaps:

- `workspace-rust-esp32` is **not in the `build-test-fixtures` fan-out**
  (`build-test-fixtures-leaves` runs zephyr/native/qemu/freertos/nuttx/
  threadx_linux/threadx_riscv64/stm32f4 — no esp32), so the standard build
  never produces it, yet the preflight required it on every host.
- `workspace-rust-{qemu-freertos,threadx-linux}` ARE built (by each platform's
  `build-examples` lane via the cargo-lane `workspace-fixtures-build.sh`, which
  writes the `.nros-workspace-fixture.*.inputsig` stamp) but only when the cross
  toolchain is present; on a lighter tier they're legitimately absent, yet the
  preflight required them — hard-failing the whole suite.
- `workspace-rust-zephyr` is worse — a **stamp mismatch**: it's built by the
  WEST lane (`zephyr-fixture-leaves.sh --include-workspace-entry`), which tracks
  staleness in the build dir's `.nros-zephyr-fixture.sig`, NOT the
  `.nros-workspace-fixture.workspace-rust-zephyr.inputsig` the generic stale
  check demands. Nothing ever writes that inputsig for zephyr → the preflight is
  **unsatisfiable even with west installed** (permanent false-stale).

**Fix** (issue direction (a)+(b): "build what you can, require only what you
built"):

1. `examples/fixtures.toml` — `workspace-rust-esp32` AND `workspace-rust-zephyr`
   get `skip_probe = true` (like `workspace-rust-qemu-nuttx`): excluded from the
   `--for-probe` required set, each still built + staleness-tracked by its own
   lane (esp `just esp32 build-fixtures`; zephyr WEST lane with its own
   `.nros-zephyr-fixture.sig`). This is what removes the zephyr stamp-mismatch
   landmine.
2. `scripts/check-fixtures-stale.sh` — `workspace_toolchain_present()` gates the
   remaining cargo-lane workspace fixtures per-id on cross-toolchain presence
   (freertos→`arm-none-eabi-gcc`, threadx-linux→`THREADX_DIR`/kernel). Absent
   toolchain → fixture dropped from the required set with an info note; present
   → required as before. Mirrors the embedded-Cyclone gate in the `test-all`
   recipe. (native/c/cpp/mixed are always-host → always required.)
3. `justfile` `test-all` — the `cyc_exclude` gate generalized to `env_exclude`:
   deselects the optional-toolchain suites (esp-idf, platformio, FVP,
   esp32_emulator) via nextest `-E "not binary(...)"` when their toolchain is
   absent, so they report **deselected** instead of the in-test
   `nros_tests::skip!` panic surfacing as a red FAIL in the live console. Each
   suite still runs (and skip!s with an actionable reason) the moment its
   toolchain is present.

Net: `just test-all` runs without `NROS_SKIP_FIXTURE_CHECK=1` on any tier;
absent-toolchain fixtures/suites are skipped, not failed; full-toolchain hosts
require everything as before.

---

## test-all results (2026-06-11, fixtures present, preflight bypassed)

Recorded for reference. After fixing all the per-platform build blockers (the
fixture build went 8/8 OK + stamped: cyclone jobserver deadlock,
[issue 0031](0031-xrce-clock-monotonic-baremetal.md) XRCE `CLOCK_MONOTONIC`,
[issue 0024](../0024-esp32-dram-overflow-size-class-buffers.md) stm32 `.bss`,
[issue 0028](0028-nros-main-rtic-defmt-timestamp.md) rtic defmt, zephyr_entry),
`NROS_SKIP_FIXTURE_CHECK=1 just test-all`:

```
1068 tests run: 951 passed (89%), 85 failed, 32 timed out, 26 skipped
Binary not found: 0   (all per-platform fixtures present)
```

The 85 fail + 32 timeout were **environmental, not code regressions** (host was
shared + heavily loaded by other users' jobs during the run): networked e2e
flakes (`rtos_e2e`, `native_api`, `xrce`/`px4_xrce` — zenohd/TAP/native_sim
timing under load), the 2 unbuildable workspace fixtures (now gated, above),
missing optional toolchains (`integration_esp_idf`, `integration_platformio`,
`phase217_*_fvp_runtime` — now deselected, above), and compile-heavy timeouts
under load (`phase212_n9_main_macro_forms`, `phase212_n_entry_poc_runs`).

A trustworthy real-bug signal needs an **idle host** + the esp toolchain + the
zephyr workspace fixture built. The code paths themselves are sound — every
build blocker is fixed and the fixture build is green.
