---
id: 2699
title: "A SystemModel written by a different launch resolver is not re-resolved when this CLI cannot parse it, so a persistent fixture tree fails the build"
status: open
type: bug
area: [cli, ci, testing]
severity: medium
found: 2026-11-18
related: [2296, 1360]
---

## Measured

`run-matrix` run 44239068930 (workflow_dispatch, 2026-11-18, branch
`w13-step3-u`, the phase-556 W13 step-3 tip before its rebase) ran on the
self-hosted runner. All three lanes
(`tier 1 (cells)`, `tier 2 (1-wise matrix)`, `tier 2 nightly (pairwise cover)`)
failed in `just build <lane>`, at `generate-bindings`, before any cell:

    Error: load SystemModel /home/runner/_work/nano-ros/nros-fixture-tree/examples/workspaces/bridge-cyclonedds/build/nros/models/demo_bringup/system_model.yaml: unsupported SystemModel schema version 6 (this reader supports <= 5)

    Location:
        nros-cli-core/src/orchestration/model_ingest.rs:76:22

The branch was cut from a main whose `packages/cli/third-party/play_launch`
pin reads schema 5. Main has since bumped the pin to a resolver that writes
schema 6, and a scheduled run of main resolved `demo_bringup` into the
runner's persistent `nros-fixture-tree` with it. The branch's CLI met that
model and refused it instead of re-resolving it with its own resolver.

The same thing reproduces locally in one worktree. Build fixtures at
origin/main, then check out an older commit (that same step-3 branch, cut
before main's schema-6 pin bump), run `just setup-cli`,
`just setup-launch-resolve` and `nros configure --force examples/linux/rust/talker`.
`nros build` then still loads the schema-6
`examples/linux/rust/talker/build/nros/models/native_talker_bringup/system_model.yaml`
and stops with the same refusal (`no entry package: the generated one could not
be written`).

## Why the existing guards miss it

Issue 2296 made the play_launch pin a freshness input: the configure stamp
records `resolver_pin`, and sync re-resolves a model whose `meta.resolver` pin
differs from the CLI's (`model_gate::provenance_stale`). That works only while
this CLI can PARSE the model. `provenance_stale_with`
(`packages/cli/nros-cli-core/src/model_gate.rs`) does

    let model = ros_launch_manifest_model::SystemModel::from_yaml_str(&raw).ok()?;

and its doc says so: "A model that cannot be parsed returns `None`, so
`run_sync` falls back to its mtime gate". A model from a NEWER schema cannot be
parsed by an older reader, so it reads as "not stale", the mtime gate finds
its inputs unchanged, and nothing re-resolves it. The consumer
(`model_ingest::load_model`) then refuses it. The pin check that would have
caught it never runs, because it sits after the parse.

## Same class as 1360 and 2296

Issue 1360: the codegen version a persistent store was built by is a
freshness input, not just its mtime. Issue 2296: the resolver pin a model was
written by is a freshness input. This is the case 2296 left open: the identity
check must not depend on the reader understanding the rest of the file.

## What would close it

- A model's freshness keys on the resolver identity and version that wrote
  it, read WITHOUT a full parse (its `meta.resolver` and schema version are
  enough), so an unparseable model from a different resolver is STALE, not
  current.
- A reader that meets a model from a different resolver re-resolves it with
  its own pinned resolver. Refusing stays only for a model it cannot
  regenerate (a hand-authored or committed model, `model =` overrides, no
  bringup to re-resolve from).
- A unit test in `model_gate`: a model carrying a schema version above the
  reader's is stale with a reason naming the schema, and sync re-resolves it.

## Which tree is shared

The self-hosted runner's `nros-fixture-tree`
(`/home/runner/_work/nano-ros/nros-fixture-tree/`), which persists across
runs, is shared by every `run-matrix` dispatch on that runner: main's
scheduled runs and any branch dispatch. Locally the shape is any worktree
whose `build/nros/models/` outlives a checkout of a different commit.

## Does a branch newer than main hurt main the same way?

Yes, by the same mechanism in the other direction. A branch that bumps
play_launch to a resolver with a newer schema, dispatched on the self-hosted
runner, leaves models main's CLI cannot parse; main's next scheduled run then
refuses them in its build stage. Today's instance went the other way (main
newer than the branch), but nothing in the gate depends on the direction.

## Acceptance

After the fix, a `run-matrix` dispatch of a branch older than main's
play_launch pin gets past `generate-bindings` on the self-hosted runner with
no hand cleanup of `nros-fixture-tree`, and so does main's next scheduled run
after a newer-branch dispatch.
