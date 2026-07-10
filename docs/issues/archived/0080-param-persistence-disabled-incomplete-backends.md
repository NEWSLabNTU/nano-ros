---
id: 80
title: param_persistence disabled — config surface off until embedded ParamStore backends land
status: wontfix
type: tech-debt
area: orchestration
related: [phase-256, rfc-0004, phase-172]
resolved_in: "design decision 2026-07-10 (no on-device persistence)"
---

## Resolution (2026-07-10) — WONTFIX: on-device param persistence is a non-goal

**nano-ros does not persist parameters on-device.** The durable-param feature this
issue tracked (runtime `~/set_parameters` → flush to flash/NVS → reload on next
boot) is **out of scope by design** — embedded targets do not carry a mutable
parameter store.

The supported model is **build-time param baking from launch files**: parameters
are authored in the launch/`system.toml` layer and the build system bakes them as
the node's **default values**. This already works and needs no persistence backend:

- `packages/cli/nros-cli-core/src/orchestration/params.rs` merges
  `parameter_defaults` + `parameters/defaults` (launch params override source
  defaults — test `launch_params_override_source_defaults`).
- Codegen emits the baked defaults into the generated entry as `declare_param` /
  `nros_cpp_declare_param(executor, "<k>", "<v>")` calls
  (`src/codegen/entry/emit_c.rs`).

So a device boots with exactly the params its launch file declared — the embedded
analog of launch-yaml, resolved at build time instead of at boot. Runtime
`get`/`set`/`describe` (the *services* half, `nros-params` `server.rs`) stay; only
the **persistence** half is dropped.

The original re-enable criteria (NVS / `embedded-storage` flash backends, typed
per-backend config, board-descriptor gating, a persist-across-restart fixture)
are **not going to be pursued**.

## Dead code (follow-up cleanup, optional)

The persistence seam was kept dormant "for re-enable"; with re-enable now a
non-goal it is dead code. It is **harmless meanwhile** — the executor's
`store: Box<dyn ParamStore>` defaults to `NullParamStore` (a no-op; the per-tick
`save()` does nothing) and the config surface already rejects `[param_persistence]`
(`deny_unknown_fields`). A later cleanup may remove:

- `packages/core/nros-params/src/persist.rs` (`ParamStore`, `NullParamStore`,
  `FileParamStore`) + the `pub use` in `lib.rs`.
- `nros-node`: the `store` field + `Executor::enable_parameter_persistence` +
  the per-tick `save`-on-dirty call in `parameter_services.rs` / `spin.rs`.
- The dormant codegen path (`render_param_persistence_fn` / `apply_param_persistence`)
  + its generate tests.

Left in place for now (no runtime cost, no config surface); tracked here rather than
kept as an open issue.

---

## Original issue (2026-06-18) — for context

`param_persistence` was kept as a feature but DISABLED at the config surface
pending completion (only the hosted `file` backend existed, 0 real users). The
`ParamStore` trait was the platform seam (board supplies the concrete store, like
RMW / BoardEntry); config would select the backend and codegen would wire it. That
direction is superseded by the 2026-07-10 decision above: no persistence, launch
params baked as defaults.
