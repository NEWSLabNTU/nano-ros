---
id: 1863
title: "`just setup-launch-resolve` built the parser's Python half without `--locked`, rewriting play_launch's tracked `parser/Cargo.lock` on every setup"
status: resolved
type: bug
area: [build, ci]
severity: medium
found: 2026-10-13
resolved: 2026-10-13
related: [1841, 0359, 0378, phase-485]
---

## Measured

After any `just setup` (it runs `setup-launch-resolve`), the play_launch
submodule was dirty:

```
 M src/ros-launch-resolve/parser/Cargo.lock
```

The diff is always the same: 52 lines deleted, the whole `[[patch.unused]]`
table (`action_msgs`, `builtin_interfaces`, `composition_interfaces`, …).
Seen in an agent worktree after a rebase (the same residue blocked its next
`setup-launch-resolve` with `cannot update the lock file … --locked`), and in
the CI log of issue 1841's first passing run on `nano-ros-runner-newslab-133`,
where it was the residue the fix discarded.

## Cause

`setup-launch-resolve` runs two cargo builds. Phase-485 W4 retired the
`--locked` PATH shim and moved `--locked` into the recipes that build a
closed-graph root, but onto the FIRST build only (`nros-launch-resolve`). The
second, the parser's Python half in play_launch's own `parser` workspace, ran
unlocked.

That lock is play_launch's tracked file. Upstream commits it from a colcon
environment, whose ros2_rust config patches the message crates, so it carries
a `[[patch.unused]]` table no config here produces. `cargo metadata --locked`
accepts the lock unchanged (rc=0, file untouched); an unlocked call, from the
repo root or from inside `parser/`, rewrites it without the table.

`check-cargo-locked` did not see the gap because it matched two AUTHORED
lines, one per recipe — so one locked call satisfied it while the other ran
unlocked (issue 0196's reach shape).

In a developer tree this is a dirty submodule after every setup; in a
persistent CI workspace it is the dirt that made issue 1841's guard refuse
after the next pin bump.

## Fix

- The parser build passes `--locked`.
- `check-cargo-locked` asks EVERY cargo invocation in the closed-root recipes
  (`setup-cli`, `setup-launch-resolve`; comments skipped, `\` continuations
  joined) for `--locked`, and fails a recipe with no cargo call at all, so a
  rename cannot pass vacuously. Self-test cases: an unlocked second build
  continued over lines, a renamed recipe.

## Evidence

- Gate on the old recipe: rc=1, naming
  `cargo build --release --manifest-path "$_pyexec_ws/Cargo.toml" -p play_launch_parser_pyexec …`.
  Fixed recipe: rc=0 (3 cargo calls in 2 recipes + repo-root nextest).
- Real build: `setup-launch-resolve` with its stamp removed (forcing both
  builds) rc=0, and `git -C packages/cli/third-party/play_launch status
  --porcelain` empty afterwards. Before the fix the same setup left the 52-line
  deletion.
