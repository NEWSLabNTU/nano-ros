---
id: 185
title: "zephyr self-pkg M-F.3 shim bake half-missing (config_cmake=false) + cli_bringup zephyr/platformio lanes red"
status: open
type: bug
area: zephyr
related: [phase-287, issue-0164]
---

## Summary

Deterministic (serialized rerun, fresh fixtures 2026-07-12):

- `zephyr_self_pkg::zephyr_self_pkg_rust_builds_via_shim`:
  `M-F.3 shim bake missing under build/west-fixtures/zephyr_self_pkg_rust/
  nros-system (config_h=true, config_cmake=false) — shim regressed?`
  — the header half bakes, the cmake half doesn't.
- `zephyr_self_pkg::zephyr_self_pkg_resolve_bringup_handles_relative_path`.
- `cli_bringup_zephyr::cli_bringup_zephyr_adapter_shim_boots_native_sim`
  (fails without a panic line — build-stage failure; see test log).
- `cli_bringup_platformio::platformio_zephyr_framework_2_component_bringup_builds`.
- `zephyr::test_zephyr_workspace_entry_native_sim_e2e` — blocked on the
  stale native rust listener fixture (#181) rather than this shim; re-check
  after a clean rebuild.

## Suspects

All four lanes drive the zephyr adapter-shim / bringup path that both
phase-287 sessions touched the same week (`nros new` ament emission
`920ca54c9`, W5 preset emission in `nros setup` `07a2fdc64`, and the
deferred-platform-link change `12fbfa4a7`). The half-baked shim
(`config_h=true, config_cmake=false`) points at the shim EMITTER, not the
consumer. Bisect across those three before assuming older rot.
