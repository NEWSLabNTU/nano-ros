---
id: 1659
title: "The CLI's play_launch pin reads the SUPERPROJECT HEAD under an inherited
  GIT_DIR, so `nros sync` refuses every step of a `git bisect run`"
status: open
type: bug
area: [cli, tooling]
severity: low
found: 2026-10-03
related: [986, 988, 1336, 409, 1657]
---

## What

Measured while bisecting issue 1657. Inside `git bisect run <script>`, git
exports `GIT_DIR=<repo>/.git/worktrees/<name>` (also `GIT_PREFIX`,
`GIT_EXEC_PATH`). `just setup-cli` then builds `nros` whose
`NROS_PLAY_LAUNCH_SHA` comes from
`nros_cli_core::source_stamp::play_launch_pin`, which runs
`git -C packages/cli/third-party/play_launch rev-parse HEAD`. `GIT_DIR`
overrides `-C`, so the answer is the SUPERPROJECT's HEAD, and every `nros sync`
then refuses:

```
Error: sync: `…/nros-launch-resolve` was built from play_launch bbf9c04496d0
but this `nros` was built from fba1b024a9f2.
```

— the second sha is the nano-ros commit under test, not a play_launch commit.
25 consecutive bisect steps were SKIPped this way. Clearing every `GIT_*`
variable in the bisect script made the same steps build.

## Why it matters

Same class as issues 0986/0988 (a hook-reached script inheriting git's
environment), one layer down: the `git()` helper in `source_stamp.rs` (and
`build.rs` through it) spawns git with the caller's environment. `git bisect run`
is the obvious tool for a regression hunt, and this makes it unusable for any
step that runs `nros sync`, with a message that blames the resolver.

## Fix direction

Clear git's local environment (`git rev-parse --local-env-vars`, the list
`scripts/lib/git_hook_env.py` uses) on the commands `source_stamp::git` spawns —
one Rust spelling of `nros_clear_inherited_git_env`, rather than per-call
`env_remove`s. `check-hook-repo-side-effects` does not cover Rust build scripts.
