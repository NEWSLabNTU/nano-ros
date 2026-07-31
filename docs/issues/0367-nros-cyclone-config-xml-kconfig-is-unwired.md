---
id: 367
title: CONFIG_NROS_CYCLONE_CONFIG_XML is declared but consumed nowhere
status: open
type: bug
area: rmw
related: [rfc-0054]
---

# 0367 — `CONFIG_NROS_CYCLONE_CONFIG_XML` is declared but consumed nowhere

**Status:** Open
**Filed:** 2026-07-31
**Affects:** `nros-rmw-cyclonedds` on Zephyr (native_sim and hardware);
any embedded target that needs a non-default Cyclone config

## Summary

`zephyr/Kconfig` declares `NROS_CYCLONE_CONFIG_XML` ("Cyclone raw config
XML (empty = default) … baked at build time") but no source file reads
`CONFIG_NROS_CYCLONE_CONFIG_XML`. The knob is dead: `session_create()` in
`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/session.cpp` picks between
the `CYCLONEDDS_URI` env var and the hard-coded `kEmbeddedCycloneConfig`
profile only.

The phase-192.4 env override is no help on Zephyr native_sim: `getenv()`
under picolibc returns nothing from the host environment, so
`zephyr.exe` silently boots with the baked profile whatever the caller
exports (verified: a garbage `CYCLONEDDS_URI` boots cleanly). Result: the
baked profile (multicast off, `MaxAutoParticipantIndex=20`, peer
127.0.0.1, no tracing) is effectively immutable on the platform that most
needs tuning — there is no way to widen the participant-index scan, change
peers, or enable Cyclone tracing without editing nano-ros source.

Found while restructuring the safety-island demo
(simple-autoware-safety-island) to put the island directly on Autoware's
domain: the island must scan ~40+ host participant indices, and the
discovery failure could not even be traced because the config is sealed.

## Fix (direction)

Wire the knob in `session.cpp`: selection order `CYCLONEDDS_URI` env (where
an environment exists) → non-empty `CONFIG_NROS_CYCLONE_CONFIG_XML` →
`kEmbeddedCycloneConfig`. Kconfig strings can't hold escaped double quotes
comfortably — XML single-quoted attributes (`Address='127.0.0.1'`) make
the blob Kconfig-safe; note that in the Kconfig help.
