---
id: 3
title: Non-configurable compile-time constants
status: resolved
type: enhancement
area: build
related: []
resolved_in: env-var configuration
---

Three user-facing constants are now configurable via environment variables:

| Env var                          | Default     | Constant                     | Crate          |
|----------------------------------|-------------|------------------------------|----------------|
| `NROS_SERVICE_TIMEOUT_MS`        | 10,000 ms   | `SERVICE_DEFAULT_TIMEOUT_MS` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `PARAM_SERVICE_BUFFER_SIZE`  | nros-node      |
| `NROS_KEYEXPR_STRING_SIZE`       | 256         | `KEYEXPR_STRING_SIZE`        | nros-rmw-zenoh |

`DEFAULT_MAX_TIMERS` was removed (dead code — timer count bounded by
`MAX_CBS`). Six internal constants remain intentionally non-configurable
(safe defaults, protocol-tied values).
