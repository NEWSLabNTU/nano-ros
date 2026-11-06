---
id: 2296
title: "Two test fixtures reuse a SystemModel from before a play_launch pin bump, and `codegen-system` refuses it as stale"
status: open
type: bug
area: [ci, testing, cli]
severity: medium
found: 2026-11-06
related: [phase-466]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**: a
scheduled-class lane stopped in its fixture build and produced no verdict.

## Measured

`run-matrix` run 42155998964 (workflow_dispatch, 2026-11-06 03:21 UTC, main
`a9e1941b13`) ran on the self-hosted runner. Both its jobs failed in the fixture
build, before reaching any cell:

- `tier 1 (cells)`, job 127706935841: `build-test-fixtures: FAILED stage(s): zephyr(rc=1) linux(rc=2)`.
- `tier 2 (1-wise matrix)`, job 127690172443: `build-test-fixtures: FAILED stage(s): zephyr(rc=1)`.

The stage logs, from the `tier1-junit` artifact
(`tmp/build-test-fixtures-20261106-041704-369374/`), name the same refusal in both
stages:

    zephyr.log:  Error: codegen-system: SystemModel
      `…/packages/testing/nros-tests/fixtures/zephyr_self_pkg/self/alpha_pkg/build/nros/models/alpha_pkg/system_model.yaml`
      is stale: resolver pin changed (model `b5243446b761` != ours `8d2c78d40671`)
      Run `nros configure` to re-resolve it.

    linux.log:   Error: codegen-system: SystemModel
      `…/packages/testing/nros-tests/fixtures/image_config_ws/build/nros/models/probe_bringup/system_model.yaml`
      is stale: resolver pin changed (model `b5243446b761` != ours `8d2c78d40671`)

The refusal comes from `model_gate.rs` (the `NROS_PLAY_LAUNCH_SHA` comparison). The
two pins are both recent `packages/cli/third-party/play_launch` gitlinks on main:

| commit | date (UTC) | play_launch pin |
| --- | --- | --- |
| `548ee74edd` (phase 536 W1, rlm v0.3.10) | 2026-11-05 04:13 | `b5243446b761`, the pin the stale models record |
| `d1e5cca620` (phase 536 W3, rlm v0.3.11) | 2026-11-05 08:02 | `dd660b2d1701` |
| `2a968b951b` (rlm v0.3.12, play_launch 0.29.0) | 2026-11-05 09:44 | `8d2c78d40671`, the pin the CLI was built with |

So both models were resolved by a run between 04:13 and 08:02 on 2026-11-05 (the
scheduled run-matrix 41979554190 at 06:18 fits). They stayed in the runner's
persistent `nros-fixture-tree` through two pin bumps. This run's fixture build then
fed them to `codegen-system` without re-resolving them.

## What it is not

- Not a code defect in either fixture. The gate is right to refuse a model from
  another resolver pin; issue 0409's shape is a model that silently misses data.
- Not issue 1360. That one is the codegen-version `#error` from a persistent store
  workspace. This one is the resolver pin, refused correctly, on a path that should
  have re-resolved first.
- Not issue 1488 (archived). That one was the zephyr self-pkg sync refusing its own
  layout. Here sync is never asked.

## What would close it

The fixture build's path to `codegen-system` for these two fixtures (the west
`nros_system_generate.cmake` road for `zephyr_self_pkg`, and the cmake workspace
road for `image_config_ws`) re-resolves a model whose recorded resolver pin differs
from the CLI's, as `nros configure` would. It should not consume a model left by an
earlier checkout. A pin bump must be a freshness input on those roads, the same way
the codegen version is for issue 1360.

Acceptance: a run-matrix tier 1 on the self-hosted runner, after a play_launch pin
bump, gets past both fixtures with no hand cleanup of `nros-fixture-tree`.
