---
id: 152
title: "Per-lane env setup gaps surfaced by the 2026-07 resync — qemu logging, px4, zephyr-cyclone fixtures, zephyr_self_pkg, integration lanes"
status: open
type: tech-debt
area: testing
related: [issue-0149, issue-0150, issue-0151, issue-0154, issue-0155]
---

## Triage result (2026-07-08) — most lanes were buildable or stale, three real trackers remain

**RESOLVED by building (verbs recorded):**
- esp32 logging image → `just esp32 build-logging-smoke`; `logging_smoke_esp32_qemu` green.
- px4 fixtures (`px4-stub`, `offboard-companion`) → plain cargo builds in
  `examples/px4/rust/xrce/*` (`--profile nros-fast-release --target-dir
  target-xrce --no-default-features --features rmw-xrce`); PX4 checkout was
  present all along.
- zephyr-cyclone images → `NROS_ZEPHYR_FIXTURE_FILTER=cyclonedds just zephyr
  build-fixtures` (build succeeds; the TESTS still fail → issue 0155).

**RESOLVED as stale-object mixing (wipe + fresh configure, the mixed_qos
rule):** all five remaining mixed lanes — mixed_logging, mixed_service_
roundtrip, mixed_custom_msg, mixed_multihost, mixed_freertos — went green
after `rm -rf examples/workspaces/{mixed,ws-custom-msg-mixed}/build-*` +
lane rebuild. Core-struct changes ⇒ wipe workspace build dirs.

**Split to their own issues:**
- issue 0154 — phase-258 retired the `system_main.c` emit; `nros_system_
  generate.cmake` + `west-fixtures.sh` + `zephyr_self_pkg`/`self_bringup`
  tests still require it (design-level migration, covers the
  `zephyr_self_pkg` + `self_bringup` + west-bringup rows here).
- issue 0155 — zephyr+cyclonedds images boot then go silent (all 8
  phase_118 tests, fresh images; needs stash-baseline debug).

**Still open in this issue (final state, 2026-07-08 second pass):**
- `bins/logging-smoke-nuttx-qemu-arm` — BUILD SOLVED: the row has no `rmw`
  field, so any rmw-filtered lane invocation (`fixtures-build.sh nuttx rust
  zenoh`, which `just nuttx build-fixtures` effectively is) filters it out;
  the UNFILTERED `scripts/build/fixtures-build.sh nuttx rust` builds it
  (also: `--id` only matches workspace rows — plain rows carry no id).
  Recipe follow-up: drop the rmw arg in the recipe or add `rmw = "zenoh"`
  to the row. The TEST still fails: QEMU nuttx-virt boots the image and
  emits NOTHING in 45 s — same silent-image signature as
  `rust_nuttx_entry_e2e` below.
- `rust_nuttx_entry_e2e` — nextest override LANDED (joined the `qemu-nuttx`
  group: it hardcodes the group's port 7452, and the default 60 s kill is
  shorter than a NuttX QEMU boot; now 120 s ×3). Past the timeout the test
  fails for real: "native observer never received /chatter — entry-path
  eth0". Phase-281's own session proved this lane green hours earlier
  (a2e11459d), and both this and the logging image go SILENT after my
  resync rebuilds — the nuttx board/image plumbing is mid-rework in that
  stream (see also issue 0149). COORDINATE with phase-281 rather than
  debugging its moving lane; re-verify both after it settles.
- `migrate_workspace` — DESIGNED skip: the released nros-cli pin lags the
  post-212.I emitter spec (Phase 214.N drift gate); clears on the next CLI
  release, nothing to do locally.
- integration_zephyr / integration_platformio / qos_zephyr_ros2 /
  params_zephyr / safety_zephyr / px4 ×2: GREEN (treadmill / built
  fixtures).

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
