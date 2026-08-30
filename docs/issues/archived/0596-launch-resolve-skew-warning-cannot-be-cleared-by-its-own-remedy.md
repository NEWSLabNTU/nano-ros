---
id: 596
title: The `nros-launch-resolve` skew warning is mtime-only, so its own remedy cannot clear it
status: resolved
type: bug
area: build
related: [issue-0363, issue-0466, issue-0561]
---

## What happens

After any `just setup-cli`, every subsequent `just check tier-preconditions`
prints:

```
check-tier-preconditions: WARNING — nros-launch-resolve is OLDER than
  the in-tree CLI. They are separate recipes that must agree on an
  argument list (issue 0363 C); a skew surfaces deep in a fixture
  build, not here.
  Remedy: just setup-launch-resolve
```

Running the remedy does not clear it. Running it repeatedly does not clear it.

## Why

The check is a bare mtime comparison of two BINARIES:

```sh
# scripts/check-tier-preconditions.sh:143-151
_resolver="packages/cli/nros-launch-resolve/target/release/nros-launch-resolve"
_cli_bin="packages/cli/target/release/nros"
if [ -f "$_resolver" ] && [ -f "$_cli_bin" ] && [ "$_cli_bin" -nt "$_resolver" ]; then
```

`just setup-launch-resolve` delegates to cargo. When the resolver's sources have
not changed, cargo correctly does nothing — it does not relink — so the binary's
mtime does not move. Meanwhile `setup-cli` has just produced a newer `nros`. The
predicate `cli -nt resolver` is therefore true forever, and the only thing that
can falsify it is an unrelated change to the resolver's own sources.

Measured on this tree after running the remedy twice:

```
2026-08-15 12:12:24  packages/cli/target/release/nros
2026-08-15 11:10:33  packages/cli/nros-launch-resolve/target/release/nros-launch-resolve
```

## Why it matters

The warning is about a REAL hazard — issue 0363 C, the two binaries must agree
on an argument list, and a skew "surfaces deep in a fixture build, not here".
That is worth warning about. But a warning that fires on a condition its remedy
cannot change is a warning readers learn to scroll past, and this one prints on
every tier check, next to the errors that DO matter (`check-tier-preconditions`
exists precisely to list every unmet precondition at once — issue 0466). Crying
wolf in that list is a direct cost to the recipe's purpose.

It is also a false negative in the other direction: the thing that matters is
whether the two were built from the same SOURCES. Touching the resolver binary
would silence the warning without proving anything, and a genuine skew — the
resolver built from an older commit whose binary happens to be newer than the
CLI's — is not detected at all. **mtime is not the question being asked.**

## Two spellings, which is how this survives a fix

The same comparison exists twice, verbatim in intent:

* `scripts/check-tier-preconditions.sh:145`
* `justfile:3958` (inside `setup-cli`, printing `[setup-cli] WARNING: …`)

Fixing one leaves the other. Per the fix-the-class rule they must move together,
and ideally become one helper.

## Direction

The CLI already has the mechanism: `setup-cli` records a SOURCE STAMP
(`nros-cli-core/src/lib.rs`, "source-stamp: STALE — built from <hash>, sources
are now <hash>"), which is a content question with a clearable answer. Give
`nros-launch-resolve` the same treatment and compare stamps, not mtimes:

1. Stamp the resolver build with the hash of its sources (and of the argument
   list it agrees with the CLI on, which is the actual invariant from 0363 C).
2. Warn when the stamps disagree — a condition `just setup-launch-resolve`
   genuinely clears.
3. One helper, called from both sites above.

Until then the warning should at minimum say it may be spurious after a CLI-only
rebuild, so a reader does not chase it.

## Evidence

Observed 2026-08-15 on `wip/feature-contract`. The branch changes no file under
`packages/cli/nros-launch-resolve/`, so `setup-launch-resolve` is a genuine
no-op here and the warning is certainly spurious in this instance.


## Resolved 2026-08-17

**Three copies, not two.** The issue names the helper's two callers; the third
was an inline mtime walk inside `setup-launch-resolve` itself — the copy that
decides whether to REBUILD, and therefore the one that kept the recipe
early-exiting without building. All three now call one helper.

**The helper was already half-fixed, and the remaining half was the mtime.** It
had been changed to ask about SOURCES rather than binaries, which addressed "the
remedy cannot clear it". It still compared source MTIMES, so any `git rebase`,
`git stash pop` or `git checkout` re-armed it by rewriting tracked files with
identical bytes — the treadmill `source_stamp.rs` documents for the CLI. It now
compares a content stamp, recorded beside the binary by the build.

**Scope, on evidence rather than omission.** `Cargo.lock` joins `.rs` /
`Cargo.toml`: it pins what the binary was built from. The other 160 tracked
files in the resolver tree stay out because they cannot change the Rust binary —
67 are `.py` belonging to layer 2's CPython runtime, and nothing embeds them
(no `include_str!`/`include_bytes!`, no `build.rs` in that tree); the rest are
README / LICENSE / .gitignore. Watching them would force a rebuild on a docs
edit.

Verified end to end, all five states:

```
no stamp                  -> STALE
run `just setup-launch-resolve` -> current
run it again              -> current   (was: stale forever)
edit a tracked .rs        -> STALE
restore it                -> current
touch the same file       -> current   (the false positive is gone)
```

### One defect in the fix, worth recording

The first content stamp was PATH-DEPENDENT. `sha256sum` prints
`<hash>  <path>`, so hashing that line verbatim made the digest depend on how
the caller spelled the root — `setup-launch-resolve` passes an absolute
`justfile_directory()`, `check-tier-preconditions` passes `.` — and the two
disagreed for an identical tree. That reproduced this issue's exact symptom:
stale immediately after its own remedy. The digest now carries repo-relative
paths.
