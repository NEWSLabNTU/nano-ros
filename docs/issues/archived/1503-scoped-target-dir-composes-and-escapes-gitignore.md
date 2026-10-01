---
id: 1503
title: "`nros_scoped_target_dir` COMPOSES onto an already-scoped base, so two `check-cpp` scratch dirs land at the repo root under names `.gitignore` does not carry — and the gate for exactly this cannot see them"
status: resolved
type: bug
area: build, ci
severity: low
related: [0400, 0196, 1071, 1354]
found: 2026-09-25
resolved: 2026-10-01
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

## Re-measured 2026-10-01 — the composition is GONE, and was fixed the same day

`just check cpp` in a clean worktree, from no `target-*` dirs:

```
target-check-cpp
target-check-cpp-clippy-zenoh
target-check-cpp-cyclone-embedded
```

and `git status --porcelain` prints **nothing** untracked at the repo root. The
composed names this issue measured (`target-check-cpp-check-cpp-clippy-zenoh`
and its sibling) do not appear, and `.gitignore`'s three existing entries match
the three directories exactly.

**It was fixed by `94124a0d6` on 2026-09-25** — the same day this was filed,
under issue **1354**, which hit the same composition from the build side (*"a
nested scoped-dir call compounds its own prefix"*). Nobody closed this one. The
fix captured the lane's base before the export and derived the two names from
it, which is the shape this issue asked for.

## The residual it left, and what closes it

`94124a0d6` replaced the two `nros_scoped_target_dir` CALLS with string
concatenation on the captured base:

```sh
CARGO_TARGET_DIR="$gen_base-check-cpp-clippy-zenoh"
```

Correct names, and **invisible to `check-scoped-target-dirs-ignored`**, which
harvests `nros_scoped_target_dir <suffix>` call sites. So from 2026-09-25 the
two `.gitignore` rows these directories need were right and *unguarded*: the
gate reported 8 call sites over 8 suffixes, and a rename of either suffix in
those two string literals would have left a new untracked directory at the repo
root with the gate still green. That is this issue's own class — a gate whose
reach is narrower than the rule — one step over from where it found it.

Closed by calling the one spelling BEFORE the export, which yields the identical
name and puts both suffixes back in the harvest:

```sh
gen_cyclone_embedded="$(nros_scoped_target_dir check-cpp-cyclone-embedded)"
gen_clippy_zenoh="$(nros_scoped_target_dir check-cpp-clippy-zenoh)"
gen="$(nros_scoped_target_dir check-cpp)"
export CARGO_TARGET_DIR="$gen"
```

`check-scoped-target-dirs-ignored` now reports **10 call sites over 10
suffixes**, and `just check cpp` re-run afterwards produces the same three
directory names with nothing untracked.

## What this issue got right and wrong

Right: *"not add the two names to `.gitignore`"* — the fix is in the derivation,
not the ignore file, and `.gitignore` needed no change in either pass.

Wrong, or at least unlucky: it proposed teaching the GATE to track a recipe
body's `CARGO_TARGET_DIR` export and compose onto it. That is not needed. The
design question it left open — *is a second call inside a re-based body
arguably a mistake?* — is answered by the fix: yes, and the answer is to make
the call before the body re-bases, not to teach a Python gate to simulate shell
scoping.
