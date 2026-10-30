---
id: 2153
title: "A plan's `[build.cc]` override parses and changes no build"
status: resolved
resolved: 2026-10-30
type: bug
area: [cli, orchestration]
severity: low
found: 2026-10-30
related: [0202, phase-515]
---

## What

`PlanBuildOptions::cc` (`packages/cli/nros-cli-core/src/orchestration/plan.rs`)
deserialises a per-layer `[build.cc]` block — `debug`, `opt_level`, `cflags` —
and its doc comment promised it reached every `cc::Build`: `debug`/`cflags` as
`CFLAGS`/`CXXFLAGS`, `opt_level` as `NROS_CC_OPT` (phase-204.15 / 204.9).

Nothing reads it. The generator that exported those variables was retired with
the standalone-package pipeline (#202, `11a00b0f8`), and no build script reads
`NROS_CC_OPT` any more. Measured 2026-10-30:

```text
git grep -nE 'PlanCcOverrides|NROS_CC_OPT' -- packages scripts cmake just
```

finds only the struct, its field and its own unit test. A user who writes
`[build.cc] opt_level = "s"` gets a plan that carries it and an image built
exactly as without it, with no diagnostic.

Found by the phase-515 W0.d comment-name rewrite pass: the doc comment named
`NROS_CC_OPT`, which the gate struck as gone.

## Options

- Delete the block (and refuse the key with a hint), since no road applies it.
- Or wire it to a road that can carry it, per `canonical-build-path.md`'s
  "which roads carry it" question — cargo `[env]`, cmake, west each need their
  own carrier.

Either way, a key that parses must not be a silent no-op.

## Resolution

Refused, not wired. Wiring needed an AUTHORED home first, and there is none:
`system.toml` has no `[build]` table (`deny_unknown_fields` rejects one), the
planner (`schema_build_json`) never writes `cc`, and the only way a plan could
carry the block was a hand-edited `nros-plan.json`. Giving it a home plus a
carrier on each of the three roads (cargo `[env]`, cmake, west) would be new
design, not a repair, and nothing asks for it.

The same audit found the block's two siblings in the identical state — the
plan `build` keys `optimize` and `cargo` (`[build.cargo]`), both phase-204.15,
both with no producer and no consumer — so the class is fixed, not the site:

- `PlanBuildOptions` (`packages/cli/nros-cli-core/src/orchestration/plan.rs`)
  drops `optimize: Option<String>`, `cargo: Option<PlanCargoOverrides>` and
  `cc: Option<PlanCcOverrides>` (both override structs deleted). Each key keeps
  a `RetiredPlanKey` slot whose `deserialize_with` refuses it BY NAME with
  `RETIRED_BUILD_TUNING_REMEDY` — which names the build tool's own knobs
  (`[image.<id>] profile` + the leaf's `[profile.*]`, `CFLAGS`/`CXXFLAGS`,
  `-DCMAKE_C_FLAGS`) — instead of serde's generic "unknown field". The slots
  never serialize, so planner output is byte-identical.
- Tests: `retired_build_tuning_keys_are_refused_by_name` (all three keys refuse,
  naming this issue, not as an unknown field) and
  `retired_build_tuning_keys_never_serialize`, replacing
  `build_cc_override_parses`, which asserted the no-op.

Nothing left open. If per-layer compiler tuning is wanted later, it needs an
RFC deciding its authored home (likely the nano-ros `nros.toml` overlay, per
the RFC-0060 amendment of 2026-10-10) and a carrier on each road.
