---
id: 80
title: param_persistence disabled — config surface off until embedded ParamStore backends land
status: open
type: tech-debt
area: orchestration
related: [phase-256, rfc-0004, phase-172]
---

## Decision (2026-06-18)

`param_persistence` is **kept as a feature but DISABLED at the config surface**
until it is complete. It is in scope (it is the durable half of `param_services`,
a real ROS 2 feature — the embedded analog of `ros2 param dump` + launch-yaml
reload, which embedded has no launch file at boot to do). But it is **incomplete**:
only the hosted `file` backend exists, and there are **0 real users**. Rather than
ship a half-working config option users can author against, the config surface is
turned off; the design + runtime seam are kept for re-enable.

## How it works (the design — keep)

`nros-params` defines the platform seam:

```rust
pub trait ParamStore {                              // object-safe, no_std
    fn load(&self, apply: &mut dyn FnMut(&str, ParameterValue));   // boot: overlay persisted
    fn save(&mut self, params: &mut dyn Iterator<...>) -> Result<(), ParamStoreError>;
}
```

Flow: declare compile-time defaults → `load()` overlays persisted overrides → a
runtime `~/set_parameters` → `save()` flushes back. The trait is the platform
boundary; the board supplies the concrete store (like RMW / transport / BoardEntry):

| Target | Storage | `ParamStore` impl |
| --- | --- | --- |
| Hosted / NuttX / Zephyr+littlefs | filesystem | `FileParamStore` — **exists** |
| ESP32 / Zephyr NVS | KV flash | NVS backend — **TODO** |
| Bare-metal Cortex-M | internal/SPI flash | over `embedded-storage` (`NorFlash`) + `sequential-storage` (KV) — **TODO** |
| no storage | — | `NullParamStore` (no-op default) |

Config (when re-enabled) selects, the board advertises support (a board feature /
descriptor, like `capability_features`), codegen wires the store; a board with no
storage + a declared `[param_persistence]` → build error (descriptor-gated).

## What "disabled" means in code (done)

- The typed `SystemParamPersistence` + `SystemToml::param_persistence` field are
  **removed** — `deny_unknown_fields` now REJECTS `[param_persistence]` in
  `system.toml`.
- The planner no longer emits `param_persistence` to the plan
  (`collect_param_persistence` removed); `apply_param_persistence` stays a no-op.
- Dropped from the `nros config show` / `nros check` audit block lists.
- **Kept dormant:** `nros-params` `ParamStore` / `FileParamStore`; the codegen path
  (`render_param_persistence_fn` / `apply_param_persistence`) + its generate tests.

## Re-enable criteria

1. An embedded `ParamStore` backend (NVS and/or `embedded-storage`-based flash).
2. Per-backend typed config (`backend = "file"|"nvs"|"flash"` + `path` vs
   `partition`/`offset`) on a restored `SystemParamPersistence`.
3. Board-descriptor advertisement + the gated lowering.
4. A proving fixture (a node persisting + reloading a param across restart).
