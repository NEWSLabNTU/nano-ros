---
id: 1488
title: "`zephyr_self_pkg`'s bringup dirs have no `package.xml`, so the `nros
  sync` the west fixture builder runs there is refused outright — and the
  configure that needs its SystemModel then fails"
status: open
type: bug
area: testing, build, cli
severity: high
related: [1312, 0510, phase-330, phase-445]
---

## Symptom

`just build-test-fixtures lane=all` fails in the zephyr module:

```
   nros sync failed in alpha_pkg (configure may fail)
...
CMake Error at zephyr/cmake/nros_system_generate.cmake:316 (message):
  nros codegen-system failed (rc=1):
  Error: codegen-system:
  .../fixtures/zephyr_self_pkg/self/alpha_pkg/system.toml declares system
  semantics but no SystemModel was found. It is a BUILD ARTIFACT
  (phase-330 W4), so generate it rather than committing one:
      nros sync      # writes <ws>/build/nros/models/<bringup>/
   MISSING nros-system/system_config.h for zephyr_self_pkg_rust
```

Two targets, `zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling`. The module
builds first in `lane=all`, so this starves every module behind it.

## Cause

`scripts/build/west-fixtures.sh:222-228` runs `nros sync` in every immediate
subdirectory holding a `system.toml`, and swallows the failure:

```sh
( cd "$_bringup" && "$_wf_cli" sync >/dev/null 2>&1 ) \
    || echo "   nros sync failed in $(basename "$_bringup") (configure may fail)" >&2
```

Run by hand, the failure is structural:

```
$ cd packages/testing/nros-tests/fixtures/zephyr_self_pkg/self/alpha_pkg && nros sync
Error: sync: no `src/<pkg>/package.xml` and no `package.xml` at root under
  .../zephyr_self_pkg/self/alpha_pkg — expected colcon-style workspace or single-pkg dir
```

`alpha_pkg` holds `Cargo.toml`, `CMakeLists.txt`, `prj.conf`, `src/` and
`system.toml` — and **no `package.xml`**, in either the `self/` or the
`sibling/` copy.

The script's own comment says the author knew:

> Sync runs INSIDE the bringup dir, not the workspace root: these fixtures keep
> their packages at the root rather than under `src/`, which `nros sync` rejects
> outright

So sync is invoked where it is known to be rejected, its failure is downgraded
to a warning, and the configure that consumes its output fails a few lines
later with a message about a missing SystemModel rather than about sync. The
`|| echo` is the 0510 masking shape the comment above it cites.

## Not caused by phase-456

`git diff --name-only origin/main...work/phase-456-rebased` touches
`packages/testing/nros-tests/fixtures/` and `zephyr/` not at all, and its only
`packages/cli/` changes are `sizing_descriptor.rs` (issue 1319's row rename) and
a board-key test — none of them in workspace discovery. The fixture arrived with
phase-445 W5 (2026-09-11) and was last touched by issue 1312 (2026-09-17); the
branch post-dates both.

**What is NOT established** is whether this ever worked. `lane=all` has not
completed on this machine in weeks, so "it broke" and "it was never exercised"
are both consistent with the evidence. Deciding that needs either a CI run that
included these targets or a build at `379f7c8d2`.

## What would fix it

Either the fixture gains the `package.xml` its layout implies — a single-package
dir is one of the two shapes sync accepts, and it already has everything else —
or the builder stops running sync where it cannot work and the configure gets
its model another way. The first is a one-file change and matches what
`_nros_system_detect_self_pkg` already resolves.

Whichever: **the failure must stop being swallowed.** A step whose output the
very next step requires is not optional, and the `|| echo` turns a one-line
cause into a five-line symptom about a build artifact.

## Acceptance

* `nros sync` succeeds in both `zephyr_self_pkg` bringup dirs, or is not run
  there.
* The west fixture builder fails loudly when a sync it depends on fails.
* `zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling` configure.
* Stated either way: whether these targets had ever been built successfully.
