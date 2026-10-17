# Phase 506 — the API crates carry only the API

**Status (2026-10-17). Opened to give six open issues a home; nothing landed
under this number yet.** Four of them are the remainder of RFC-0071, baselined
by `check-rmw-agnostic` when [phase-444](archived/phase-444-rmw-fix-up.md) was
archived. Two are the same idea applied to what the facade exports and how it
decodes a node.

## Why this phase exists

[RFC-0071](../design/0071-rmw-backend-descriptor.md) says the core-facing crates
(`packages/core/**`, `nros`, `nros-c`, `nros-cpp`) name no backend. A backend
is selected by the build and reached through the vtable, not by name. The gate
holds that line for new code. The baselined sites are still there, and each
one is a place where adding a backend means editing the API.

The facade has the same problem one level up. `nros::` publishes runtime
machinery beside the user API (issue 1789), and nros-c still decodes node slot
0 as "no node" (issue 1405), which nros-cpp fixed with a bias in issue 0312.

## Work items

### W1 — no backend names in core and the API crates (RFC-0071 D2, D8)

- [ ] [#1297](../issues/1297-rmw-agnostic-api-crates-name-backends.md):
      backend features, `register()` calls and `extern` declarations leave
      nros-c and nros-cpp.
- [ ] [#1298](../issues/1298-rmw-agnostic-core-names-backends.md): the two core
      crates that name backends stop doing so.
- [ ] [#1299](../issues/1299-rmw-agnostic-backend-named-config-in-api.md):
      backend-named configuration is keyed on the resolved backend.
- [ ] `check-rmw-agnostic`'s baseline reaches zero, and the gate fails on any
      new line.

### W2 — closed backend lists outside core and the API

- [ ] [#1300](../issues/1300-closed-backend-lists-outside-core-api.md): the CLI
      scaffold, colcon's `RMW_BACKENDS` and `board.knobs.zenoh` derive their
      list from the backends present, and a gate reaches them.

### W3 — the facade's surface

- [ ] [#1789](../issues/1789-nros-facade-exports-runtime-machinery.md): the 64
      ledger rows are settled as hidden, moved or kept with a reason.
- [x] [#1405](../issues/archived/1405-c-executor-node-id-zero-reads-as-no-node.md):
      nros-c's nine registration sites keep "no node" distinct from slot 0.
      **Done 2026-10-18:** measured as a real defect (a primary node on an
      extra session registered its executor entities on session 0); one
      decode helper, an `executor_bound` flag in `rclc_node_ref_t`'s padding,
      and `executor_primary_node_session.c` in `just check c`.

## Acceptance

`check-rmw-agnostic` passes with an empty baseline, and the `nros::` export
ledger has no row whose decision is not yet applied.

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#1297](../issues/1297-rmw-agnostic-api-crates-name-backends.md) | Per-backend Cargo features, `register()` calls and `extern` declarations in nros-c and nros-cpp are RFC-0071 D2's class. `check-rmw-agnostic` baselines them. |
| [#1298](../issues/1298-rmw-agnostic-core-names-backends.md) | `nros-macros` and one other core crate name backends in code, inside RFC-0071's reach. |
| [#1299](../issues/1299-rmw-agnostic-backend-named-config-in-api.md) | Backend-named configuration (`CONFIG_NROS_ZENOH_*`, `ZENOH_LOCATOR`) in the API crates is RFC-0071 D8's class. |
| [#1300](../issues/1300-closed-backend-lists-outside-core-api.md) | Closed backend lists outside core/api (the CLI scaffold, colcon's `RMW_BACKENDS`, `board.knobs.zenoh`) are beyond the gate's reach. |
| [#1405](../issues/archived/1405-c-executor-node-id-zero-reads-as-no-node.md) | `node_raw_id != 0` decodes the primary node slot as "no node" at nine nros-c registration sites, the same conflation issue 1384 fixed one layer up. |
| [#1789](../issues/1789-nros-facade-exports-runtime-machinery.md) | `nros::` still exports runtime machinery beside the user API: 64 ledger rows say a name should be hidden or moved. |
