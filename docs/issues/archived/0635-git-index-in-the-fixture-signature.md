---
id: 635
title: "A fixture signature hashes `.git/index`, so every commit stales three compile-check rows — permanently, and only for people who commit"
status: resolved
type: bug
severity: high
area: build/ci
related: [issue-0466, issue-0351, issue-0196, phase-363, phase-360]
---

## Symptom

`just ci` fails at `_check-fixtures-stale`:

```
ERROR: 3 compile-check fixture(s) are missing or stale:
  nav2_compat_smoke (stale .../compile-check-fixtures/nav2_compat_smoke/.inputsig)
  board_agnostic_run_plan (stale .../board_agnostic_run_plan/.inputsig)
  freertos_firmware (stale .../freertos_firmware/.inputsig)
  Run `just build-test-fixtures` before test-all.
```

Running `just build-test-fixtures` does not fix it. The same three are stale on
the next run, and the one after that. Observed across four rebuild-and-rerun
rounds on 2026-08-16 — each round separated from its verification by a commit,
which is the part that matters and the part that is invisible.

## Cause

`compile-check-signature.sh` mixes the row's own source tree with the closure
the build MEASURED (`nros_dep_closure_manifest`, phase-360 W4 — cargo's dep-info,
so a row that compiles against workspace crates notices when those change).

`packages/cli/nros-cli-core/build.rs` declares, deliberately:

```rust
// …and when the index moves (commit, rebase, branch switch), since the
// stamp reads index blob SHAs.
let index = root.join(".git/index");
```

That is right for the CLI: its `NROS_CLI_SOURCE_STAMP` reads index blob SHAs, so
cargo must rebuild it when the index moves. But any fixture whose closure reaches
that crate inherits `.git/index` as a signature input — and the index is
rewritten by `git add`, `git reset`, `git commit`, `git rebase`, and by a plain
`git status` that refreshes stat data. `.git/modules/**/HEAD` came along too.

So the signature moves with no source change whatsoever. Measured directly: a
`git add` of one unchanged file followed by `git reset` moves
`board_agnostic_run_plan`'s signature.

The consequence is worse than a slow lane. **The fixture can never be fresh for
anyone who commits**: the build stamps a signature, you commit, the stamp is
already wrong. A fresh clone that builds and immediately tests is green, which is
why CI never saw it — the same asymmetry as issue 0624, from a different
direction.

Why the existing filter missed it: the extractor already drops gitignored paths,
precisely to keep build output out of a signature. `.git/` is not ignored — it is
outside the working tree git reports on at all — so that filter could never have
covered this.

## Fix

`dep-closure.py` drops any path whose first component is `.git`. VCS state is not
a build input, whatever a depfile says; the CLI's own rebuild edge is unaffected,
because that is cargo's fingerprint and not this signature.

One site: the compile-check lane is the only consumer of the closure. The
workspace lane's signature enumerates tracked files (`nros_source_manifest`) and
was never exposed — checked, rather than assumed.

## Verification

* `scripts/check-source-manifest.sh` gained both directions: a listed `.git/`
  path must not reach the closure, and excluding it must not drop the real deps
  beside it. Mutation: neutering the exclusion fails the first, passes the
  second, so the assertion is load-bearing.
* End to end: recompute `board_agnostic_run_plan`'s signature, `git add` +
  `git reset` an unchanged file, recompute — identical after the fix, different
  before it.

## What this cost, and the transferable part

Four rebuilds of the native fixture lane (~7 minutes each) chasing a "stale"
verdict that no rebuild could clear, plus the misreading it invites: three rows
stale after a successful build reads as a broken build, not as a broken
signature.

The rule worth keeping: **a signature input must be something a source change
moves and nothing else moves.** A depfile is evidence of what a build read, not
of what a build DEPENDS on — a build script may watch a file for its own
purposes, and inheriting that watch into a signature imports its instability.
