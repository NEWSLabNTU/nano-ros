---
id: 386
title: "The `--locked` cargo shim breaks the book's first node on a fresh clone — leaf `Cargo.lock` is gitignored, so cargo may not create it"
status: resolved
type: bug
area: build
related: [issue-0359, issue-0378, issue-0373, issue-0384, rfc-0048]
---

## Resolution (2026-08-02)

`scripts/bin/cargo` now skips `--locked` injection when the target's
`Cargo.lock` is git-IGNORED — a regenerable local artifact, not a tracked
promise. It resolves the manifest dir (`--manifest-path` if given, else cwd)
and, when `git -C <dir> check-ignore -q Cargo.lock` succeeds, execs the real
cargo with no injected flag. This covers BOTH failure modes: `--locked`
forbidding the *creation* of a first lock and forbidding the *update* of a stale
one after `nros sync` changes a leaf's deps. The repo's own workspaces (tracked
locks) are unaffected — `check-ignore` returns non-zero, so `--locked` still
applies; a non-git dir also falls through to injection. Verified via `bash -x`:
example leaf (cwd or `--manifest-path`) → no `--locked`; repo root → `--locked`
injected. Sibling of issue 0384 (both in the shim).
---

# `--locked` blocks the fresh-clone example build

## Symptom

The book's documented first-node sequence
(`book/src/getting-started/first-node-rust.md`, the `probe=30` block) fails on a
clean checkout:

```
$ cd examples/native/rust/talker
$ nros sync            # ok
$ cargo build
    Updating crates.io index
error: cannot create the lock file /…/examples/native/rust/talker/Cargo.lock
       because --locked was passed to prevent this
```

Reproduced by deleting the leaf lock and rebuilding; with an out-of-date lock
the same command fails with `cannot update the lock file` instead.

## Cause

Two correct-in-isolation decisions collide:

1. `examples/native/rust/talker/.gitignore:10` ignores `/Cargo.lock` — example
   leaf locks are local artifacts, not tracked.
2. `activate.sh` now exports `NROS_CARGO_FLAGS=--locked` and puts
   `scripts/bin/cargo` on PATH, so every `cargo` invocation in an activated
   shell carries `--locked` (issues 0359/0378 — "`Cargo.lock` only means
   something if builds REFUSE to rewrite it").

A user who has never built the example has no lock, and `--locked` forbids
*creating* one. The flag is right for the repo's own workspaces, whose locks are
tracked; it is wrong for a leaf whose lock is deliberately untracked and
generated on first build.

Note the shim reaches further than a `just` recipe would: it is on PATH, so it
also applies to the plain `cargo build` the BOOK tells the reader to run, and to
cmake/corrosion invocations that call `cargo` by name. That reach is the
feature; this is its blast radius.

Distinct from issue 0384, which is a different defect in the same shim (it
appended `--locked` at the argv TAIL, so `cargo <sub> -- <args>` leaked the flag
to the child process). That one is fixed; this one is about the flag being
applied at all where no lock is tracked.

## Impact

Every new user following the getting-started page in an activated shell, on any
platform. The escape hatch exists (`NROS_CARGO_FLAGS= cargo build`, or
`just lock-update <crate>`) but is documented for a different purpose —
deliberate dependency changes — and the error message names neither.

## Direction

Options, roughly in order of preference:

1. **Scope the flag to tracked locks.** Have `scripts/bin/cargo` drop `--locked`
   when the target manifest's `Cargo.lock` is absent or gitignored — the
   invariant "locks change only on purpose" only has meaning where the lock is
   under version control.
2. **Have `nros sync` generate the leaf lock** (`cargo generate-lockfile`) as
   part of the sync it already performs, so the first build always finds one.
   Sync already writes `generated/` and the patch table; the lock belongs to the
   same generated set.
3. **Track the example locks** and drop them from `.gitignore`. Consistent with
   the flag, but adds churn on every dependency bump across ~16 leaves — which
   is what issue 0359 was trying to reduce.
4. Failing all of the above, at minimum teach `scripts/bin/cargo` to catch the
   error and print the remedy.

Whichever lands, add the fresh-clone case to the probe: `just probe bootstrap`
runs the book's `probe=30` block on a machine with no prior build, so it should
have caught this — verify it now does.

## Evidence

Arch Linux, checkout at `c0aad42d8` (after the shim landed in `d3adb8df6` /
`ae00901cf`), reproduced on both the host and inside an Ubuntu 22.04 distrobox.
The lock's content is *not* the problem: after one permissive build the tree is
consistent and `--locked` succeeds; the failure is that the first build in a
fresh tree is never permitted to happen.
