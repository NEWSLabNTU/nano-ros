---
id: 241
title: "RMW QoS boundary conversions silently lose the user's value: depth u32→u16 saturates, rmw_time_t durations clamp to u32 ms with 0 overloaded as infinite"
status: open
type: bug
severity: low
area: rmw
related: [issue-0240]
---

## Finding (RMW/platform API audit, 2026-07-21)

`impl From<QosSettings> for NrosRmwQos` (`nros-rmw-cffi/src/lib.rs:409`)
silently reshapes values the user explicitly set:

- **`depth` u32 → u16 SATURATES** at 65535 (`lib.rs:428`). A user asking
  for a history depth > 65535 silently gets 65535, not an error. (The u16
  narrowing itself is a deliberate 2-byte-per-entity saving,
  `rmw_entity.h:84-87` — the SILENT clamp is the bug.)
- **`deadline`/`lifespan`/`liveliness_lease` → `uint32_t` milliseconds**
  (`rmw_entity.h:101-109`) from the upstream `rmw_time_t {sec,nsec}`.
  Loses sub-ms resolution and caps the range at ~49.7 days; `0` is
  overloaded as "infinite" (upstream uses a distinct infinite sentinel),
  so a legitimate 0-duration request is indistinguishable from "unset".

## Fix direction

A QoS value the caller set that the ABI cannot represent should be a
create-time error (`NROS_RMW_RET_INVALID_ARGUMENT` /
`IncompatibleQos`), consistent with the no-silent-downgrade QoS philosophy
already in place (`QosPolicyMask` + `Session::supported_qos_policies` +
`validate_against`, `traits.rs:1163-1254`) — that machinery rejects
unsupported POLICIES loudly but the numeric width/precision clamps slip
through under it. Either reject the out-of-range value or document the
representable range at the API and clamp explicitly-with-a-warning. Fold
the `0`-vs-infinite ambiguity fix in (a dedicated "unset" sentinel).
