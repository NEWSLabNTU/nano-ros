---
id: 1300
title: "Closed backend lists OUTSIDE core/api have no gate — `check-rmw-agnostic` reaches exactly RFC-0071's two bullets, and the CLI scaffold, colcon's `RMW_BACKENDS` and `board.knobs.zenoh` sit beyond them"
status: resolved
type: tech-debt
area: ci, cli, rmw
severity: low
found: 2026-09-11
resolved: 2026-10-19
related: [1219, 0934, RFC-0071, phase-444, phase-506]
---

**Home: [phase-506](../../roadmap/archived/phase-506-api-crates-carry-only-the-api.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What

Issue 1219 counted five closed backend lists; phase-439 W4 removed three and
phase-444 W4.b wrote `check-rmw-agnostic`, whose REACH is exactly RFC-0071 §
Verification: `packages/core/**`, `packages/api/nros{,-c,-cpp}/**` and
`cmake/NanoRosRmwDispatch.cmake`. That is the rule the RFC states, and the gate
enforces all of it. The lists 1219 found beyond it are a DIFFERENT rule — "no
tool enumerates the backends" — and are recorded here rather than dropped when
1219 closed:

| site | shape | `uorb`? |
| --- | --- | --- |
| `packages/cli/cargo-nano-ros/src/workspace_scaffold.rs` | `"cyclonedds" \| "zenoh" \| "xrce" => {}` else `bail!` | rejected |
| `packages/cli/colcon-cargo-ros2/colcon_nano_ros/task/nros/build.py` | `RMW_BACKENDS = ("zenoh", "xrce", "cyclonedds")` | absent |
| `packages/cli/nros-cli-core/src/cmd/config.rs` | `board.knobs.zenoh.tx`, `"zenoh.tx.batch"` (RFC-0071 D8) | n/a |
| `packages/cli/nros-cli-core/src/orchestration/bridge_gen.rs` | the first spelling of `rmw_crate_ident`'s table | absent |

## Why this is not simply "widen the gate"

1219's first attempt measured it: over the build-decision surface, comments
stripped and names matched only as quoted literals or alternations, **39 files**,
roughly 40 % of them test-plan coordinates, bridge demos that name two backends
because bridging them is the point, and help text. A baseline of 39 unverified
reasons is a gate that reads as coverage. It needs the classification pass
(dispatch vs test plan vs demo vs help text) first.

## Fix direction

Either resolve each list through the provider scan (`rmw_resolver::known_rmw_in`)
the way phase-439 W4 did for CMake, or classify the surface and extend
`check-rmw-agnostic`'s reach with a closed-list rule (two or more names in one
construct) once the noise is sorted.

## 2026-10-18 — three of the four lists are derived now (phase-506 W2)

RFC-0071 D11 settled the direction: tools list backends through the provider
scan, never a literal.

| site | now |
| --- | --- |
| `workspace_scaffold.rs` | `rmw_resolver::resolve_rmw()`, the descriptor-derived table `nros new` (single package) already used. `uorb` is accepted; a test walks `known_rmw()` and checks an unknown name is refused before any write |
| colcon `RMW_BACKENDS` | gone; `known_rmw_backends()` asks `nros ws rmw-dispatch --known` |
| `bridge_gen.rs` | the descriptor's `register` field (D11); Cyclone's host-only rule is now its CMake link strategy, not its name |
| `config.rs` `board.knobs.zenoh` | open — moves with the knob roles (D10, phase-506 W3) |

## Resolution (2026-10-19) — the fourth list, `nros config explain`

`config.rs` printed `zenoh.tx.batch` / `split_lock` / `flush_ms` for every
platform, read from `board.knobs.zenoh.tx`. The `tx` tenant was already
per-backend in the schema (`[knobs.<rmw>.tx]`, phase-400 W2) but not in the
resolver, so:

* `PlatformsTree::resolve_tx` takes the backend. Its platform rung reads
  `Knobs::tx_for(backend)` per file, and its env rung is the knob registry's
  `<backend>.tx.*` rows (none for a backend the registry does not know).
* `Knobs::tx_for` handed the legacy `[knobs.zenoh.tx]` field to EVERY backend
  without a table of its own, so asking for `xrce`'s tx knobs answered with
  zenoh's. The fallback is now the legacy field's own key,
  `LEGACY_TX_BACKEND`, and nothing else. Two tests pin it, one with a
  third-party backend's table.
* `nros config explain` walks `rmw_resolver::known_rmw()` and prints
  `<rmw>.tx.*` for each backend the registry or the platform/board files give
  that tenant. The in-tree output is byte-identical (zenoh is the only such
  backend); `config.rs` names none.

With this, all four lists the issue counted are derived. A closed-list GATE
over the wider build-decision surface was not added: its own section above
measured that it needs a classification pass first (39 files, roughly 40 %
test plans, bridge demos and help text), and RFC-0071 D11 settled the
direction as derivation rather than a gate.
