---
id: 1866
title: "A fixture's cmake build tree (`build-<rmw>/`) was ignored only by per-leaf `.gitignore` lines, so a leaf without one left it untracked"
status: resolved
type: bug
area: [build, tests]
severity: low
found: 2026-10-13
resolved: 2026-10-13
related: [1845, 1305, phase-445, phase-340]
---

## Measured

After the native C++ fixture build in issue 1305 phase F,
`packages/testing/nros-tests/bins/contract-monitor-cpp/build-zenoh/` sat
untracked and unignored. Issue 1845 (`b766d64c9e`) fixed that one leaf by
giving it a `.gitignore`, the way every other cmake leaf had one, so the next
leaf without one would repeat it.

## Cause

The fixture harness configures a cmake row into `<dir>/build-<rmw>/`
(`Rmw::build_dir()`), and other roads into `<dir>/build/`. The repo root
ignored `examples/**/build/` and `examples/**/target-*/`, but no
`build-<rmw>` name and nothing under `packages/testing/`. Coverage was a
per-leaf line each new leaf had to remember.

## Fix

- Root `.gitignore`: one rule per harness build-dir name, under `examples/`
  and `packages/testing/` (`build/` there too). They hide no tracked file
  (`git ls-files -ci --exclude-standard`: 26 before and after, all
  pre-existing `.cargo` configs).
- Gate `check fixture-build-dirs-ignored`: reads the names from
  `Rmw::build_dir()` (plus `build`) and the dirs from every `dir =` row of
  `examples/fixtures.toml`, and asks `git check-ignore --no-index` about a
  path inside each — git's own answer, root and per-leaf rules alike. The
  selftest (run on every call) parses a planted `build_dir()` and reports a
  planted uncovered dir.

## Evidence

- Without the new root rules: rc=1, 392 uncovered `<dir>/<name>` pairs
  (e.g. every `examples/mps2-an385-baremetal/c/*/build-{zenoh,xrce,cyclonedds}`).
- With them: rc=0, 276 fixture dirs × 4 names.
- The main checkout, after months of every lane, shows no untracked build
  output under these roots.
