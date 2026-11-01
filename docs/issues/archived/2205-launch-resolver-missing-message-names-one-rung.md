---
id: 2205
title: "`nros sync`'s missing-resolver refusal names one of four rungs and not the override"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [cli, build]
severity: low
found: 2026-11-01
related: [0285, 0409, 1850, 1641]
---

## What was measured

A Zephyr 4.4 spike for an out-of-tree board ran a CLI built from `main` with
`cargo build --target-dir <ws>/cli-target`, outside the checkout's own
`packages/cli/target`. That checkout was a shared worktree with no
`play_launch` submodule, and the spike was not allowed to change it. Every
Rust Zephyr leaf runs `nros sync`, and `nros sync` stopped with:

```
Error: sync: 1 SystemModel(s) need resolving but `nros-launch-resolve` is not next to the `nros` binary:
  zephyr_talker — build/nros/models/talker-rs/system_model.yaml is missing (system.launch.xml)

Build it:  ./scripts/bootstrap.sh   (contributors: just setup-launch-resolve)
(If the submodule is missing:  git submodule update --init packages/cli/third-party/play_launch)
```

The spike then built `nros-launch-resolve` in a copied subtree and placed it
beside `nros`. That copy cannot build on its own: the crate's `build.rs`
reaches `../build-support` and `../../tooling/nros-build-paths`, and its
sources are the `play_launch` submodule.

## The provisioning roads (no defect here)

The resolver is provisioned by the same road as the CLI. `nros setup` does
not provision it, and does not need to:

| Track | Road | Where it lands (ladder rung) |
|---|---|---|
| checkout | `./scripts/bootstrap.sh`, or `just setup-worktree` + `just setup-cli` + `just setup-launch-resolve` (it needs the `play_launch` submodule) | `packages/cli/nros-launch-resolve/target/release/` — "this checkout's build", which a CLI built from that checkout finds whatever its `--target-dir` |
| installed | the release asset (`release-nros.yml` ships `bin/nros-launch-resolve`), fronted by `scripts/install.sh` | `$NROS_HOME/bin` — "beside this nros" / "the store" |
| anything else | `$NROS_LAUNCH_RESOLVE=<path>` | rung 1 |

`nros locate tool nros-launch-resolve --why` prints which rung answered.

## The defect: the refusal

`nros sync` said the resolver "is not next to the `nros` binary". That is one
of the four rungs `model_location::launch_resolver_from` tries. The message
did not mention `$NROS_LAUNCH_RESOLVE`, the override for a binary built
anywhere else, and it gave no road for an installed CLI, which has no
`./scripts/bootstrap.sh`. `nros locate tool nros-launch-resolve` had its own
text for the same miss, which listed the rungs but not their paths.

## Fix

- `model_location::launch_resolver_rungs` lists the ladder's rungs in order,
  each with the path it would answer. `launch_resolver_from` is now the first
  of those paths that is a file, so the two cannot drift.
- `launch_resolver_missing_help{,_for_exe}` is the one refusal text. It names
  every rung with its concrete path (or "not applicable here"), then the road
  for each track: the checkout recipes plus the submodule, the release asset
  plus `install.sh`, and `export NROS_LAUNCH_RESOLVE=<path>`.
- `nros sync` and `nros locate tool nros-launch-resolve` both print it.

Test: `cmd::ws::launch_resolver_tests::a_missing_resolver_names_every_rung_it_tried`.
The existing ladder tests still pass unchanged against the refactored
`launch_resolver_from`.
