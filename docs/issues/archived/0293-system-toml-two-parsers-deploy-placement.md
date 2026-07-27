---
id: 293
title: "system.toml had TWO parsers with different schemas — `launch`-scoped deploy blocks and `<node machine=>` were silently ignored, making demo_bringup unresolvable"
status: resolved
type: bug
area: cli, codegen
related: [0285, rfc-0050, rfc-0059]
resolved_in: "issue-0293 (rlm launch-scope + machine placement; one deploy schema)"
---

## Symptom

Any bringup whose `system.toml` carries more than one `[deploy.*]` block could
not have its SystemModel regenerated:

```
$ touch src/demo_bringup/launch/system.launch.xml   # make the model stale
$ nros sync
Error: ws sync: nros-launch-resolve failed for `demo_bringup`
       (.../demo_bringup/launch/system.launch.xml):
Error: system config: node '/listener' is not placed — with multiple
       [deploy.*] blocks every node needs a `nodes = [..]` entry
```

`examples/workspaces/cpp/src/demo_bringup` is the in-tree case, and the config
it fails on is correct.

## Why it stayed hidden

`config/*_model.yaml` are committed, and `nros sync` only resolves when a model
is older than its inputs. So this bites exactly when someone **edits a launch
file** — the moment regeneration has to work. It reproduces on a clean tree.

It was also masked by issue 0285: while `nros sync` shelled out to a
`play_launch` found on PATH, a wrong-or-absent tool meant "use the committed
model", so the resolver never ran. Shipping `nros-launch-resolve` (0285) made
the resolver actually execute, which is how this surfaced. The 0285 fix did not
cause this — it stopped hiding it.

## Root cause: two parsers, one file

`system.toml` was parsed by two structs with **different schemas**:

| Parser | `launch` | `locator` | `nodes` |
| --- | --- | --- | --- |
| nano-ros `DeployTarget` (`cargo_metadata_schema.rs`) | **yes** | no | no |
| rlm `DeployBlock` (`model/src/system_config.rs`) | **no** | yes | yes |

Each declared fields the other lacked, for the same `[deploy.*]` table.
`DeployBlock` had no `deny_unknown_fields`, so serde **silently dropped**
`launch`. Placement then counted every block against every launch file:

```rust
let single = (self.deploy.len() == 1).then(|| ...);
```

`demo_bringup` has one unscoped block plus two scoped to
`multihost.launch.xml`. Resolving `system.launch.xml` saw `len() == 3`, took
the multi-block branch, and demanded `nodes = [..]` for nodes those scoped
siblings never governed.

A second, independent gap sat behind it: `<node machine="robot1">` **is** a
placement, and `model_builder` already records it as
`execution.deploy[fqn].host` before placement runs — but `apply_to` never
consulted it, so the multi-host launch was unresolvable even once scoping was
fixed.

## Fix

In `ros-launch-manifest` (`3199a83`, `317bf92`):

1. `DeployBlock.launch` is a declared field, so the key round-trips instead of
   being swallowed.
2. `DeployBlock::applies_to_launch` decides scope — unscoped blocks govern
   every launch file, scoped ones only their own, compared on file NAME (the
   key is relative to the bringup pkg; callers hold a full path).
3. `apply_to_launch(execution, fqns, launch_file)` filters to the governing
   blocks before the single/multi test. `apply_to` remains as a wrapper passing
   `None` — callers that cannot name the launch file behave exactly as before.
4. A node whose `host` names an in-scope block is placed by it, after an
   explicit `nodes = [..]` and before the error.
5. The `Deploy` insert no longer writes `host: None`. It replaces the whole
   entry, so blanking dropped the very placement that had just selected the
   block.

In `play_launch` (`0867f35`): `resolve.rs` passes the launch file through.
Without the caller supplying it, the rlm-side fix is inert.

The fail-loud rule is unchanged where placement is genuinely ambiguous: several
in-scope blocks, no `nodes = [..]`, no `machine=` still errors.

## Receipt

Before: `nros sync` failed on `system.launch.xml`. After: all six launch files
in `demo_bringup` resolve, `multihost` included, with `host: robot1` /
`host: robot2` preserved from `machine=`.

## Tests

`ros-launch-manifest-model`, 15 passing, two new:

- `launch_scoped_deploy_blocks_do_not_govern_other_launch_files` — the key
  round-trips; `system.launch.xml` places implicitly; bare name and full path
  agree; `multihost.launch.xml` is still an error; `apply_to`'s behaviour is
  unchanged.
- `node_machine_attribute_places_without_a_nodes_list` — both nodes place from
  `machine=`, their hosts survive the insert, an unknown host still fails loud.

## The SSoT question — CLOSED (2026-07-28, phase-312 W4.2)

The original fix removed the *symptom*. The structural cause is now gone too:
there is ONE definition of the deploy schema.

`ros_launch_manifest_model::system_config::DeployBlock` is the single schema.
It gained `Serialize`/`Clone`/`PartialEq`/`Eq` (nano-ros WRITES `system.toml`;
rlm only read it) and `skip_serializing_if` so absent fields stay absent.
nano-ros's `DeployTarget` is now a re-export of it, not a redeclaration — the
two struct literals that had to learn `nodes` were the whole cost.

`deny_unknown_fields` IS now on the shared struct, so the next divergence is a
loud parse error rather than a dropped key. The audit it was waiting on turned
out to be trivial: nano-ros's mirror already denied unknown fields, so every
`system.toml` in use already satisfied its field set, and unifying to the union
(which only adds `nodes`) can reject nothing that previously parsed.

Receipts: rlm 17 tests pass, including one asserting the round-trip, that
absent options are skipped on serialize, and that an unknown key fails with the
key named. nano-ros: 459 cli-core tests pass; `nros sync` resolves 6/6.
