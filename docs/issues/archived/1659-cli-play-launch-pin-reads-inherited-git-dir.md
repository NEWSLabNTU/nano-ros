---
id: 1659
title: "The CLI's play_launch pin reads the SUPERPROJECT HEAD under an inherited
  GIT_DIR, so `nros sync` refuses every step of a `git bisect run`"
status: resolved
type: bug
area: [cli, tooling]
severity: low
found: 2026-10-03
resolved_in: 2026-10-03
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

## Resolution

**One helper, `packages/cli/build-support/git_env.rs`, and every `git` the CLI
spawns goes through it.** `nros_clear_inherited_git_env(&mut Command)` removes
what `git rev-parse --local-env-vars` names (the probe itself runs with every
`GIT_*` removed, because an inherited `GIT_DIR` naming a non-repository makes
`rev-parse` die before it answers); `nros_git_command(program)` is a `Command`
with that already done. Same identifier as the shell and Python helpers, which
is what `check-hook-repo-side-effects` credits.

It is `include!`d, because the consumers cannot share a crate: by
`nros-cli-core/src/source_stamp.rs` (so the crate and its `build.rs`, which
both compile that file, get it from one place), by
`nros-launch-resolve/build.rs` (the other half of the 0409 pin comparison) and
by `cargo-nano-ros` (`scaffold.rs`'s `git config`).

The sweep (`git grep -n 'Command::new("git")\|"git",\|git_program()' --
'packages/cli/**/*.rs' ':!packages/cli/third-party'`) found more than the
issue named:

| site | before | now |
| --- | --- | --- |
| `source_stamp::git` (pin, index path, ls-files, …) | inherited env | `nros_git_command(git_program())` |
| `nros-launch-resolve/build.rs` pin | inherited env | `nros_git_command("git")` |
| `sdk_store::{sh_with_toolchain, sh_capture_raw}` — 19 `git` argv sites, three of them `git init <store dir>` | inherited env — 0986's WRITE hazard | `store_command()`, which clears for EVERY provisioning command (they run `make`/`cmake`/`cargo`, which run git) |
| `cargo-nano-ros` `git config user.*` | inherited env | `nros_git_command("git")` |
| `source_stamp` / `stale_guard` test `sh()` | each hand-rolled the same query | the shared helper |

### Measured

- `source_stamp::tests::play_launch_pin_ignores_an_inherited_git_dir` re-runs
  itself in a child whose INHERITED `GIT_DIR` names a different repository (a
  test may not write its own process environment, which concurrent tests
  read). Passes. **Negative control:** with the helper made to keep `GIT_DIR`,
  the child fails with `left: Some("eefa17f…")` (the decoy's HEAD) against
  `right: Some("6cef69a…")` (the submodule's) — the issue's symptom.
- `check-hook-repo-side-effects` gained a CLI-Rust section
  (`Command::new("git")` refused outside the helper; an argv runner must route
  through `nros_clear_inherited_git_env`) with its own inline negative control.
  Its patterns fire on all four pre-fix files from `origin/main`
  (`source_stamp.rs`, `nros-launch-resolve/build.rs`, `scaffold.rs`, and
  `sdk_store.rs` with 19 argv sites and no clear); green on this branch.
- `just check cli-tests`: 2516 passed, 0 failed. 1.99 `just check
  test-targets`: clean.

### Not measured

- The end-to-end `git bisect run` → `just setup-cli` → `nros sync` path: this
  session's sandbox refuses to spawn anything with `GIT_DIR` set, so the
  rebuilt binary was not exercised under it. The child-process test is the
  substitute — it reaches `play_launch_pin` through the same `git()` that
  `build.rs` `include!`s.
- `cargo` itself (which the CLI spawns for builds) resolves git dependencies
  with its own git; that spawn is cargo's and was not changed.
