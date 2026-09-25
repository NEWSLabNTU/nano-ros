---
id: 1503
title: "`nros_scoped_target_dir` COMPOSES onto an already-scoped base, so two `check-cpp` scratch dirs land at the repo root under names `.gitignore` does not carry — and the gate for exactly this cannot see them"
status: open
type: bug
area: build, ci
severity: low
related: [0400, 0196, 1071]
found: 2026-09-25
---

# A gate that checks `target-<suffix>` cannot see `target-<base>-<suffix>`

## Measured

After one `just ci gate` in a clean worktree, `git status` shows two untracked
directories at the repo root:

```
?? target-check-cpp-check-cpp-clippy-zenoh/
?? target-check-cpp-check-cpp-cyclone-embedded/
```

Neither name is in `.gitignore`. The file carries the un-composed spellings:

```
113:/target-check-cpp/
114:/target-check-cpp-clippy-zenoh/
115:/target-check-cpp-cyclone-embedded/
```

## Why the names differ

`nros_scoped_target_dir` (scripts/build/cargo.sh) is deliberately RELATIVE to
whatever base is active — that is issue 0400, so a ROS distrobox's
`CARGO_TARGET_DIR` redirect is not undone by a recipe hardcoding a relative dir:

```sh
nros_scoped_target_dir() {
    printf '%s' "${CARGO_TARGET_DIR:-$PWD/target}-$1"
}
```

The `check-cpp` lane sets the base for its whole body
(`just/check/lanes.just:848`):

```sh
gen="$(nros_scoped_target_dir check-cpp)"
export CARGO_TARGET_DIR="$gen"          # => $PWD/target-check-cpp
```

and two calls INSIDE that body then compose onto it:

| call site | suffix | actual dir |
| --- | --- | --- |
| `lanes.just:2093` | `check-cpp-clippy-zenoh` | `target-check-cpp-check-cpp-clippy-zenoh` |
| `lanes.just:1084` | `check-cpp-cyclone-embedded` | `target-check-cpp-check-cpp-cyclone-embedded` |

So the composition is working as designed; the `.gitignore` entries were written
for the name a call would produce from a PLAIN base, which is not the base these
two have.

## Why the gate stays green

`check-scoped-target-dirs-ignored` exists for exactly this class — its own
docstring cites `target-excluded-tests` at 329 MB untracked at the repo root.
It harvests every `nros_scoped_target_dir <suffix>` call and asserts the ignore
file carries the corresponding entry, deriving that entry as (line 121):

```python
wanted = f"/target-{suffix}/"
```

which assumes the base is always plain `target`. It has no way to know that a
call site sits inside a body that re-based `CARGO_TARGET_DIR`, so it passes over
the two names that actually appear and vouches for two that never do. A reach
narrower than the rule it enforces — issue 0196's shape, and the second half of
issue 1071's lesson that a green gate is not the same as a covered case.

The consequence is the hazard CLAUDE.md names by hand: a `git add -A` in a tree
that has run `just check cpp` scoops up two build-output directories. The rule
against blanket adds is what has been catching this.

## What a fix has to do

Not "add the two names to `.gitignore`" — that is the fix-the-site antipattern,
and it goes stale the next time a lane re-bases its target dir. The gate has to
derive the name the way the shell does: track the `CARGO_TARGET_DIR` a recipe
body exports and compose the call's suffix onto it, so the expected entry is
what the call actually produces. Whether the cleaner answer is instead to make
nested scoping impossible (a second call inside a re-based body is arguably a
mistake, and `nros_scoped_target_dir`'s own doc restricts it to EPHEMERAL
scratch with no fixed-path consumer) is the design question this issue does not
settle.

Found incidentally while running `just ci gate` for phase work on the tier-2
lane (issue 1158); not investigated further than the two directories above, so
whether other lanes compose the same way is unmeasured.
