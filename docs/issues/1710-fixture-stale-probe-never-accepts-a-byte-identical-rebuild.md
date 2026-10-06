---
id: 1710
title: "A format-only edit to nros-c / nros-cpp leaves every native C/C++ fixture
  STALE forever — the build converges on a byte-identical archive and never
  relinks, and the probe compares source mtimes"
status: open
type: bug
area: [testing, build]
severity: medium
found: 2026-10-06
related: [1686, 0196, 0445, phase-480]
---

## What was measured

While fixing issue 1686, `rustfmt` reflowed two lines of
`packages/api/nros-cpp/src/action.rs` (whitespace only) after the native C/C++
fixtures had been built. Then, on `examples/native/c/service-server/build-xrce`:

- `scripts/build/fixtures-build.sh linux c xrce` ran to `rc=0`. Cargo rebuilt
  `cargo/.../libnros_cpp.a` (mtime 08:40).
- The copy cmake links, `nano_ros/packages/api/nros-cpp/libnros_cpp.a`, kept its
  08:14 mtime, and `cmp` says the two archives are **byte-identical** — the
  copy step is write-if-different, correctly.
- So nothing relinked: `c_service_server` stayed at 08:14, which is a CORRECT
  binary for the current source.
- `native_example_reqresp_e2e` then failed every C/C++ cell with

      Test fixture is STALE — a source is newer than the built binary:
        binary: .../examples/native/c/service-server/build-xrce/c_service_server
        newer:  .../packages/api/nros-cpp/src/action.rs

Re-running the fixture build does not change anything, so the state is
ABSORBING: no build the harness can ask for makes these fixtures pass the probe.
The only way out was to `touch` the copied archives so ninja relinks
(`tmp/relink.sh` in the 1686 session), which launders the probe rather than
answering it.

## Why

The build side and the probe disagree about what "up to date" means. The build
uses content (cargo fingerprints, then a write-if-different copy, then ninja
mtimes on the COPY), so an edit that changes no object code legitimately ends
with an old binary. The probe uses mtimes of the SOURCES against the binary, so
the same edit is stale forever. Issue 0196's rule — a build-side probe and a
test-side gate must watch the same inputs — has the mirror-image form here:
they must also agree on what counts as a change.

## Shape of a fix (not attempted)

Either the probe judges the binary against what the binary was LINKED FROM (the
copied archives' mtimes, which only move on a content change), or the build
records a stamp that moves on every successful build of the leaf whatever the
outcome, and the probe compares against the stamp. The first keeps the probe
honest about content; the second is the fixtures-built-stamp pattern this repo
already uses elsewhere.

## Acceptance

After a whitespace-only edit to an `nros-c` / `nros-cpp` source and one
`fixtures-build.sh` run, the native C/C++ fixtures pass the staleness probe
without any `touch`.
