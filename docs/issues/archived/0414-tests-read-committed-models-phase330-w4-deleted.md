---
id: 414
title: phase-330 W4 made the SystemModel a build artifact but left four test files reading its committed path
status: resolved  # fixed 2026-08-04
type: bug
area: testing
related: [phase-330, rfc-0063, 0380, 0409]
---

## Problem

`39d007dfc` ("the SystemModel is a pure build artifact") deleted every committed
`config/*model.yaml`. Four test files still read those paths and now fail on a
missing file rather than on anything they were written to assert:

```
qos_override_e2e the_committed_model_declares_a_reliability_override_that_lowers
  committed model missing at …/examples/workspaces/features/src/demo_bringup/config/rust_qos_model.yaml:
  No such file or directory (os error 2)

multihost_partition_bake committed_per_host_models_carry_their_binding
  read …/examples/workspaces/rust/src/demo_bringup/config/multihost_robot1_model.yaml:
  No such file or directory (os error 2)

multihost_partition_bake multihost_bake_emits_only_the_hosts_node
zephyr_edf_deadline_applied zephyr_edf_deadline_applied_for_real_time_tier
native_main_macro_misuse rebuilds_on_model_touch
```

Measured directly, one file at a time, on `aa3f70acb`:

| test file | result |
| --- | --- |
| `multihost_partition_bake` | 1 passed, **2 failed** |
| `native_main_macro_misuse` | 3 passed, **1 failed** |
| `zephyr_edf_deadline_applied` | 2 passed, **1 failed** |
| `qos_override_e2e` | 1 passed, **1 failed** |
| `launch_synth` | 1 passed |
| `workspace_dirwalk_discovery` | 2 passed |

The last two grep as consumers but do not read a deleted path.

## Why it matters beyond the count

These tests assert things the flip did not change — that a per-host bake carries
its binding, that a real-time tier gets an EDF deadline, that a QoS override
lowers. The declarations still exist in `system.toml`; only the artifact's
LOCATION moved. So the tests are right and their input path is stale, which is
the cheap half of the fix.

The expensive half is that `check-no-tracked-models.sh` (added by the same
commit) enforces the new rule, and nothing points the consumers at the new
location. A test that reads a build artifact needs to know the active build's
output dir; there is no helper for that today, which is presumably why the flip
left them.

## Fix direction

Add one helper in `nros-tests` that resolves a bringup's model from the build
output dir the way the entry does, and repoint the four files at it. Failing
that, the tests should skip with a message naming the flip, rather than panic on
`os error 2` — a missing build artifact is an unmet precondition, not a failed
assertion (CLAUDE.md: tests must fail loud on unmet preconditions, and these
currently fail loud about the WRONG thing).

## Found by

Triaging `just ci-matrix` (26 failed / 2 timed out). Most of that count was
stale fixtures — `workspace-fixtures-build.sh` could not run at all on main
(fixed in `aa3f70acb`) — plus three orphaned `zenohd` processes, one 6 days old.
With fixtures rebuilt and the routers killed, the ROS 2 interop surface is clean
(`interop_e2e` 10/10, `param_live_read_e2e`, `cpp_c_param_live_read_e2e` all
pass), and this issue plus `params::test_ros2_param_set_reconfigures_live_read`
are what remain.

## Resolution (2026-08-04)

The rule applied: **a test never reads a committed model.** Models are
intermediate artifacts now — transparent to users, readable by anyone who wants
them, and not an input the build or its tests may depend on. Where a test needs
one it RESOLVES it, the way a build does; where it only needs the declaration it
reads the input that declares it.

Five consumers, not the four this issue first named — `zephyr_edf_deadline_applied`
was deleted by phase-329 W2 between filing and fixing, and
`entry_typed_plan` in the CLI sub-workspace turned up once the others were done:

| test | before | after |
| --- | --- | --- |
| `qos_override_e2e::the_committed_model_declares_…` | read `rust_qos_model.yaml` | reads `system.toml`, finds the component BY THE OVERRIDE KEY (renamed in W2b, issue 0398), asserts the lowering — renamed `the_bringup_declares_…` |
| `multihost_partition_bake::committed_per_host_models_…` | read 8 committed per-host models | resolves each of the 4 workspaces × 2 hosts into a temp dir and asserts the partition — renamed `per_host_resolves_partition_and_carry_their_binding` |
| `multihost_partition_bake::multihost_bake_emits_only_the_hosts_node` | baked from committed models | resolves both host models first, then bakes from those |
| `native_main_macro_misuse::rebuilds_on_model_touch` | touched a committed model | resolves into a build-output dir, points the macro at it with `NROS_MODEL_DIR`, touches THAT |
| `entry_typed_plan::typed_plan_from_template_…` | asserted the committed template model's absence "is a repo defect" | resolves the template bringup into a temp dir |

That last assertion is worth naming: it said a missing committed model was a
repo defect. phase-330 W4 inverted the rule — committing one is now the defect,
enforced by `check-no-tracked-models.sh` — so the test failed on a condition the
repo had deliberately created.

`launch_resolver_bin()` moved into `nros_tests` (it had been private to
`multihost_partition_bake`) so the two suites that now need the resolver share
one answer to "where is it".

  -E 'binary(native_main_macro_misuse) or binary(multihost_partition_bake) or binary(qos_override_e2e)'
    -> 10 passed
  nros-cli-core -E 'binary(entry_typed_plan)' -> 1 passed

## Still open, and bigger than this issue

**`nros::main!` consumes the model, not the inputs.** The macro resolves
`config/system_model.yaml` (build-output copy first, via
`model_location::resolve_model_path`) and TRACKS that file. It never sees
`system.toml` or the launch XML, so touching either does not force a rebuild,
and a leaf checked with plain `cargo check` and no build step fails with
"SystemModel not found" — which is what `rebuilds_on_model_touch` hit.

The stated direction is that the build system should ask for launch + config and
resolve the model itself. Until it does, `rebuilds_on_model_touch` touches the
artifact rather than the input: asserting the input-touch contract today would
be asserting a wish, and the test says so where it touches.
