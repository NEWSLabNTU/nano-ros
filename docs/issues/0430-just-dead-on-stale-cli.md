---
id: 430
title: A stale in-tree `nros` makes EVERY `just` recipe fail, including the one
  that rebuilds it
status: open
type: bug
area: build
related: [0363, 0197, phase-336]
---

## Problem

`just` evaluates every variable in the loaded justfiles before running any
recipe, and `just/qemu-baremetal.just:19` is a backtick:

```just
PROFILE_DIR := `bash -c 'source scripts/build/cargo.sh && nros_cargo_target_profile_dir'`
```

That function calls `nros profile …` (phase-336). An `nros` predating that verb
exits with clap's usage error, the backtick fails, and `just` refuses to run
**anything**:

```
error: backtick failed with exit code 1
  ——▶ just/qemu-baremetal.just:19:16
   │
19 │ PROFILE_DIR := `bash -c 'source scripts/build/cargo.sh && nros_cargo_target_profile_dir'`
```

Including `just setup-cli` — the recipe whose entire job is to rebuild the
binary that would fix this. The tree is bricked from the user's point of view.

## Why it is easy to hit and hard to read

Pulling a branch that adds a CLI verb used at justfile-EVALUATION time is enough.
It happened here simply by rebasing onto main with a CLI built an hour earlier.

The message names `qemu-baremetal.just` and a profile helper. Nothing mentions
the CLI, its staleness, or `setup-cli`. A reader with no context looks at the
QEMU baremetal platform first — which has nothing to do with it.

Note the existing stale-CLI guard (0363/0197) cannot help: it lives *inside*
`nros`, and the failure is that `nros` cannot be invoked usefully at all.

## Escape hatch (what unblocks it today)

```sh
cd packages/cli && cargo build --release --bin nros
```

Bypasses `just` entirely. Obvious in hindsight, invisible from the error.

## Fix directions

1. **Make the backtick degrade.** `nros_cargo_target_profile_dir` already
   returns non-zero when the query fails; the justfile could tolerate that and
   fall back to a default, so evaluation never dies. A wrong `PROFILE_DIR` for
   one platform recipe is a far smaller failure than every recipe refusing to
   run.
2. **Move the query out of evaluation.** Make `PROFILE_DIR` a recipe-local shell
   assignment rather than a `:=` variable, so only recipes that need it pay for
   it — and only they fail when the CLI is stale.
3. **Say what is wrong.** If the query fails, have `_nros_profile_query` emit
   "in-tree nros is stale or predates `nros profile`; run
   `cd packages/cli && cargo build --release --bin nros`" on stderr. The
   information exists at the failure point; today it is discarded.

(2) looks strongest: it removes the coupling rather than papering it.

## Notes

Hit 2026-08-05 while provisioning NuttX for issue 0420, on an ordinary
`git rebase origin/main` + stale CLI.
