---
id: 492
title: "The CMake self-provisioned CycloneDDS builds slim GCC-LTO objects that
  `ld.lld` cannot link, so `build-test-fixtures lane=native` fails on any host
  whose C/C++ examples link with lld"
status: resolved
resolved_in: phase-347
type: bug
area: build
related: [issue-0475, phase-340, phase-186]
---

## Symptom

`just build-test-fixtures lane=native` dies linking the first CycloneDDS C
fixture:

```
ld.lld: error: undefined symbol: dds_get_guid
>>> referenced by graph.cpp
>>>               graph.cpp.o:(…graph_init…) in archive …/libnros_rmw_cyclonedds.a
ld.lld: error: undefined symbol: dds_create_topic
ld.lld: error: undefined symbol: ddsrt_calloc
```

36 undefined symbols, all CycloneDDS.

## Why it does not look like what it is

Every obvious check says the link is correct:

* `libddsc.a` **is** on the link line, inside the whole-archive group:
  `-Wl,--whole-archive,…/libnros_rmw_cyclonedds.a,…/lib/libddsc.a,--no-whole-archive`
* the archive is 8.4 MB, a normal (non-thin) `ar` with 148 members, x86-64
* `nm --defined-only` reports `00000000 T dds_get_guid`, and
  `--print-file-name` names the member: `dds_entity.c.o`
* `-Wl,-t` shows **all 148 members loaded**, `libddsc.a(dds_entity.c.o)` among
  them

So: the defining member is loaded, the symbol is global, the name is unmangled
and identical on both sides — and the link still fails. `ddsrt_calloc` in the
list also makes it look like issue 0475's whole-archive-group breakage, which it
is not.

The minimal case isolates it. `t.c` referencing `dds_get_guid`, linked directly
against the extracted `dds_entity.c.o`:

```
-fuse-ld=lld   ->  ld.lld: error: undefined symbol: dds_get_guid
-fuse-ld=bfd   ->  links clean
```

Same object, same symbol, two linkers, opposite results. And `readelf -sW` finds
**no** `dds_get_guid` at all while `nm` does.

## Cause

The object is a **slim GCC LTO object**:

```
$ readelf -SW dds_entity.c.o | grep gnu.lto
.gnu.lto_.profile.…  .gnu.lto_.icf.…  .gnu.lto_.ipa_sra.…  .gnu.lto_.inline.…
$ readelf -sW dds_entity.c.o | grep -c 'FUNC\|OBJECT'
1
```

The real symbols live in GCC IR, not the ELF symbol table. `nm` sees them
because it loads GCC's `liblto_plugin.so`; `ld.bfd` links them for the same
reason. **`ld.lld` cannot read GCC LTO IR at all** — and
`cmake/platform/nano-ros-posix.cmake` links with `-fuse-ld=lld`.

The LTO comes from CycloneDDS's own default:

```cmake
# third-party/dds/cyclonedds/CMakeLists.txt:218
option(ENABLE_LTO "Enable link time optimization." ON)
```

and the CMake self-provision `add_subdirectory`s Cyclone without overriding it
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake`).

**Installing a CycloneDDS does not help.** Phase 186 sets
`CMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON` and self-provisions from
`third-party/dds/cyclonedds`, so `nros setup --tool cyclonedds` is inert here —
which is its own diagnostic dead end, since the store install looks like the
obvious fix and changes nothing.

## The class

**The Rust self-provision has always had this fix.**
`packages/rmw/cyclonedds/cyclonedds-sys/build.rs:48`:

```rust
// - ENABLE_LTO=OFF: rust-lld cannot link slim-LTO objects produced
//   by Cyclone's default GCC LTO settings (cf. MEMORY: "ThreadX
//   Cyclone LTO vs rust-lld" — same hazard on native).
.define("ENABLE_LTO", "OFF")
```

It names the hazard, names the linker, and says *"same hazard on native"*. The
CMake self-provision — the path every C/C++ example takes — never got it. One of
two sibling paths fixed, which is the class CLAUDE.md's "fix the CLASS, not the
reported site" rule exists for, and the third instance this session (the espflash
PATH whitelist and the esp32 QEMU resolver were the others: a fix applied to one
of two equivalent paths).

## Fix

Set `ENABLE_LTO OFF` in the CMake self-provision, before `add_subdirectory`.
Both spellings, deliberately:

* the **normal variable** is what `option()` honours under CMP0077 NEW;
* the **FORCEd cache entry** overwrites the `ENABLE_LTO:BOOL=ON` that existing
  build trees already carry from an earlier configure — without it a stale tree
  keeps emitting LTO objects and keeps failing.

An existing build tree must still be wiped once, since its objects are already
LTO.

## Why CI does not see it

The distrobox lane and CI link these fixtures on a host whose toolchain pairing
differs; this reproduces on Arch with gcc 16.1.1 / LLD 22.1.8. It blocks
`lane=native` on a host configuration the project documents as supported
(`docs/development/ros2-on-non-ubuntu.md`), which is how it was found — trying to
produce a clean tree to settle the phase-340 identity-budget reading.

## RESOLVED 2026-08-11 — and re-proved twice since

The fix (`ENABLE_LTO OFF` on the CMake self-provision, both spellings) landed in
`78d6c79e6` and was verified then: `c_talker`, the exact binary that produced the
36 undefined `dds_*` symbols, linked.

It has since been exercised twice more without special handling, which is the
better evidence:

* phase-347 W5 rebuilt `examples/native/c/talker/build-cyclonedds` from scratch
  to validate the moved codegen hook — it produced both `__cyclonedds_ts`
  libraries and linked a 13 MB binary carrying 28 descriptor symbols;
* the workspace-fixtures rebuild after the corrosion bump completed RC=0 across
  all linux workspaces, cyclonedds rows included.

This issue was left `open` after its fix landed — a bookkeeping miss on my part,
not an unresolved defect. Recorded rather than quietly flipped, because "fixed
but still open" is how a closed list of known bugs stops matching the tree.
