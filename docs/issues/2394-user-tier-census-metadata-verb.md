---
id: 2394
title: "User-tier text still tells a user to run a dev verb where no user-tier verb exists yet: the entity census, source metadata, the bringup move and the central patch"
status: open
type: limitation
severity: medium
area: [cli, ux]
related: [phase-542, phase-526]
found: 2026-11-10
---

**Home: [phase-542](../roadmap/phase-542-px4-supported-platform.md)** (W3 found it).
A follow-up of [RFC-0117](../design/0117-cli-command-tiers.md) D2/D3.

## What

RFC-0117 D3 puts every build-tool verb under the hidden `nros dev` tier, and
phase-542 W3 added `check-user-text-dev-verbs`, which refuses user-facing text
that TELLS a user to run one. The sites below cannot be reworded, because the
action they ask for has no user-tier verb. Each carries
`dev-verb-advice-ok: issue 2394`, which the gate accepts only while this
issue is open:

| Site | Who reads it | The dev verb it names |
| --- | --- | --- |
| `packages/cli/cargo-nano-ros/src/scaffold.rs` (`nros new` component, "Next steps") | `nros new` | `nros dev metadata --build` |
| `packages/cli/nros-cli-core/src/cmd/ws.rs` (stale-census line) | `nros configure` | `nros dev image entity-census` |
| `packages/cli/nros-cli-core/src/cmd/entity_census.rs` (`census_remedy`) | the census freshness refusal a cmake configure prints | `nros dev image entity-census take` |
| `packages/cli/nros-cli-core/src/entity_census.rs` (two `remedy` strings) | census verdict rows (an unobserved node or parameter) | `nros dev image entity-census run` |
| `packages/cli/nros-cli-core/src/cmd/ws.rs` (system.toml beside code warning) | `nros configure` | `nros dev migrate-bringup` |
| `examples/px4/cpp/bridge/src/modules/nros_uorb_bridge/CMakeLists.txt` (missing FFI archive) | the PX4 bridge's configure | `nros dev lock central-patch` |

Phase-530 W6 (#3405) landed its census step with a FATAL_ERROR in
`cmake/NanoRosCensusStep.cmake` that names `nros dev image entity-census path`
only as the call that could not answer: a diagnosis, so it needs no marker.

## Why

Each of these is advice a user acts on: take a fresh census, record source
metadata, move a `system.toml`, write the central patch file. A dev verb is
hidden from `nros --help`, has no user page and keeps no promise to users
(RFC-0117 D1), so a user told to run one is sent outside the tier the book
teaches (RFC-0124 D8).

## Fix

Give each action a user-tier home (an RFC-0117 D2 amendment), or fold it into
a verb the user already runs: `nros configure` or `nros build` could take the
census and record metadata themselves, as they already write the central
patch for a workspace. Then reword each site, drop its marker, and close this
issue. Done when `git grep 'dev-verb-advice-ok: issue 2394'` is empty.
