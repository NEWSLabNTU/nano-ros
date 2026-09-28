---
id: 1556
title: "The twelve standalone NuttX C/C++ leaves declare their entities for a
  reason issue 1265 does not describe — no probe REACHES them, and there is no
  model either"
status: open
type: tech-debt
area: [tooling, build]
related: [1265, 1555, 1142, 0827, 1061, rfc-0098, phase-412]
---

## What

`system.toml` `[[component]] entities` is the third and last live `ENTITIES`
population (populations 1 and 2 — the cmake `ENTITIES` keyword and the
`nros-metadata.json` key — are retired and issue 1555 respectively). Issue 1265
is the standing item to retire it, and it describes ONE reason a leaf must
declare: the host metadata probe exists for this leaf and **cannot run**
(foreign `[build] target` + `build-std`, or a board crate with no host build).

**Measured 2026-09-29: 14 leaves declare, and they split 2 / 12 by reason.**

```
$ for f in $(find examples -name system.toml | sort); do
      grep -qE '^\s*entities\s*=' "$f" && echo "$f"; done
```

| leaves | who | why they declare | covered by |
| --- | --- | --- | --- |
| 2 | `examples/esp32-c3-baremetal/rust/{talker,listener}` | the Rust probe exists for them and cannot run | **1265** |
| 12 | `examples/qemu-armv7a-nuttx/{c,cpp}/*` (6 + 6) | no probe REACHES them, and no model describes them | **this issue** |

Two corrections to the survey this came from, both worth recording:

* it said 15 leaves and 10 NuttX. It is **14 and 12**.
* `examples/workspaces/sizing/src/demo_bringup/system.toml` does **not** declare
  entities — it is a bringup whose line 4 is prose (*"The launch wiring names
  zero callback entities for it"*). It matched a `git grep -l entities`, which
  is not the same measurement.

## Why 1265 does not cover the twelve

Not, as first hypothesised, "C and C++ have no probe". **A C/C++ metadata probe
does exist** — `orchestration::metadata_probe_cmake::run_probes`, which batches
every C/C++ component of a workspace into ONE cmake project (phase-313). The
`examples/workspaces/cpp/src/*_pkg/metadata/*.json.unprobeable` markers are that
probe having been attempted.

The twelve are blocked by something else, and by two things independently:

1. **The probe is WORKSPACE-scoped and these are standalone leaves.**
   `metadata_refresh::refresh_stale_sidecars` enumerates
   `Workspace::discover(ws_root).component_declarations()`. A leaf like
   `examples/qemu-armv7a-nuttx/c/talker/` is a `package.xml` + `CMakeLists.txt`
   + `src/` + `system.toml` with **no `Cargo.toml`**, so nothing enumerates it.
   The evidence is the absence: across all twelve there is no `metadata/` dir
   and no `.unprobeable` marker — they never entered the pipeline, as opposed to
   entering it and degrading.
2. **There is no SystemModel either.** A standalone leaf has no bringup, so the
   contract-sidecar road that replaced the cmake `ENTITIES` keyword in phase-412
   has nothing to resolve. `nros ws entity-facts --leaf <dir>` reading
   `system.toml` is the entire channel, which is exactly what issue 1142 built.

So 1265's fix direction — read the entities from the cross-compiled artifact
instead of from a host rebuild — would not help these leaves, because the
problem is not that the host build fails. Nothing asks.

## What it is worth

Do not retire this declaration casually. Issue 1142 measured, on
`examples/qemu-armv7a-nuttx/cpp/action-client` (arm-none-eabi 13.2.1,
`nros-minsizerel`):

| | `SERVICE_BUFFERS` | image `.bss` |
| --- | --- | --- |
| leaf declares nothing | 35,584 B | 508,208 B |
| leaf declares its one action client | **4,448 B** | 467,248 B |
| delta | **−31,136 B (−87.5 %)** | **−40,960 B** |

A retirement that silently restores the 8-slot fallback guess is a 31 KB
regression on a board with ~512 KB, and the fallback direction is the *safe*
one — the dangerous direction is a declaration that goes stale under-size, which
is what 1265 is about and is equally true here: nothing cross-checks these
twelve, because there is no probe output to check them against.

## A gap this measurement turned up

`find examples -name '*.json.unprobeable'` reports **65** unprobeable components
against **14** declarations. Most of the difference is legitimate — the
`workspaces/cpp` and `workspaces/features` packages have a bringup, so the
contract/model road covers them. But the six `examples/zephyr/rust/*` leaves are
unprobeable cross-only cargo leaves, 1265's exact shape, and declare nothing.

Whether that is a real under-size is NOT established here: CLAUDE.md records
that a standalone Zephyr leaf reaches no descriptor producer and is served by
the four kept `NROS_DECLARED_*` carriers instead. Someone should measure which
of the two it is before treating it as either a bug or a non-issue.

## Fix direction (not decided)

Give a standalone C/C++ leaf the same thing a workspace package has, so the
declaration has something to be checked against or derived from. Candidates,
none measured:

* extend the cmake probe project to a standalone leaf — it already knows how to
  configure a C/C++ component for the host; what is missing is enumeration of a
  non-cargo leaf, not the probe;
* let a standalone leaf resolve a degenerate one-node model from its own
  `system.toml` + a contract sidecar, so the phase-412 road applies and the
  declaration becomes derived rather than authored.

## Acceptance

A NuttX C or C++ standalone leaf with NO `entities` in its `system.toml` gets
the same `SERVICE_BUFFERS` and `.bss` as today's declaring leaf (4,448 B /
467,248 B on `cpp/action-client`, not the 35,584 B fallback), and adding a
service client to its `src/` changes them without touching a declaration.
