---
id: 390
title: "`nros setup <board> --rmw <x>` provisions one board's sources, but the build stage needs the UNION — and the failures name no remedy"
status: open
type: bug
area: build
related: [issue-0368, issue-0373, issue-0378, issue-0388, rfc-0014]
---

# Setup is scoped per board; the build stage needs every board's sources

## Symptom

A host set up exactly as the book documents —
`nros setup native --rmw zenoh`, the `native` board, one RMW — cannot run the
build stage. Two failures, hit in sequence on a real Arch host today, each after
the previous one was fixed:

**1. `just test` (workspace build) needs the XRCE submodule:**

```
error: failed to run custom build command for `nros-rmw-xrce-cffi v0.5.0`
  nros-rmw-xrce-cffi: vendored `micro-xrce-dds-client` source root
  …/packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/src/c is missing or has no
  .c files — submodule not initialised or upstream layout drifted.
  Fix: git submodule update --init packages/rmw/xrce/xrce-sys/micro-xrce-dds-client
```

**2. `just build-test-fixtures` needs the NuttX libc submodule** — while
refreshing metadata for a NATIVE component:

```
error: failed to load source for dependency `libc`
Caused by: unable to update /…/third-party/nuttx/libc
Caused by: failed to read /…/third-party/nuttx/libc/Cargo.toml
Caused by: No such file or directory (os error 2)
Error: refresh source metadata for `action_client_pkg`
Caused by: metadata-mode harness failed (exit 101) for component 'fibonacci_client'
```

Each was one command to fix once identified — `nros setup native --rmw xrce`,
`nros setup --source nuttx-libc` — which is the point: the sources are in the
index and provisioning them is trivial. The defect is that nothing tells you to.

## The gap

RFC-0014's model is per-board provisioning: `[board.<name>].packages` unions with
`[rmw.<name>].packages`, and `nros setup <board> --rmw <x>` fetches exactly that
set. That is right for *building an application* for one board.

But the repo's own build stage is not per-board:

- `just test` / `test-unit` build `--workspace`, which includes every RMW's
  `-sys` crate, so every RMW's vendored C must be present regardless of which
  RMW you provisioned.
- `just build-test-fixtures` refreshes metadata for every component in every
  example workspace, so it resolves dependency graphs that name platform crates
  (`third-party/nuttx/libc`) even when the component being refreshed is native.

So the contributor path needs the UNION of the index's source packages, while
the documented setup command gives one slice. `just setup all` presumably
approximates the union, but the book's contributor section presents it as
optional convenience ("Provision one module: `just freertos setup`"), and
issue 0368 documented that `just setup all` itself fails 7 of 18 modules on a
clean host — so following it is not currently a reliable path either.

## Why it reads as a bug rather than a papercut

Neither failure names the remedy in the vocabulary the user has. The XRCE one is
good — it prints a `git submodule update --init` line — but it names git rather
than `nros setup --rmw xrce`, so a user who provisions through the CLI learns
that the CLI was not the whole story. The NuttX one is a raw cargo
dependency-resolution error four `Caused by:` layers deep, pointing at a path
that does not exist, with no mention of setup, sources, or the index at all.

That matters most for the exact population it hits: someone on a non-Ubuntu host
following the book for the first time, who cannot tell an unprovisioned tree from
a broken one.

## Direction

1. **Declare the build stage's requirement.** A `[profile.contributor]`-style
   package set in `nros-sdk-index.toml` (or a `--all-sources` flag) that
   provisions every `[source.*]` the workspace build touches, with
   `just build-test-fixtures` / `just test` checking it the way
   `_require-fixtures` checks fixtures.
2. **Make the failures name the remedy.** A missing vendored source should say
   `run: nros setup --source <name>` — the index already knows the mapping from
   path to package name, so the message can be generated rather than hand-written
   per build script.
3. **Say it in the book.** The contributor section should state that the test and
   fixture stages need the union, not one board.

Related but distinct: issue 0388 was the same shape one layer down (a test
hardcoding `build/zenohd` instead of resolving the setup-provided copy), and
0368 F1 is the same shape in `just setup all` (one sudo step aborting the
sudo-less installers behind it).

## Evidence

Arch Linux, 2026-08-02, checkout `67df8975e`. Host provisioned with
`nros setup native --rmw zenoh` + `--rmw cyclonedds`. `just test` failed on
XRCE; after `nros setup native --rmw xrce`, `just build-test-fixtures` failed on
`third-party/nuttx/libc`; after `nros setup --source nuttx-libc`, the stage
proceeded.
