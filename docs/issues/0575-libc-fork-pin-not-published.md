---
id: 575
title: "The NuttX `libc` fork is pinned at a commit that exists only on this machine — a fresh clone cannot check it out"
status: open
type: bug
area: build
related: [issue-0167, issue-0570]
---

## Symptom

`git submodule status` pins `third-party/nuttx/libc` at `adb4c59`. After a
`git fetch` in that submodule, `origin/main` is `2aa834e` (upstream libc) and

```
$ git -C third-party/nuttx/libc branch -r --contains adb4c59
<nothing>
```

`adb4c59` is #167's `--wrap=poll` ABI shim. It was committed locally, the
superproject pointer was moved to it, and it was never pushed to
`github.com/jerry73204/libc`. So the pin names an object no other clone can
obtain: `git submodule update --init third-party/nuttx/libc` fails on a fresh
checkout, and with it every NuttX **Rust** row (the fork is the `[patch.crates-io]
libc` that `-Z build-std` needs).

## Why it went unnoticed

The rule in CLAUDE.md is push the fork branch FIRST, then bump the superproject
pointer — precisely to stop this. It went the other way here, and the only host
that could tell is one that has never had the commit. This machine has it, so
every build since has been green for a reason that does not survive a clone.

The same shape as the `generated/`-plus-`Cargo.lock` trap (a tree that builds
here and cannot build anywhere else) and it is invisible to every existing gate:
`check-submodule-drift` compares the pin to the CHECKOUT, not to what the remote
can serve.

## Scope

Two commits are affected, both local-only:

* `adb4c59` — poll() ABI shim for NuttX's 24-byte `struct pollfd` (#167);
* the `__PTHREAD_ATTR_SIZE__` fix on top of it (#570).

Other vendored forks (cyclonedds, netxduo, zenoh) should be checked for the same
thing rather than assumed clean — this gate does not exist for any of them.

## Fix

1. Maintainer pushes both commits to the fork remote, then the superproject
   pointer moves to the pushed commit (agent does not push fork remotes).
2. A gate: for every submodule whose URL is a fork we control, assert the pinned
   commit is contained in some remote branch (`git branch -r --contains`, after a
   fetch). That is the check `check-submodule-drift` cannot make, because it asks
   a question about the WORKING TREE and this is a question about the REMOTE.

## Acceptance

* `git branch -r --contains <pin>` is non-empty for every fork submodule;
* a clone into an empty directory runs `git submodule update --init` for the
  NuttX libc fork without error — verified by cloning, not by reasoning.
