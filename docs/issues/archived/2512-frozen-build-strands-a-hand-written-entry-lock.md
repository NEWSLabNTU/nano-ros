---
id: 2512
title: "A dependency bump on main strands the untracked lock of a hand-written entry, and `nros build --offline` refuses it without saying to delete or update it"
status: resolved
type: bug
severity: low
area: [cli, build, cargo, testing]
found: 2026-11-14
related: []
---

## What

Measured on 2026-11-14 while building the NuttX fixtures (`just nuttx
build-fixtures`) on a worktree that had built them before main bumped
`ros-launch-manifest` from tag `v0.3.14` to `v0.3.15` (the rlm and
play_launch update).

Generated entries re-resolved, and their builds printed
`Removing … v0.3.14 … Adding … v0.3.15`. One hand-written entry did not:
the logging-smoke test image
(`packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/src/logging-smoke-nuttx-qemu-arm/`).
Its build failed with:

```
error: cannot update the lock file …/logging-smoke-nuttx-qemu-arm/Cargo.lock because --frozen was passed to prevent this
help: to generate the lock file without accessing the network, remove the --frozen flag and use --offline instead.
nros build: `RUSTUP_TOOLCHAIN=nightly-2026-04-11 cargo build --manifest-path src/logging-smoke-nuttx-qemu-arm/Cargo.toml … --frozen …` exited 101
  !! nuttx-logging-smoke: nros build failed (rc=101); the row's artifact is NOT current (issue 1961)
```

That stopped `build-fixtures-arm`, so the riscv half never ran, and every
NuttX C/C++ test then ran against stale images.

## Why

The lock is untracked: it was seeded from the registry pin set by an
earlier build, and it still names `v0.3.14`. `nros build` treats a lock on
a HAND-WRITTEN entry that already names its package as the user's, so it
runs no `cargo update --workspace` prepare step (that step runs only for a
generated entry, or for an entry with no lock). `--offline` becomes
`--frozen`, which forbids moving the lock. Nothing else moves it.

So any dependency bump on main breaks every such leftover lock. The
refusal is cargo's own text. It suggests dropping `--frozen`, which the
user does not pass, and names neither of the two real remedies:

- delete the lock, so it is re-seeded from the pin set;
- `nros update` (the sanctioned way to move a lock).

## Repro

1. Build a hand-written NuttX entry once, offline:
   `just nuttx build-fixtures`. This leaves an untracked
   `Cargo.lock` beside the logging-smoke entry.
2. Advance main past a commit that moves a git-tag dependency in the
   registry pin set (here `ros-launch-manifest` `v0.3.14` → `v0.3.15`),
   and rebase or check it out.
3. `just nuttx build-fixtures` again. The logging-smoke row fails as
   above.

Deleting the untracked lock clears it: the next build re-seeds it.

## Fix (2026-11-14)

Two halves, split on the one question that decides whether `--frozen`
protects anything: is the lock a promise?

- **A lock is a promise exactly when it is tracked.** In a checkout that is
  a `[rust] locked_roots` row of the index (RFC-0104 D3). Every other lock
  in a checkout was seeded from the pin set by an earlier build: it is build
  output. `nros build` now treats such a lock like a generated entry's
  (`lock_is_seeded_output` in `cmd/build.rs`). It runs the prepare step,
  `cargo update --workspace` (`--offline` when the build is), before the
  `--frozen` build on the workspace cargo road, where the repro's entry
  builds. That moves only what the manifests no longer accept, and keeps
  every other resolution. A standalone leaf is left as it was: `nros
  configure` already seeds and prunes its lock (ruling R10), and a test
  holds that a configured leaf gets no second prune. A stale leaf lock gets
  the refusal below.
- **A tracked lock, and any lock outside a checkout, is left alone.**
  `--frozen` still refuses to move it. When a `--frozen` cargo build fails,
  `nros build` asks `cargo metadata --locked --offline` with the build's own
  manifest, settings and environment whether the lock is the cause. If it
  is, it prints which lock no longer matches its manifests and the two
  remedies: `nros update` (or `cargo update --workspace`) to keep it, or
  delete it to re-seed it.

Measured on the repro, with the lock edited back to `v0.3.14`:

- The pre-fix CLI fails with cargo's refusal alone.
- With the fix, the untracked lock is re-resolved offline (`Removing …
  v0.3.14` / `Adding … v0.3.15`, five crates) and the build succeeds.
- With the same lock listed as a locked root, the build still fails, the
  lock is untouched (still `v0.3.14`), and the new refusal names it and both
  remedies.
