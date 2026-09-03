---
id: 999
title: "`nros build` preflight asked rustup about a `build-std` target, so every
  nuttx build failed with a remedy that cannot work"
status: resolved
type: bug
area: cli, build
severity: high
found: 2026-09-03
related: [issue-0968, issue-0998, issue-0833]
---

## Symptom

`just build-test-fixtures lane=tier2`, the `nuttx` module, building a workspace
fixture:

```
  -> workspace-c-nuttx (c) examples/workspaces/c
     nros build demo_bringup:nuttx --workspace . --offline -- ...
Error: missing prerequisites for this build:
  - Rust target `armv7a-nuttx-eabihf` (board `nuttx`)
      run: rustup target add armv7a-nuttx-eabihf
```

The remedy cannot work:

```
$ rustup target list | grep -c armv7a-nuttx
0
```

## Cause

`builder/preflight.rs` asked one question — "does `rustup target list
--installed` name this triple?" — and printed one remedy for every no.

`armv7a-nuttx-eabihf` is Tier 3 / custom-JSON. rustc does not DISTRIBUTE it, so
`rustup target list --installed` can never name it and `rustup target add` can
never install it. The check reported it missing on every host, including a fully
provisioned one, and sent the reader to a command that fails. `-Z build-std`
compiles core/alloc from source; the prerequisite is `rust-src`, not a target.

The tree already said so, twice:

* `config/rust-targets.txt:43` — `armv7a-nuttx-eabihf   build-std`
* `scripts/lib/rust-targets.sh:10` — of that column: *"Tier 3 / custom-JSON
  targets, nothing to install"*

**Issue 0833's class**: a second idea of what the target list means, held
somewhere that does not read the list. Which is precisely why the set is DATA in
`config/rust-targets.txt`, and why `just/workspace.just` carries the comment
"read from config/rust-targets.txt, NOT a second copy of the list".

## Fix

`target_provisioning(root, target)` reads `config/rust-targets.txt` and returns
`Provisioning::BuildStd` or `Provisioning::Rustup`; preflight branches on it and
probes `rust-src` for the build-std case, with `rustup component add rust-src`
as the remedy.

Landed on the `fix/0998-sertype-freestanding` branch alongside the #0998 work.

**Two independent fixes existed briefly, and the other one is better.** I wrote
one that branched on the board descriptor's `Toolchain::Nightly`; this one reads
the target list itself. Reading the DATA is the more faithful answer to a defect
whose cause is "a second idea of what the list says", so mine was dropped rather
than merged. Recorded because the reasoning is the useful part: when the bug is
a duplicated fact, prefer the fix that consults the fact.

One caveat worth knowing: `target_provisioning` resolves
`config/rust-targets.txt` relative to the WORKSPACE root, so an out-of-tree
workspace finds no file and falls back to `Provisioning::Rustup` — the old
behaviour. In-tree that is always right; out-of-tree a nuttx board would see
the dead-end remedy again.

## How it was found

Trying to reproduce issue 0968. The tier-2 lane could not build its nuttx
fixtures at all, so nothing downstream of them had run — the same shape as
0998, in the same sweep: a lane nobody runs accumulates blockers, and each is
invisible because the lane is already red for an earlier reason every time
anyone looks.

## Acceptance

* [x] A `build-std` board is never told to `rustup target add`.
* [x] The remedy for a build-std board is one that works.
* [ ] Out-of-tree workspaces resolve the target list too — see the caveat.
