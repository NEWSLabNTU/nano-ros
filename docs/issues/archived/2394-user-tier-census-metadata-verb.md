---
id: 2394
title: "User-tier text still tells a user to run a dev verb where no user-tier verb exists yet: the entity census, source metadata, the bringup move and the central patch"
status: resolved
type: limitation
severity: medium
area: [cli, ux]
related: [phase-542, phase-526]
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "nros configure --census [BRINGUP:IMAGE] and --migrate; nros new names nros build; the PX4 message names a workspace or the module template"
---

**Home: [phase-542](../../roadmap/phase-542-px4-supported-platform.md)** (W3 found it).
A follow-up of [RFC-0117](../../design/0117-cli-command-tiers.md) D2/D3.

## What

RFC-0117 D3 puts every build-tool verb under the hidden `nros dev` tier, and
phase-542 W3 added `check-user-text-dev-verbs`, which refuses user-facing text
that TELLS a user to run one. The sites below could not be reworded, because
the action they asked for had no user-tier verb. Each carried a
`dev-verb-advice-ok` marker naming this issue, which the gate accepts only
while the issue is open:

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
issue. Done when no site carries this issue's marker.

Homes decided 2026-11-10 (user ruling), recorded as the RFC-0117 amendment
"user-tier verbs for 2394": `nros configure --census [B:I]`, source metadata
recorded by `nros build` (under `--census` if it needs the native build),
`nros configure --migrate` for the bringup move with `nros doctor` staying
read-only, and no new verb for the central patch.

## Resolution (2026-11-10)

Each site now names its user-tier home, and no site carries the marker.

| Site | Now says |
| --- | --- |
| `nros new` component, "Next steps" | `nros build   # builds it and records its source metadata`. Configure's first phase, which `nros build` runs, already refreshes every missing or stale sidecar (`metadata_refresh::refresh_stale_sidecars`), and that needs no native image, so answer 1 went to `nros build`, not `--census`. |
| `nros configure`, stale census line | `nros configure --census <bringup>:<cross image>` (or the bare flag for an entry's own census), and says it builds and runs the native image. |
| `census_remedy`, the cmake configure's refusal | "in the workspace, run `nros configure --census B:I`, then build again". |
| census verdict rows (unobserved node or parameter) | "run `nros configure --census` with an image that launches `<node>`". |
| `nros configure`, `system.toml` beside code | "`nros configure --migrate` moves it and says what it moved". `nros doctor` gives the same finding, in a project only, and moves nothing. |
| PX4 bridge, FFI archive missing | build the module from an nros workspace (`nros configure` writes the patch table), or start from the module template under `nros locate nano-ros`. No dev verb. |

The CLI:

- `nros configure --census [BRINGUP:IMAGE]` takes the census of every image
  the configure covers, or of the one named, before configuring. It uses the
  `take` path `nros build` already runs, and it fails when a take cannot run,
  because the user asked for one.
- `nros configure --migrate` moves each `system.toml` found beside package
  code (the project and every discovered package) into a bringup package,
  through the existing migration, which prints every file it moves.
- Neither flag combines with `--check` or `--dry-run`.

Book: `guide/contracts/fix-a-contract-refusal.md` names `--census` beside
`nros build`, `guide/build/organize-a-larger-workspace.md` names `--migrate`,
and `reference/cli.md` is regenerated.

Tests: `census_and_migrate_flags_parse_and_refuse_a_writeless_configure`
(configure_verb), plus the updated wording assertions in `entity_census` and
`ws`.
