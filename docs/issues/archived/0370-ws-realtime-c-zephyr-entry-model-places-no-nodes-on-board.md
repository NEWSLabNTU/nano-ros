---
id: 370
title: ws-realtime-c zephyr entry rejected — system model places no nodes on board `zephyr`
status: resolved
resolved_in: issue-0288-board-filter
type: bug
area: build
related: [rfc-0060, issue-0361]
---

# 0370 — ws-realtime-c zephyr entry rejected: model places no nodes on board `zephyr`

**Status:** Resolved (2026-08-01)
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

Environment note: found while running tier-1 ci for the unrelated #377 fix
(the commit says `fix(#367)` — that issue was renumbered 0367 → 0372 → 0377
after two id collisions; see `0377-*`)
(cyclone Kconfig wiring); the failure reproduces with that change absent
from the build path (it touches `packages/rmw/cyclonedds` + `zephyr/Kconfig`
only). Tier-1 could not be driven fully green on this box because of it.

## Resolution (2026-08-01)

Already fixed on main. The second of the two hypotheses above was right: **the
entry codegen's board selection lost these nodes**, and the model did not need
re-resolving.

`ws-realtime-c` DOES declare `[deploy.zephyr]`, but embedded blocks are excluded
from placement (they are whole-system board builds, not machines — several of
them cannot partition nodes between themselves). So the model's
`execution.deploy` only ever names `linux`, and the C/C++ emitter's board filter
dropped every node for `--board zephyr`.

The rule it was missing: a board the deploy map never MENTIONS is not "a board
with nothing on it", it is a board the model has no opinion about — which is
exactly what the emitter's own `(None, _) => true` arm already said per-NODE,
lifted to the board level. `nros-macros`'s `main_macro.rs` (the Rust emitter)
had been taught this; `codegen/entry/mod.rs` had not. Two emitters, one rule —
the class filed as issue 0358.

Verified by negative control, not by inspection:

```
with the board_mentioned rule   -> passes the board check, proceeds to
                                   metadata matching
rule removed, CLI rebuilt       -> "places no nodes on board `zephyr`"
                                   — this issue's error, verbatim
```

The environment note in this issue is consistent: the failure appeared after a
`just setup-cli` on a rebase that carried the resolver change but not the
emitter fix, so the model stopped naming `zephyr` while the emitter still
demanded it.

## Repro

```
just setup-cli && just build-test-fixtures
```
