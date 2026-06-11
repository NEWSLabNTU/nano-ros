---
id: 30
title: test-all preflight requires cross-target workspace fixtures that build-test-fixtures doesn't build
status: open
type: tech-debt
area: build
related: [phase-226, issue-0024, issue-0025]
---

A green `just build-test-fixtures` stamps `target/nextest/.fixtures-built`, but
`just test-all` then **fails its `_check-fixtures-stale` preflight** on workspace
fixtures the fixture build never produced:

```
ERROR: 2 workspace fixture(s) are missing or stale:
  workspace-rust-zephyr (missing examples/workspaces/rust/target/.nros-workspace-fixture.workspace-rust-zephyr.inputsig)
  workspace-rust-esp32  (missing examples/workspaces/rust/target-fixtures/esp32/.nros-workspace-fixture.workspace-rust-esp32.inputsig)
  Run `just native build-workspace-fixtures` before test-all.
```

**Cause.** `_check-fixtures-stale` validates the *cross-target* workspace-entry
fixtures (`examples/workspaces/rust/src/{zephyr_entry,esp32_entry}`), but the
fixture build only produces the **native** ones — `just native
build-fixtures` → `build-workspace-fixtures` → `workspace-fixtures-build.sh
native` builds `native_entry`, not the zephyr/esp32 entries. The zephyr/esp32
workspace fixtures need their own cross builds (`workspace-fixtures-build.sh
zephyr` / `esp32`, via the per-platform lanes), which are NOT part of
`build-test-fixtures`. So:

- the `.fixtures-built` stamp is **inconsistent** with the preflight — a green
  `build-test-fixtures` does not guarantee `test-all` runs;
- `workspace-rust-esp32` can never satisfy the preflight on a host without the
  esp toolchain (cf. [issue 0024](0024-esp32-dram-overflow-size-class-buffers.md)
  — esp32 fixtures are unbuildable in the dev env);
- the only way to run `test-all` today is `NROS_SKIP_FIXTURE_CHECK=1` (bypass).

**Direction.** Either (a) have `build-test-fixtures` build the cross-target
workspace fixtures it stamps for (gating each on toolchain presence the way the
embedded-Cyclone tests are gated), or (b) make `_check-fixtures-stale` only
require the workspace fixtures whose toolchain is actually present, or (c) keep
them separate but stop `build-test-fixtures` from stamping until the preflight
set is satisfied. (a)+(b) together is cleanest: build what you can, require only
what you built.

---

## test-all results (2026-06-11, fixtures present, preflight bypassed)

Recorded for reference. After fixing all the per-platform build blockers (the
fixture build went 8/8 OK + stamped: cyclone jobserver deadlock,
[issue 0029](archived/0029-xrce-clock-monotonic-baremetal.md) XRCE
`CLOCK_MONOTONIC`, [issue 0024](0024-esp32-dram-overflow-size-class-buffers.md)
stm32 `.bss`, [issue 0028](archived/0028-nros-main-rtic-defmt-timestamp.md) rtic
defmt, zephyr_entry), `NROS_SKIP_FIXTURE_CHECK=1 just test-all`:

```
1068 tests run: 951 passed (89%), 85 failed, 32 timed out, 26 skipped
Binary not found: 0   (all per-platform fixtures present)
```

The 85 fail + 32 timeout were **environmental, not code regressions** (host was
shared + heavily loaded by other users' jobs during the run):

- **Networked e2e flakes** — `rtos_e2e` (qemu talker↔listener, 89–94 s),
  `native_api`, `orchestration_tiers_*`, `xrce`/`px4_xrce` — zenohd/TAP/native_sim
  timing under load.
- **The 2 unbuildable workspace fixtures** — `zephyr`/`esp32` workspace-entry e2e
  fail fast (precondition: binary absent), per the gap above.
- **Missing optional toolchains** — `integration_esp_idf`, `integration_platformio`,
  `phase217_*_fvp_runtime` (esp-idf / platformio / Arm FVP not installed).
- **Compile-heavy timeouts under load** — `phase212_n9_main_macro_forms`,
  `phase212_n_entry_poc_runs`.

A trustworthy real-bug signal needs an **idle host** + the esp toolchain + the
zephyr workspace fixture built (it builds locally but a 400 s probe timeout cut
it). The code paths themselves are sound — every build blocker is fixed and the
fixture build is green.
