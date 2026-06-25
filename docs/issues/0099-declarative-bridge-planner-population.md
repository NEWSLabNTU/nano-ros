---
id: 99
title: "Declarative `[[bridge]]` in system.toml does not forward — the planner emits neither `build.transports` nor `plan.bridges`, so the (code-complete) bridge codegen never fires"
status: open
type: bug
area: cli
related: [phase-263, rfc-0009]
---

## Summary

A cross-RMW bridge declared with `[[bridge]]` in `system.toml` **links the union
of bridged RMW backends but forwards nothing**. The entire downstream bridge
pipeline — the `[[bridge]]` schema, the `PlanBridge`/`PlanTransport` IR, the
`validate_bridges` + `render_register_bridges_fn` codegen, the `nros-bridge`
echo codec, and the multi-session `Executor::open_multi` runtime — is
code-complete and unit-tested (RFC-0009, "Stable / DONE"). The one missing link
is in the **planner**: it never translates `system.toml`'s `[[bridge]]` /
`[[domain]]` into the plan fields the codegen consumes.

## Root cause

`schema_build` (`planner.rs:887`) does the only bridge-aware thing today: when
`system.bridges` is non-empty it inserts `bridged_rmws` (the RMW **name** union)
into `plan.build` so board-feature lowering pulls in the extra backend. It does
**not** emit either of the two fields the bridge codegen keys off:

1. **`plan.build.transports`** — `PlanBuildOptions::is_bridge()`
   (`plan.rs:802`) is `self.transports.len() > 1`, and `SESSION_SPECS`
   (`generate.rs:2019`, consumed by `build_executor_bridge` →
   `Executor::open_multi`) is built one `SessionSpec::new(rmw, locator)
   .domain_id(domain)` per `plan.build.transports` entry. The planner emits an
   **empty** transports array for a bridged system → `is_bridge()` is `false`,
   `SESSION_SPECS` has one entry → no multi-session executor.
2. **`plan.bridges`** — `render_register_bridges_fn` (`generate.rs:2510`) and
   `register_all` (`generate.rs:2409`) both gate on `!plan.bridges.is_empty()`.
   The planner builds the plan with `bridges: Vec::new()` and never populates it
   → no relay nodes, no forwarding.

So `validate_bridges` / `render_register_bridges_fn` are dead code on the
system.toml path: ready, but never handed a non-empty `plan.bridges`.

(RFC-0009 predates issue #51's "no root `nros.toml`" move — its `apply_bridges`
deploy path read the now-removed root `nros.toml`. The schema migrated to
`system.toml [[bridge]]` + `bridged_rmws`, but the planner's
system.toml→plan transform for transports/bridges was never written.)

## Fix (the bounded transform)

In the planner (around `schema_plan_json` / `schema_build`, which already parse
`system_caps: SystemToml` and have the resolved `interfaces` in scope), when
`system.bridges` is non-empty emit a **consistent** transports + bridges pair:

- **`build.transports`** — one `PlanTransport { kind, rmw: Some(rmw),
  domain: Some(id), locator }` per **distinct** bridge endpoint. Endpoint string
  `"<rmw>:<domain>"` → `rmw` = prefix, `domain` = the `[[domain]]` named after
  `:` resolved to its `id` (`SystemDomainEntry { name, rmw, id }`); a bare
  endpoint is a `[[domain]]` name resolved to its `rmw`. `locator` = the
  `[system].locator` for the endpoint whose `rmw` == `[system].rmw`, else `None`
  (DDS/multicast endpoints discover by domain). Mirrors `bridged_rmws()`'s
  endpoint parsing (`cargo_metadata_schema.rs:652`).
- **`plan.bridges`** — one `PlanBridge { name, connect: [from_ep, to_ep],
  topics }` per `[[bridge]]`, each `PlanBridgeEndpoint { rmw, domain, locator }`
  byte-matching its transport (so `bridge_endpoint_session_idx`
  (`generate.rs:2447`) resolves the slot). `topics` = every declared interface
  topic (RFC-0009 "resolve from interfaces"; wildcard deferred).

`SystemBridgeEntry` has **no `topics` field** today → forward-all-declared is the
v1 behaviour; an explicit `topics = [...]` for selective forwarding is a later
refinement.

Add planner unit tests: a `[[bridge]]` + two `[[domain]]`s → `is_bridge()` true,
`plan.bridges` populated, endpoints byte-match transports, topics resolved from
interfaces.

## Impact / consumers

- **phase-263 B3** (`ws-bridge-rust`) is blocked on this — a declarative bridge
  workspace cannot forward until the planner populates these fields.
- Reference (imperative, working): `examples/bridges/tt-zenoh-to-{xrce,cyclonedds}`.
- Related runtime gotchas already resolved: #53 (egress extra-session defaults to
  domain 0 — thread `.domain_id()`), #67 (typed-cyclone marker; multi-RMW uses
  the raw + `register_type_descriptor` path).
