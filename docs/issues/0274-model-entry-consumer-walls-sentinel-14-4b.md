---
id: 274
title: "Model-arm entry consumer walls from the sentinel 14.4b port: bounded hosted spin, param-services node identity, resolver gaps"
status: resolved
type: enhancement
area: codegen
related: [issue-0269, issue-0272]
---

## Resolution (2026-07-25, same day)

All four walls closed in the follow-up commit:

1. `spin = "forever"` macro arg (hosted arms emit an unbounded spin) +
   `NROS_ENTRY_SPIN_MS=forever` env spelling. The bounded-default stays
   for the e2e fixtures that rely on register-and-exit.
2. `[param_services] node = "<name>"` in the bringup's system.toml names
   the executor identity (primary-session node_name → param-service FQN
   + liveliness). Explicit value wins over the single-node auto-name.
3. The model arm now reads `[param_services]` straight from
   `<bringup>/system.toml` (tracked file), so the resolver's missing
   `execution.features` no longer gates the feature.
4. Float lowering (fixed in the previous commit).

Verified by the reporting consumer: autoware_sentinel's 12-node entry
runs `spin = "forever"` with `/sentinel/{list,get}_parameters` fully
discoverable and typed, no env workarounds, 15/15 integration tests.

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
