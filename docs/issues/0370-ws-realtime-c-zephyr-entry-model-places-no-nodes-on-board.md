---
id: 370
title: ws-realtime-c zephyr entry rejected — system model places no nodes on board `zephyr`
status: open
type: bug
area: build
related: [rfc-0060, issue-0361]
---

# 0370 — ws-realtime-c zephyr entry rejected: model places no nodes on board `zephyr`

**Status:** Open
**Filed:** 2026-08-01
**Affects:** `just build-test-fixtures` (zephyr family), tier-1 `just ci`
(via `_check-fixtures-stale` — fixtures can't be rebuilt to freshness)

## Summary

After a rebase to main (`471a62529`) and the mandated `just setup-cli`
rebuild, the zephyr fixture family fails at
`build-ws-c-realtime-entry-zenoh`:

```
nros codegen entry --lang c --workspace examples/workspaces/ws-realtime-c
  --model .../demo_bringup/config/system_model.yaml --board zephyr ...
Error: SystemModel `...` places no nodes on board `zephyr` —
check execution.deploy targets
```

The committed `demo_bringup/config/system_model.yaml` and the rebuilt CLI
(rlm via ros-launch-resolve, RFC-0060 line) disagree about deploy targets —
either the model needs re-resolving with the current resolver or the
entry codegen's board-selection lost these nodes. Same shape as archived
#0361 (embedded entries with no deploy target in their model).

Environment note: found while running tier-1 ci for the unrelated #367 fix
(cyclone Kconfig wiring); the failure reproduces with that change absent
from the build path (it touches `packages/rmw/cyclonedds` + `zephyr/Kconfig`
only). Tier-1 could not be driven fully green on this box because of it.

## Repro

```
just setup-cli && just build-test-fixtures
```
