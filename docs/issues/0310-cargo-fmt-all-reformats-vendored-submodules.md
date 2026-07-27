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

## Why the existing guards miss it

The repo already knows about this class of hazard for OTHER formatters:
`.clang-format-ignore` guards `cmake/templates/*` (issue 0159), and the format
recipes exclude generated dirs. Rust formatting has no equivalent exclusion —
`--all` follows workspace membership, and the vendored crates are path deps of
the root workspace, so they are members.

## Workaround

Format the crates that actually changed (`cargo +nightly fmt -p <crate> …`)
rather than `--all`. If `--all` has already run, revert the submodule with
`git -C <submodule> checkout <path>` before committing.

## Direction

Options, cheapest first:

1. **A format recipe that enumerates first-party crates** instead of `--all`,
   the way the C/C++ recipes already exclude what they must not touch. Keeps one
   command for contributors.
2. **`rustfmt.toml` `ignore = [...]`** entries for the vendored paths. Simpler,
   but rustfmt's `ignore` is nightly-only and silently no-ops in some
   invocations — needs verifying against the pinned toolchain before relying on
   it.
3. **A `just check` gate** that fails when a submodule is left dirty by a format
   run. Catches the general case (any tool touching vendored trees), not just
   rustfmt, and turns the confusing rebase failure into a named error.

(3) has the best failure mode — the current symptom is a git error that does not
mention formatting — but (1) prevents rather than reports.
