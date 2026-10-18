---
id: 1299
title: "Backend-named configuration in the API crates — `entry_config.h` / Zephyr `app_config.h` read `CONFIG_NROS_ZENOH_*` / `XRCE_AGENT_*` / `CYCLONE_*`, and `nros::env` still honours `ZENOH_LOCATOR` / `ZENOH_MODE`"
status: resolved
type: tech-debt
area: api, config, rmw
severity: low
found: 2026-09-11
resolved: 2026-10-18
related: [1219, 0934, RFC-0071, phase-444, phase-506]
---

**Home: [phase-506](../../roadmap/phase-506-api-crates-carry-only-the-api.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What

`check-rmw-agnostic` (issue 1219, phase-444 W4.b) baselines these under this
issue. They are RFC-0071 D8's class — a configuration surface keyed on a
backend's name instead of on the resolved backend — inside crates the RFC says
name none:

| file | lines | shape |
| --- | ---: | --- |
| `packages/api/nros-c/include/nros/entry_config.h` | 6 | locator from `CONFIG_NROS_ZENOH_LOCATOR`, else `CONFIG_NROS_RMW_XRCE` + `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}`; domain from `CONFIG_NROS_CYCLONE_DOMAIN_ID` under `NROS_RMW_CYCLONEDDS` |
| `packages/api/nros-c/include/nros/zephyr/app_config.h` | 9 | a `.zenoh` struct member, `zenoh_{read,lease}_{priority,stack_bytes}` fields, `CONFIG_NROS_ZENOH_LOCATOR` default |
| `packages/api/nros/src/retired_env.rs` | 2 | the RETIRED `ZENOH_LOCATOR` / `ZENOH_MODE` refusal table (phase-484 W8c retired the aliases `env.rs` and `host_env.rs` honoured, 3 + 2 lines; the refusal must still spell the name it refuses) |

A fifth backend with its own locator or task knobs has nowhere to put them
without editing these files. Issue 0934 maps the wider redundancy (R15 is the
`[knobs.zenoh.tx]` twin); this issue is only the API-crate slice the gate sees.

## Fix direction

Generic knobs (`NROS_LOCATOR`, `NROS_DOMAIN_ID`, transport task
priorities/stacks by ROLE) with the backend-specific ones mapped onto them by the
backend's own Kconfig / descriptor; the env aliases retire on the deprecation
schedule they already print. Lower the `BASELINE` rows in
`scripts/check-rmw-agnostic.py` as lines go.

## Resolved (2026-10-18) — RFC-0071 D10, phase-506 W3

* **`entry_config.h` reads only the ROLES.** `NROS_ENTRY_LOCATOR` falls to
  `""` and `NROS_ENTRY_DOMAIN_ID` to `CONFIG_NROS_DOMAIN_ID`. Each backend's
  Zephyr module maps its own Kconfig onto them with `zephyr_compile_definitions`:
  zenoh's `CONFIG_NROS_ZENOH_LOCATOR`, XRCE's `addr:port` (the synthesis issue
  #174 had put in the header), and Cyclone's own domain knob. The RTOS roads
  already defined both from `nano_ros_entry()`.
* **`nros_app_config_t` names roles.** `.zenoh` is `.transport`, and the
  scheduling fields `zenoh_{read,lease}_{priority,stack_bytes}` are
  `{rx,keepalive}_{priority,stack_bytes}`, in the header, every emitter
  (`freertos_build.rs`, `threadx_qemu_riscv64_build.rs`, both ThreadX board
  cmakes, `freertos_app_config.c.in`) and the FreeRTOS C entry that reads them.
  The Zephyr initializer now takes the roles too, so an XRCE or Cyclone image's
  `.transport.locator` is its own instead of always `""`.
* **`retired_env.rs`** is an EXEMPTION, not debt: it is the refusal table for
  the retired `ZENOH_LOCATOR` / `ZENOH_MODE` env spellings, and a refusal must
  name what it refuses.

Measured on built images: the Zephyr C zenoh and XRCE talkers carry
`NROS_ENTRY_LOCATOR="tcp/127.0.0.1:7500"` and `"127.0.0.1:2500"` from their
backend modules; `zephyr example_e2e` zenoh and xrce C pub/sub pass; the FreeRTOS,
ThreadX-linux and ThreadX-riscv64 C workspace entries build (designated
initializers, so a stale field name is a compile error); the ThreadX-linux and
Zephyr C `entry_e2e` cells pass. All three `check-rmw-agnostic` rows are gone.
