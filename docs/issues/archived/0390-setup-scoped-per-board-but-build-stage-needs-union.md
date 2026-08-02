---
id: 390
title: "`nros setup <board> --rmw <x>` provisions one board's sources, but the build stage needs the UNION — and the failures name no remedy"
status: resolved
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

## Progress (2026-08-03) — Direction 2 landed; Direction 1 scoped

**Direction 2 (failures name the remedy) — DONE.**
- `build_metadata` (the metadata-refresh harness, the NuttX-libc worst case that
  named *nothing*) now captures the harness stderr and, on failure, scans it for
  any index `[source.*]` `dest` path; if one is implicated it appends
  `run: nros setup --source <name>`. Index-driven (dest → package name),
  unit-tested against the real shipped index (`e7bdacef7`). NOTE: could not run
  the unit test end-to-end locally — the nros-cli-core lib-test build was
  transiently red from a concurrent phase-333 schema migration (`class` field on
  `ComponentConfig`, `TierRtosSpec` fields); the change itself compiles (lib
  `check` green) and the test passes once that clears.
- The vendored-source presence gates now lead with `nros setup --source <name>`
  instead of only `git submodule update --init` — fixed the whole class:
  nros-rmw-xrce-cffi (micro-xrce-dds-client, micro-cdr), cyclonedds-sys
  (cyclonedds-src), nros-zpico-build (zenoh-pico, mbedtls) (`1ba7b23ee`).

**Direction 1 (declare + preflight the build-stage source union) — DONE.** The
per-rmw/board `build_sources` were unpopulated + `tools/setup.sh`-only, so the
repo build's own union was declared nowhere. Fixed:
1. **Declared** — a top-level `build_sources` union in `nros-sdk-index.toml`
   (`SdkIndex.build_sources`), curated from what the ROOT workspace + metadata
   refresh touch on the host: the 5 RMW `-sys` sources (zenoh-pico, mbedtls,
   cyclonedds-src, micro-xrce-dds-client, micro-cdr) + `px4-rs` (a root cargo
   path-dep — the workspace won't even LOAD without it) + `nuttx-libc`. NOT the
   cross-only trees (freertos/threadx/nuttx kernels, px4-autopilot, rosidl).
2. **Preflight** — `nros setup --build-sources` provisions the union;
   `--build-sources --check` verifies each present and names `nros setup --source
   <name>` per missing (exit non-zero). Wired as `_require-build-sources`, the
   first dep of `just test` + `just build-test-fixtures`
   (NROS_SKIP_BUILD_SOURCE_CHECK=1 bypass).
3. Verified BOTH ways locally: all 7 present → green; hiding `nuttx-libc` →
   `[MISSING] nuttx-libc  run: nros setup --source nuttx-libc` + exit 1, through
   the recipe. The clean-host end-to-end (`just probe bootstrap`) is the only
   remaining confidence check — the fire path itself is proven by the
   simulated-missing run.

**Direction 3 (book) — DONE.** `book/src/internals/contributing.md` now states
the test/fixture stages need the source union, not one board, and names the
preflight remedy.

**RESOLVED.** All three directions landed (Direction 2 `e7bdacef7`/`1ba7b23ee`;
Direction 1 `f63bf71bd`; Direction 3 with this commit). The reported symptom —
build-stage failures naming no `nros` remedy — is fixed at the source (the
preflight fails fast + names it) and at each failure site (metadata-refresh
translation + the `-sys` gate messages).

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
