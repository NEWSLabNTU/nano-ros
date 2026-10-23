---
id: 2013
title: "`nros config explain` prints 8 of the 11 ladder tenants; `zenoh.wire`, `zenoh.limits` and `xrce` are invisible"
status: open
type: bug
area: [config, cli]
severity: low
found: 2026-10-23
related: [2006, 2007]
---

## Measured

RFC-0086 D6 makes provenance "the acceptance test": every knob the resolver
touches is printed with the rung that set it. On `main` (2026-10-23),
`nros config explain --platform freertos` prints these tenants: `zenoh.tx`,
`executor`, `runtime`, `log`, `net`, `rmw`, `params` and `memory`.

It prints nothing for three tenants that build scripts DO read through the
ladder:

| tenant | knobs | read by |
| --- | ---: | --- |
| `[knobs.zenoh.wire]` | 7 | `nros-zpico-build` (`runner.rs`) |
| `[knobs.zenoh.limits]` | 3 | `nros-rmw-zenoh/build.rs`, and `service_timeout_ms` also by `nros-build-helpers` (issue 2007) |
| `[knobs.xrce]` | 2 | `nros-rmw-xrce-cffi/build.rs` |

That is 12 of the 42 ladder knobs. `PlatformsTree::resolve_wire` exists with a
provenance-carrying return type and has no caller in `cmd/config.rs`. The
`zenoh.limits` and `xrce` tenants have rung accessors (`zenoh_limit_rungs`,
`xrce_rungs`) but no `resolve_*` that reports a source.

Found while closing issue 2007. Until then, `service_timeout_ms` had a
rung that neither reader passed, and `explain` could not have shown that,
because it never printed the tenant.

## Closes when

`explain` prints every tenant in `config-knob-census`'s `ladder_knobs()` list.
A gate should check that list against the tenants `explain` prints, so a new
tenant without a printed row fails.
