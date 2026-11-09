---
id: 2350
title: "`nros run --dry-run` says it would configure, then refuses the stale SystemModel"
status: resolved
type: bug
area: cli-run
related: [phase-544]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "fix-2350-dry-run-model branch: the dry run's implicit configure hands the model door a StaleModel policy"
---

**Home: [phase-544](../../roadmap/archived/phase-544-harness-run-plan-followups.md).** Found replaying Tutorial 9 (`tutorial/another-target.md`)
on main e55d53507b with the release asset of an earlier main.

## Symptom

After the chapter adds a second `[image.freertos]` to `system.toml`, its
step `nros run baremetal --dry-run --why` prints:

```text
nros build: --dry-run: would configure sensor_node_bringup:baremetal: inputs changed (recorded sha256:86ab…, now sha256:f5d5…)
nros build:   resolved → /root/sensor_ws/build/sensor_node_bringup__baremetal/resolved.toml (entity counts not derived; configured defaults kept)
nros run: `baremetal`: SystemModel `/root/sensor_ws/build/nros/models/sensor_node_bringup/system_model.yaml` is stale: input hash changed `system.toml`
  Run `nros configure` to re-resolve it.
```

and exits non-zero. The `--why` lines naming the image never print.

## Cause

Under `--dry-run`, the implicit build reports that it *would* configure, and
so writes nothing. Then the run's model gate (`packages/cli/nros-cli-core/src/model_gate.rs`, `Refusal::Stale`)
checks the model the dry run did not refresh, and refuses. A real `nros run`
would configure first and pass the gate.

## Acceptance

- Under `--dry-run`, a model that the implicit configure would refresh is
  reported as "would re-resolve", not refused.
- `--why` answers which image resolved without a configured model, since
  resolving the image name reads only `system.toml`.
- A CLI test edits `system.toml` after a configure, then asserts that
  `nros run <image> --dry-run --why` exits 0 and prints the `image =` lines.

## Resolution

The implicit configure (`configure::ensure_current`) now returns whether a
dry run found an image a real run would configure, and
`build::configure_implicitly` turns that into a
`model_gate::StaleModel` policy for the planning that follows. The one
decision is `StaleModel::admit`: under `WouldReResolve` (a dry run whose
configure would run) a `Refusal::Stale` model is read as it stands and
announced as

```text
nros run: --dry-run: would re-resolve <model>: <reason>
```

and under `Refuse` (every real build and run, and a dry run with nothing to
configure) it is refused as before. A `Refusal::Refused` model (the
producer's marker) is refused under both. The policy is a parameter, threaded
through `plan_builds_with` to every model door planning opens (the leaf road's
gate, `resolve_image`, both `generate_entry` calls); `nros run --dry-run` and
`nros build --dry-run` are the two front doors that configure implicitly, and
both pass it. `nros flash` and `nros monitor` run no implicit configure, so
they keep refusing a stale model, which is correct for them. The dry run still
writes no model (R43).

The would-configure line now names the verb that printed it (`nros run:`),
not `nros build:`.

Tests: `model_gate::tests::{only_a_dry_run_that_would_configure_admits_a_stale_model,
a_refused_model_is_refused_under_every_policy,
open_with_reads_a_stale_model_only_under_the_dry_run_policy}` and the CLI test
`packages/cli/nros-cli/tests/run_dry_run_stale_model.rs`, which configures a
copy of `examples/linux/rust/listener`, adds an image to `system.toml`, and
asserts `nros run linux --dry-run --why` exits 0, prints the `image =` line
with its rung, reports "would re-resolve" and leaves the model untouched.
