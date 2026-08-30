---
id: 938
title: "Two verbs resolve RMW from different tables: `nros build` reads
  `[image.*]`, `nros plan` / `codegen-system` read `[deploy.<t>].rmw`"
status: open
type: bug
area: cli
related: [rfc-0031, phase-383, phase-255]
---

## Problem

RMW has two live resolution paths that read different tables, so the same
workspace can answer "which RMW?" two ways depending on the verb:

| verb | resolver | reads |
| --- | --- | --- |
| `nros build` | `facade::image_rmw` | `[image.<x>].rmw` over `[image_defaults].rmw` (RFC-0065) |
| `nros plan`, `nros codegen-system` | `SystemToml::resolved_rmw` | `--rmw` > `[deploy.<t>].rmw` > `[system].rmw` > `zenoh` (RFC-0031) |

Both are user-facing. `nros plan --target`'s own help says it selects "the
`[deploy.<t>]` the planner resolves per-target values against (RMW override,
build tuning, domain/locator)", and `codegen_system` bakes the result into
`#define NROS_SYSTEM_RMW`, which every embedded RTOS adapter consumes.

So a workspace declaring `[image.x].rmw = "cyclonedds"` and
`[deploy.x].rmw = "zenoh"` builds with cyclonedds and BAKES zenoh into its C
config. Nothing warns.

`resolved_rmw`'s own doc comment names the hazard it now has —
"so a given target gets exactly one RMW — no duality".

## Why this is invisible today

The only two `[deploy.*].rmw` in the tree set the value `[system].rmw` already
yields:

    multi_pkg_workspace_freertos  [system] rmw = "zenoh"  [deploy.qemu-mps2-an385] rmw = "zenoh"
    multi_pkg_workspace_nuttx     [system] rmw = "zenoh"  [deploy.qemu-arm-nuttx]  rmw = "zenoh"

and neither fixture declares a conflicting `[image.*].rmw`. The duality is
reachable, not exercised.

## How the RFCs got here

RFC-0031 (Stable, 2026-06) says "RMW is a property of the deploy target /
binary" and makes `[deploy.<target>].rmw` precedence rung 2 — correct when a
deploy target WAS the buildable unit. RFC-0065 (2026-08) split placement from
the buildable unit, named the latter `[image.*]`, and moved build fields there.
`nros build` followed; `plan` and `codegen-system` did not, and RFC-0031 was
never amended.

## Decision taken (2026-08-31): image owns RMW

`[deploy.*]` keeps PLACEMENT; `[image.*]` owns what gets built. Chosen because a
deploy cannot need an RMW its image cannot express — a deploy with no image
builds nothing, and one with an image inherits it — and because it reduces the
five places `rmw` can be written (`[system]`, `[image_defaults]`, `[image.<x>]`,
`[[domain]]`, `[deploy.<x>]`) by one, making the rule sayable: deploy = where,
image = what.

**This is bigger than deleting a rung.** `plan` and `codegen-system` resolve
per-TARGET and have no image in hand, so migrating them means teaching them the
image table — or deciding those verbs are superseded by `nros build` and saying
so. Until one of those happens, `[deploy.<t>].rmw` must keep working on those
paths, so W10.b cannot delete the field at 0.6.0 without addressing them.

## Correction history, because this issue was wrong twice

1. First filed as "the rung never fires", from a grep that found only
   `bridged_rmws()` (which passes `target = None`) and unit tests.
2. That was refuted by the compiler the moment the parameter was removed:
   `codegen_system.rs:677` and `planner.rs:735` both pass a real target.

Grep found the callers it was pointed at. The lesson is the one this repo keeps
relearning — a claim about whether code is reachable has to be made with a tool
that knows the call graph, not a text search.
