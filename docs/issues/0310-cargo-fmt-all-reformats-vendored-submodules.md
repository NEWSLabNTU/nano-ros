---
id: 310
title: "`cargo +nightly fmt --all` reformats vendored submodule sources, producing drive-by diffs in another repo"
status: open
type: limitation
severity: low
area: build, tooling
related: []
---

## Finding (2026-07-28)

`cargo +nightly fmt --all` at the repo root reformats crates inside
`packages/cli/third-party/`, which are SUBMODULES tracking other repositories.
Observed while formatting an unrelated change: the run rewrote
`ros-launch-manifest/sched/src/chain_aware_mapper.rs`, leaving the submodule
`-dirty`. That in turn blocked `git rebase` in the superproject with

```
error: cannot rebase: You have unstaged changes.
```

which surfaces far from its cause — the rebase failure looks like a git problem,
not a formatting one. It cost a push cycle to trace.

The diff itself was legitimate rustfmt normalization; the problem is authorship,
not correctness. A nano-ros change should not carry unrelated reformatting of a
vendored repo — it either has to be discarded (losing nothing) or pushed to the
fork (a cross-repo commit nobody asked for).

## Mechanism (measured 2026-07-28, correcting this issue's first draft)

The first draft blamed workspace membership. That is wrong, and worth recording
because it points at the wrong fix: **neither workspace lists the vendored
crates as members.** `Cargo.toml` excludes `packages/cli` outright (line ~504),
and `packages/cli/Cargo.toml`'s `members` does not mention `third-party/`.

What actually happens is that `cargo fmt --all` formats **path dependencies**,
crossing workspace and directory boundaries. Measured on a clean tree:

| command | reaches |
|---|---|
| `cargo +nightly fmt` (what `just format-workspace` runs) | root workspace members only — vendored trees untouched |
| `cargo +nightly fmt --all` at the repo root | also `packages/cli/nros-cli-core`, which the root workspace EXCLUDES |
| `cargo +nightly fmt --all` in `packages/cli` | also `packages/core/nros-orchestration-ir`, outside that directory entirely |

So the blast radius follows the path-dep graph, and the vendored submodules sit
in it (`nros-macros` → `ros-launch-manifest-model`,
`nros-orchestration-ir` → `ros-launch-manifest-sched`).

**The project's own recipes are safe**: `just format` → `format-workspace` →
plain `cargo fmt`, which touched nothing outside the root workspace. The hazard
is reaching for `--all` by hand, which is what happened.

**It is also latent, not deterministic.** The diff only appears when the pinned
submodule's sources are not already rustfmt-clean under our `rustfmt.toml`. At
the time of the original incident the rlm pin had an unformatted
`chain_aware_mapper.rs`; after later pin bumps the same commands produce no
diff at all. So this reproduces on some pins and not others — which is exactly
the kind of intermittency that makes the eventual rebase failure baffling.

## Workaround

Format the crates that actually changed (`cargo +nightly fmt -p <crate> …`)
rather than `--all`. If `--all` has already run, revert the submodule with
`git -C <submodule> checkout <path>` before committing.

## Direction

Given the measurement, the ranking changes: the recipes are already correct, so
this is about catching a hand-run `--all`, not about fixing `just format`.

1. **A `just check` gate that fails when a submodule is left dirty.** Catches the
   general case — any tool that writes into a vendored tree, not just rustfmt —
   and converts the current symptom (a `git rebase` error that never mentions
   formatting) into a named one. Best failure mode, and it does not depend on
   enumerating paths that will drift.
2. **`rustfmt.toml` `ignore = [...]`** for the vendored paths. Prevents rather
   than reports, but rustfmt's `ignore` is nightly-only and silently no-ops in
   some invocations — verify against the pinned toolchain before relying on it,
   or it becomes a guard that looks present and does nothing.
3. **Document that `--all` is not the project's format command.** `just format`
   is; `--all` reaches outside the repo. Cheapest of the three and worth doing
   regardless of (1)/(2).

Not worth doing: reworking the workspace membership. Membership is already
correct — path deps are the mechanism, and they are load-bearing.
