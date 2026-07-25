---
id: 274
title: "Model-arm entry consumer walls from the sentinel 14.4b port: bounded hosted spin, param-services node identity, resolver gaps"
status: open
type: enhancement
area: codegen
related: [issue-0269, issue-0272]
---

## Findings (autoware_sentinel phase-14.4b — first external 12-node model-arm consumer, 2026-07-25)

Porting the sentinel onto `nros::main!(model = ...)` (12 Node-trait
wrappers, one shared-state chain) surfaced four walls; the float one is
fixed in the commit alongside this doc, the rest are open:

1. **Hosted entries only spin when `NROS_ENTRY_SPIN_MS` is set** — the
   default is register-and-exit ("nros: application complete" after ~0 ms).
   Fine for e2e fixtures, a trap for a production entry: the sentinel now
   ships `NROS_ENTRY_SPIN_MS=31536000000000` (1000 years) as an unbounded
   stand-in. Ask: an unbounded default (or an explicit `spin = "forever"`
   macro arg) for hosted deploys.

2. **`[param_services]` needs a node identity the multi-node entry never
   sets** — `register_parameter_services()` derives its FQN and (crucially)
   its liveliness attribution from the EXECUTOR's node_name, which the
   model arm leaves empty for multi-node systems. The 6 services then exist
   but are invisible to rmw_zenoh discovery (no NN/entity token). The
   `NROS_NODE_NAME` env rung (RFC-0045 model A) is the workaround; the fix
   is a first-class identity for the param-services host node (e.g.
   `[param_services] node = "sentinel"` in system.toml, threaded through
   the model).

3. **`play_launch resolve` does not populate `execution.features`** — a
   `[param_services]` block in system.toml never reaches the model, and the
   macro gates `apply_param_services` on `features: [param_services]`. The
   sentinel hand-appends the axis post-resolve. One of the two sides needs
   to own this (resolver emits it, or the macro reads system.toml).

4. **FIXED here — float params lowered to ints**: the model arm baked
   `ParamValue::Float(f)` via `f.to_string()`, so a launch `value="1.0"`
   became "1" and the runtime's `infer_param_value` re-typed it INTEGER
   (`pid.kp` came back as int 1 over `/sentinel/get_parameters`). Now
   `format!("{f:?}")` keeps the ".0".

Also documented for consumers (not a bug): one SystemModel carries ONE
placement per node — multi-target `[deploy.*]` rosters in a single
system.toml clobber each other (last wins) and the board slice keeps an
unexpected subset. Per-target models ("one fully-resolved artifact per
concrete arg-set", phase-296) are the intended shape.
