---
id: 879
title: The macOS qemu dist links Homebrew libs and is not bundled
status: resolved
type: bug
area: build
related: [issue-0368]
resolved_in: qemu-11.0.0-nros6
---

## Problem

`qemu-11.0.0-nros4` bundled its shared-library closure on **Linux only**. The
macOS dist still linked Homebrew absolute paths — glib, pixman, slirp, intl,
pcre2 — so `nros setup --tool qemu` on a Mac without those formulae installed
the tarball and then failed in dyld: issue 0368 F3 unfixed on that host.

## Why it was filed instead of fixed, and why that was wrong

The stated blocker was "no macOS runner exists to verify a bundling pass
against, and an untested `install_name_tool` pass ships worse than a documented
Homebrew requirement."

The first half was false on inspection: the `build-tool` job that cuts the dist
**runs on `macos-14`**. What was missing was not a runner but a check. Worth
keeping as a shape — "we cannot test this platform" was true of the developer's
laptop and not of the pipeline that builds the artifact, and nobody checked
which one the claim was about.

## Fix (`qemu-11.0.0-nros6`, 2026-08-29)

`scripts/build-qemu.sh` in NEWSLabNTU/nano-ros-sdk bundles both platforms, by
different mechanisms because the platforms differ:

- **Linux** — rpath `$ORIGIN/../lib` on the binaries, `$ORIGIN` on each bundled
  lib (DT_RUNPATH is not inherited, so libgio cannot otherwise find libglib).
- **macOS** — a launcher setting `DYLD_LIBRARY_PATH`, which substitutes by LEAF
  NAME even for absolute install names. No binary is modified, so no signature
  is invalidated and no `codesign -s - -f` pass is needed. Same idiom as
  `build-xrce-agent.sh` in that repo, so the two now bundle one way.

Both **prove the bundle wins** rather than assuming it — the whole failure mode
is a build host that happens to have the libraries, which is how `-nros2`
shipped broken. Linux realpath-compares every `ldd` resolution against the
prefix; macOS reads `DYLD_PRINT_LIBRARIES` and requires each bundled dylib to
load from the bundle AND no Homebrew path to appear for that leaf name.

Result: 18 libs on Linux, 19 dylibs on macOS. The `[tool.qemu].system` list and
the `[system.libslirp]` entry are gone from the index — a clean host needs no
package manager for qemu at all.

`otool -L` gives only DIRECT dependencies, unlike `ldd`, so the macOS walk is a
real worklist. A non-absolute install name (`@rpath`/`@loader_path`) aborts the
build rather than being skipped: a silent skip ships a dist bundled everywhere
except the one library that was unhandled.

**Scope:** `qemu-system-*` only. `qemu-ga` ships unbundled; nano-ros never
invokes it.
