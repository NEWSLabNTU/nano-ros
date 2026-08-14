---
id: 575
title: "REFUTED: the NuttX `libc` pin was published all along — `git branch -r --contains` is not a publication test in a shallow, single-refspec submodule"
status: resolved
type: bug
area: build
related: [issue-0167, issue-0570]
---

## What this was filed as

That the superproject pinned `third-party/nuttx/libc` at `adb4c59` (#167's
`--wrap=poll` shim) and **no remote branch contained it**, so a fresh clone could
not resolve the pin and every NuttX Rust row was unbuildable anywhere but this
machine.

The evidence was:

```
$ git -C third-party/nuttx/libc fetch origin
$ git -C third-party/nuttx/libc branch -r --contains adb4c59
<nothing>
$ git -C third-party/nuttx/libc rev-parse origin/main
2aa834ea…      # upstream, not the pin
```

## Why that was wrong

Both facts were artifacts of how THIS checkout of the submodule is configured,
not of the remote.

1. **The clone is shallow.** `git rev-parse --is-shallow-repository` -> `true`,
   with two grafted roots in `.git/modules/.../shallow`. Every commit at the
   boundary reports `parent=` (empty), so ancestry queries answer about a
   truncated graph. `git merge-base --is-ancestor origin/main nuttx-abi`
   returned "no" while `git log --graph` drew a straight line — the two
   disagreed because one root was a graft.
2. **The fetch refspec was single-branch.** `remote.origin.fetch` was
   `+refs/heads/main:refs/remotes/origin/main`, so `git fetch` created exactly
   one remote-tracking ref and `--contains` could only search `origin/main`.
   Nothing else was ever visible to it, published or not.

Ask the remote instead of the local refs and the answer inverts:

```
$ git ls-remote origin | grep nuttx
adb4c592e…  refs/heads/nuttx-0.2      # the pin, published, since 2026-07-11
```

`origin/nuttx-0.2` is where this lineage lives. #167's commit was pushed at the
time, exactly as the vendored-fork workflow requires. There was no unpublished
pin and no broken clone.

## What was actually done

`826c4ca9` (#570's `__PTHREAD_ATTR_SIZE__` fix) fast-forwarded `nuttx-0.2` from
`adb4c592e`, which is the one-commit fast-forward that branch was waiting for.
The superproject pin already names `826c4ca9`, so pin and remote agree.

A `main` fast-forward was NOT possible and must not be attempted: `origin/main`
tracks upstream rust-lang/libc and diverged from this lineage at
`72093f38f` (2024-01-07) — 2140 commits on main, 1252 on `nuttx-0.2`. Forcing
main to the NuttX branch would rewind the fork by two years. The NuttX work
belongs on `nuttx-0.2` and only there.

## The durable lesson

**`git branch -r --contains <sha>` is not a publication test.** It answers "is
this commit reachable from a remote-tracking ref I happen to have", which in a
shallow or narrow-refspec clone — the normal state of a provisioned submodule —
is a much smaller question than "does the remote have it". The two look
identical when the answer is empty, and the empty answer reads as alarming.

Use `git ls-remote <remote>` for the publication question. It talks to the
server and does not care what the local clone fetched or how deep it is.

Corollary for a would-be gate: the "is every fork pin published" check that this
issue proposed is still a reasonable thing to want, but it must be built on
`ls-remote`, not `--contains`, or it will fail loudly on every correctly-pinned
shallow submodule.

## Status

Resolved — refuted. `.gitmodules` sets neither `branch` nor `shallow` for this
submodule, so a fresh `git submodule update --init` clones with the default
refspec, fetches every branch, and resolves the pin. The shallow single-branch
configuration is local to this machine's provisioning, not something the
repository imposes.

The commit message of `3b8981ad3` repeats this issue's original claim ("pinned at
a commit no remote branch contains") and is wrong on that point; the fix it
describes is unaffected.
