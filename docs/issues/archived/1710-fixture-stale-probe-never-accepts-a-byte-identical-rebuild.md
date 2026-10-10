---
id: 1710
title: "A format-only edit to nros-c / nros-cpp leaves every native C/C++ fixture
  STALE forever — the build converges on a byte-identical archive and never
  relinks, and the probe compares source mtimes"
status: resolved
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

## Resolution

Fixed on `fix/1710-byte-identical-relink` (commit "a byte-identical cargo
rebuild the binary links answers its inputs").

**Cause, confirmed.** The content arm (issue 0764) adjudicates an mtime that
moved against a byte hash, which clears a git-induced mtime bump but not a
format-only edit: the source's bytes really did change, so it reports STALE.
The build had already answered the question by content one level down —
cargo rebuilt the staticlib, `copy_if_different` found it identical, ninja did
not relink — and the probe never asked the build.

**Fix: the issue's first shape — judge against what the binary was LINKED
FROM.** `cargo_unit_link_cover` (`nros-tests/src/fixtures/binaries/mod.rs`),
called per cargo unit `.d` in `cargo_rust_inputs`, answers that unit's inputs
when all three hold:

1. cargo's output (`<stem>.a` beside `<stem>.d`) is at least as new as every
   input the `.d` lists — cargo ran after the last edit;
2. every copy of that archive in the cmake build dir (found by name, never
   under `cargo/`, `corrosion/`, `CMakeFiles/`, and never the original itself)
   is byte-identical to it;
3. the binary is at least as new as every such copy.

Anything short of all three falls through to the old arm unchanged, and no copy
found is "not shown", never "covered". Covered inputs are counted on the
probe's accounting line (`staleness::note_link_covered`), not dropped silently.

**Measured (2026-10-11, this worktree, `fixtures-build.sh linux c xrce`).**
After a fresh build, one trailing space on line 1 of
`packages/api/nros-cpp/src/action.rs` (02:24:51) and a second
`fixtures-build.sh linux c xrce` (rc 0): cargo's `libnros_cpp.a` 02:24:54, the
linked copy `nano_ros/packages/api/nros-cpp/libnros_cpp.a` kept 02:24:16 and
`cmp` says IDENTICAL, `c_service_server` kept 02:24:19.

- Old probe: `test_c_xrce_service_request_response` FAILS with `Test fixture is
  STALE — a source is newer than the built binary … newer: …/nros-cpp/src/`.
- Fixed probe, same tree, no `touch`: PASS (3.7 s, the live request/response).

Test: `a_byte_identical_cargo_rebuild_the_binary_links_answers_its_inputs`, a
hermetic corrosion layout with one case per condition. It FAILS with the cover
disabled (mutation measured).

Sweep: `git grep -n "cargo_unit_dep_files\|cargo_rust_inputs" -- packages/testing/nros-tests/src`
— the cmake arm is the one caller; the pure-cargo and Zephyr arms link no
copied archive.

## Not measured (at resolution)

- The same edit on a cross (FreeRTOS/ThreadX/NuttX) cmake fixture. Same arm and
  same copy rule, but only the native C XRCE group was rebuilt.
- An edit that reaches the ninja arm (a C header the C/C++ sources include): a
  byte-identical OBJECT there is ninja's own business and is not covered here.
