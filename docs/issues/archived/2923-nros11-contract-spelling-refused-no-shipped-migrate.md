---
id: 2923
title: "A project scaffolded by nros11 stops at configure on its own contract's `rate_hz:`, and the refusal names `play_launch migrate`, which nano-ros does not ship"
status: resolved
type: bug
area: [cli, orchestration, release]
severity: high
found: 2026-11-24
resolved: 2026-11-24
resolved_in: "nros configure --migrate rewrites contracts and system.toml files with rlm's own migration; the refusal names it and the source file"
related: [phase-590]
---

## What failed

`nros new` in the nros-0.6.0-nros11 release wrote a talker contract whose timer
is `trigger: { timer: { rate_hz: 1 } }` (`new_workspace.rs`'s
`contract_node_rows` at that tag). rlm v0.3.27, which main pins since the
dependency bump to play_launch 0.36.0, refuses that spelling: a rate is a value
with a unit (`rate: 1Hz`). So every project that release scaffolded, in Rust, C
and C++, stopped at `nros configure` (and so at `nros build`):

```text
Error: sync: nros-launch-resolve failed for `rs_node_bringup` (.../build/nros/launch/rs_node_bringup/launch/system.launch.xml):
error: [manifest-parse] error: could not parse contract .../build/nros/launch/rs_node_bringup/launch/system.contract.yaml:14:
at 'nodes.rs_node.paths.on_tick.trigger.timer.rate_hz': Renamed `rate:` in v0.3.21 and refused since v0.3.27 ...
`play_launch migrate <contract> --write` rewrites it.
```

The remedy it named is not something a nano-ros user can run: `play_launch` is
not in the release asset. The file it named is the copy configure writes under
`build/`, which the next configure regenerates, so editing it changes nothing.

## The class

Every message nano-ros shows a user that names `play_launch migrate`:

- the resolver's refusal of an old contract spelling, printed by `nros
  configure` (`cmd/ws.rs`);
- the same rlm parse error from nano-ros's own contract readers
  (`orchestration/manifest.rs`, `cmd/entity_census.rs`);
- nano-ros's own refusal of a `system.toml`'s retired `[tiers.*]` /
  `group_tiers` (`nros-orchestration-ir`'s `retired_tables.rs`).

Each named a remedy nano-ros does not ship.

## Fix

- `nros configure --migrate` rewrites every `*.contract.yaml` and `system.toml`
  in the project after its existing layout moves, printing each key it changed
  (`file:line: key (rewrite)`). The rewrites are rlm's, called as a library:
  `ros_launch_manifest_types::migrate::migrate_str` for a contract and
  `ros_launch_manifest_model::system_config::migrate_system_toml_str` for a
  `system.toml`, the functions `play_launch migrate` calls. nano-ros only finds
  the files (skipping `build/`, `install/`, `target/` and the like), repeats a
  file's passes until nothing changes, and writes it
  (`nros-cli-core/src/rlm_migrate.rs`).
- When the resolver refuses an old spelling, configure's message leads with
  `To fix: run nros configure --migrate`, naming the source files that need it,
  and the checker's own `play_launch migrate ...` remedy is rewritten to
  `nros configure --migrate`. The in-process readers rewrite it too.
- The `[tiers.*]` / `group_tiers` refusals name `nros configure --migrate`.
- `nros doctor` warns about a contract or `system.toml` still in an old
  spelling.

rlm's own text (its "phase 104 S3" reference and its "may be newer than this
binary" line) is unchanged: that is rlm's to change.

## The project lock, on the installed track

The installed-path upgrade trial (an nros11 project built by the nros11 release,
then `nros pin` to this one, `nros setup`, `nros configure --migrate`,
`nros build`) got past configure and then stopped the Rust build:

```text
Error: the build finished, but it resolved off the pin set.
  source `zenoh-pico`: this project's nros-sdk.lock pins 19a47ae7..., the SDK now has e9675d97...
```

The project's `nros-sdk.lock` records the sources and tools the release that
last built it had, and a build never moves a pin the project recorded without
`nros update`. That verb exists for an installed SDK (`update_project`), but
`nros update --help` described only the checkout path, where it edits the pin
set.

- `nros configure --migrate`, with an installed SDK, also accepts this SDK's
  pin for each `[source.*]` and `[tool.*]` the project's lock pins differently,
  by calling `update_project`, so it is the same write `nros update --source
  <name>` makes. A `[cargo_override.*]` is the project's own decision and is not
  touched. In a checkout nothing is accepted: the pin set is the tree's.
- The drift message names `nros configure --migrate` beside `nros update`.
- `nros update --help` says what `--source`, `--tool` and `--crate` do with an
  installed SDK.

The C and C++ builds in the same trial stopped in the metadata probe, on a
header from the earlier release left in a build directory. That is a separate
defect with its own fix: the probe's cache is not keyed on the SDK it was
configured with.

## Verified

- `nros-cli/tests/nros11_scaffold_migrate.rs` scaffolds a talker in Rust, C and
  C++, writes the nros11 contract over it, and asserts that configure refuses it
  naming `nros configure --migrate` and the source contract, never `play_launch
  migrate`; that `--migrate` rewrites only the renamed key; and that the next
  configure succeeds. With the source changes reverted it fails, all three, at
  the remedy assertion.
- By hand on this host: a talker scaffolded in Rust, C and C++, with the
  nros11 contract written over it, runs `nros configure --migrate` (one line per
  rewritten key) and then `nros build` to a linux image, all three.
- `update::tests::accepting_the_sdk_pins_moves_only_what_disagrees`: a lock
  pinning an older zenoh-pico and a current kernel, plus a crate override;
  accepting moves only zenoh-pico, keeps its origin and the override, and a
  second run writes nothing.
- `rlm_migrate` unit tests: rlm refuses the nros11 text, the migration output
  parses and differs only in the rate key, a current contract is untouched, a
  `system.toml` takes the `system.toml` rewrites, and the walk rewrites the
  source and never the copy under `build/`.
