---
id: 1081
title: "`_setup-common`'s submodule guard tests PRESENCE, not the pin — so a persistent CI workspace builds against the wrong play_launch and froze the merge queue for seven PRs"
status: resolved
type: bug
area: tooling, ci
severity: high
found: 2026-09-05
related: [0409, 0996]
---

# One red, seven pull requests, none of them the cause

`just queue-triage` reported `queue` failing in the merge group for **seven
different pull requests** (#333, #429, #438, #459, #478, #479, #482), every one
of them green on its own branch. The same check failing across unrelated changes
is not a property of any of them.

Every failure was byte-identical:

```
error: cannot update the lock file …/packages/cli/nros-launch-resolve/Cargo.lock
because --locked was passed to prevent this
error: recipe `setup-launch-resolve` failed with exit code 101
```

## Cause

`_setup-common` guarded the play_launch submodule like this:

```sh
sub="packages/cli/third-party/play_launch"
if [ ! -f "$sub/src/ros-launch-resolve/resolve/Cargo.toml" ]; then
    git submodule update --init "$sub"
fi
```

That answers **"is it initialised?"**. It does not answer **"is it at the commit
this tree pins?"** — and a checkout can be initialised and at the wrong commit.

The CI runner is self-hosted with a persistent workspace, and
`actions/checkout` runs with `submodules: false`, so nothing moves the submodule
back to the pin between runs. `git submodule status` in the run log reads:

```
+8fda8d89e7314e7684e22673c2c0b8f32d4f560c packages/cli/third-party/play_launch (heads/main)
```

The leading `+` is git saying "differs from the recorded pointer". The pin is
`4c214a63`. The file the guard looks for exists at either commit, so the guard
skipped, and every build resolved `nros-launch-resolve` against a source tree
its committed `Cargo.lock` does not describe. Cargo wanted to rewrite the lock;
`--locked` — injected project-wide by the `scripts/bin/cargo` shim — correctly
refused.

So the error names the lockfile, and the lockfile is fine. Nothing about the
message points at a submodule.

## Reproduced locally, exactly

```sh
git -C packages/cli/third-party/play_launch checkout HEAD~2   # = 8fda8d89
just setup-launch-resolve
# error: cannot update the lock file … because --locked was passed
```

The runner was simply two commits ahead on `main` and behind the pin — the
superproject's pointer and the checkout disagreeing, which is the state
CLAUDE.md already says is fixed by `git submodule update <path>`.

## Fix

The guard now reads `git submodule status --cached`, whose first column
distinguishes the two states that need different handling:

- `-` — not initialised. Init it, non-recursively (RFC-0060).
- `+` — present at another commit. If the submodule worktree is **clean**,
  update it to the pin and say so. If it is **dirty**, FAIL with the command,
  because `git submodule update` discards work in a submodule and AGENTS.md
  makes that a human decision. Failing loudly beats resolving against a tree the
  lockfiles do not describe.

The distinction matters: `post-rebase` deliberately only REPORTS a moved
submodule for exactly this reason, and it would have named this one. `setup` is
a provisioning verb, so it may repair — but only where there is provably nothing
to lose.

## What this is not

Not a lockfile problem, and `nros-launch-resolve/Cargo.lock` is unchanged. A
regenerated lock would have made the symptom disappear on the runner while
encoding the wrong graph for everyone else.

## Related

Issue 0996 is the same lane failing for a different reason a hand-rolled
workflow step was hiding. The pattern both share: a claim that the toolchain
does the work, where the toolchain does *almost* the work.
