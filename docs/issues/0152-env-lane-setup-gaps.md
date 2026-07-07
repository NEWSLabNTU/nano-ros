---
id: 152
title: "Per-lane env setup gaps surfaced by the 2026-07 resync — qemu logging, px4, zephyr-cyclone fixtures, zephyr_self_pkg, integration lanes"
status: open
type: tech-debt
area: testing
related: [issue-0149, issue-0150, issue-0151]
---

## Summary

Residue of the 2026-07-08 env-resync (fail-loud #133 semantics = missing env
shows as FAILED): test families whose lane needs a machine-setup pass or a
lane-owned fixture recipe that the standard
`just build-test-fixtures` does not cover. Each needs its lane's setup verb
run (and, where none exists, a recipe home):

- `logging_smoke_esp32_qemu_*` / `logging_smoke_nuttx_qemu_arm_*` — the
  esp32/nuttx QEMU logging images; check `just esp32 build-fixtures` /
  `just nuttx build-examples` cover them and that qemu-system deps are
  provisioned.
- `px4_xrce` — needs the PX4-Autopilot checkout (`PX4_AUTOPILOT_DIR`) and
  its SITL build; unprovisioned here.
- `phase_118_collapse` zephyr-CYCLONE lanes — zephyr cyclonedds fixture
  images (`build-rs-{talker,listener,service-server}-cyclonedds`) are their
  own `just zephyr build-fixtures` arm with the cyclone env; they were stale
  or unbuilt through the resync.
- `zephyr_self_pkg`, `integration_zephyr`, `integration_platformio`,
  `self_bringup`, `migrate_workspace`, `qos_zephyr_ros2_interop_e2e`,
  `params_zephyr_entry_e2e`, `safety_zephyr_entry_e2e`,
  `rust_nuttx_entry_e2e`, `mixed_{multihost,logging,custom_msg,
  service_roundtrip,freertos}_*` — assorted lane fixtures/e2e; re-triage
  AFTER a quiet-window full rebuild (most looked like the mtime treadmill;
  see `env_machine_test_debt` memory) and file specific issues for whatever
  survives.

Rule of engagement (memory `env_machine_test_debt`): rebase once → full
fixture rebuild → run WITHOUT pulling; only failures that survive that are
real.

Also: `just nuttx build-fixtures` does not build the nuttx WORKSPACE
fixtures at all (they live only in `build-examples`) — the fixture verb
should cover everything `examples/fixtures.toml` declares for its platform
(cross-ref issue 0149).
