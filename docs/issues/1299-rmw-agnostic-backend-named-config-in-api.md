---
id: 1299
title: "Backend-named configuration in the API crates — `entry_config.h` / Zephyr `app_config.h` read `CONFIG_NROS_ZENOH_*` / `XRCE_AGENT_*` / `CYCLONE_*`, and `nros::env` still honours `ZENOH_LOCATOR` / `ZENOH_MODE`"
status: open
type: tech-debt
area: api, config, rmw
severity: low
found: 2026-09-11
related: [1219, 0934, RFC-0071, phase-444]
---

## What

`check-rmw-agnostic` (issue 1219, phase-444 W4.b) baselines these under this
issue. They are RFC-0071 D8's class — a configuration surface keyed on a
backend's name instead of on the resolved backend — inside crates the RFC says
name none:

| file | lines | shape |
| --- | ---: | --- |
| `packages/api/nros-c/include/nros/entry_config.h` | 6 | locator from `CONFIG_NROS_ZENOH_LOCATOR`, else `CONFIG_NROS_RMW_XRCE` + `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}`; domain from `CONFIG_NROS_CYCLONE_DOMAIN_ID` under `NROS_RMW_CYCLONEDDS` |
| `packages/api/nros-c/include/nros/zephyr/app_config.h` | 9 | a `.zenoh` struct member, `zenoh_{read,lease}_{priority,stack_bytes}` fields, `CONFIG_NROS_ZENOH_LOCATOR` default |
| `packages/api/nros/src/env.rs` | 3 | the deprecated `ZENOH_LOCATOR` / `ZENOH_MODE` aliases for `NROS_LOCATOR` / `NROS_SESSION_MODE` |

A fifth backend with its own locator or task knobs has nowhere to put them
without editing these files. Issue 0934 maps the wider redundancy (R15 is the
`[knobs.zenoh.tx]` twin); this issue is only the API-crate slice the gate sees.

## Fix direction

Generic knobs (`NROS_LOCATOR`, `NROS_DOMAIN_ID`, transport task
priorities/stacks by ROLE) with the backend-specific ones mapped onto them by the
backend's own Kconfig / descriptor; the env aliases retire on the deprecation
schedule they already print. Lower the `BASELINE` rows in
`scripts/check-rmw-agnostic.py` as lines go.
