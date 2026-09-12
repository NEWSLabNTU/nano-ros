---
id: 1358
title: "`check-api-parity` rewrites the tracked root `Cargo.lock`, because the
  `--locked` shim looks for the lock in the MANIFEST dir"
status: open
type: bug
area: ci
related: [issue-0359, issue-0378, issue-0386, issue-1066, issue-0986]
---

## The defect

`just check api-parity` leaves the tree DIRTY. Measured on a pristine worktree
at `6f40985a5`, three times, with `git checkout -- Cargo.lock` between runs:

```
$ git status --porcelain            # clean
$ just check api-parity             # rc=0, "every divergence carries a ledger entry"
$ git status --porcelain
 M Cargo.lock
$ git diff Cargo.lock
   name = "nros-rmw-cyclonedds"
    "critical-section",
 -  "heapless 0.8.0",
 +  "heapless",
```

## Mechanism, measured rather than inferred

`scripts/api-parity.py::ours_rust` runs `cargo doc --lib` with
`cwd = packages/api/nros`. Reproduced standalone from that directory with the
same features and `CARGO_TARGET_DIR`: rc=0, and `Cargo.lock` modified. The same
command with `cwd` at the repo root does not do it.

`scripts/bin/cargo` decides whether to inject `--locked` with

```
git -C "$_manifest_dir" ls-files --error-unmatch Cargo.lock
```

and `packages/api/nros` is a WORKSPACE MEMBER — its lock is the root's. So:

```
$ cd packages/api/nros && git ls-files --error-unmatch Cargo.lock
error: pathspec 'Cargo.lock' did not match any file(s) known to git
```

the predicate fails, the shim treats the crate like an example leaf whose lock
is deliberately untracked (issue 0386), and cargo is free to re-resolve and
rewrite. Meanwhile `cargo metadata --locked` at the root exits 0, so the
committed lock is not stale in cargo's eyes when it is asked from the root.

Every cargo invocation in the tree whose `cwd` is a workspace MEMBER rather
than the workspace root is in this class; `api-parity` is where it surfaced.

## Why it matters more than it did

Issue 1066 put `api-parity` on the fast lane, which is the merge-gating one, and
the fast lane fans out at `-P4` over 319 gates that share this checkout. A gate
that rewrites a tracked file while its siblings read it is the hazard
`check-hook-repo-side-effects` exists for one layer over (issues 0986/0988).
Locally it is worse than in CI: a CI checkout is ephemeral, but a developer's
`just ci gate` leaves a `Cargo.lock` edit that `git add -u` scoops up, and a
lockfile is supposed to move only when a dev means it (issues 0359/0378).

## Why it was NOT fixed in 1066's change

The obvious fix — inject `--locked` for this call — turns a silent rewrite into
a hard `the lock file needs to be updated but --locked was passed`. Whether the
committed lock survives that from a member dir is unmeasured, and putting an
untested hard failure into a lane that has just started gating merges is
precisely the "do not land a required step that is red" rule 1066 was written
around.

## Candidate fixes, in rising blast radius

1. Give `extract_rust.rustdoc_json` an explicit `locked=` and pass it for
   `ours_rust` only (`derive_rclrs` points at an out-of-tree rclrs checkout).
   Measure whether the committed lock passes `--locked` from
   `packages/api/nros` first; if it does not, that is itself a finding.
2. Run the extraction from the workspace root with `-p nros` instead of `cd`ing
   into the member.
3. Teach the shim to resolve the workspace root. `cargo locate-project
   --workspace` answers exactly, and costs a cargo invocation per cargo
   invocation; a path walk to the git toplevel is cheap and WRONG, because it
   would inject `--locked` into every example leaf the shim's comment
   deliberately exempts.

Whichever lands, the sweep is "every cargo call site whose cwd is a workspace
member", not this one call.
