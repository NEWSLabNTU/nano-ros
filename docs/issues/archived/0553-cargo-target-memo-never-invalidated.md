---
id: 553
title: "`_NROS_RUST_TARGET` is a permanent cache memo nothing invalidates, so a build tree configured host-first cross-compiles for the host forever"
status: resolved
resolved_in: issue-0553
type: bug
area: build
related: [issue-0551, issue-0525, phase-340, phase-155]
---

## Symptom

Two failures that looked unrelated, in the NuttX lane, after issue 0551 cleared
the `nuttx/config.h` blocker in front of them:

```
ld: .../nano_ros_cpp_ffi_std_msgs/target/x86_64-unknown-linux-gnu/nros-minsizerel/
    libnano_ros_cpp_ffi_std_msgs.a: error adding symbols: file format not recognized
```

and, in the same workspace, `nros_nuttx_include_root()` resolving to the SHARED
NuttX tree rather than this arch's export snapshot — i.e. 0551 apparently
un-fixing itself in one tree.

## Cause

`_nros_resolve_rust_target` (`cmake/NanoRosCodegenCore.cmake`) memoized its
answer into a permanent `CACHE INTERNAL` entry and short-circuited on it before
reading anything else:

```cmake
if(DEFINED CACHE{_NROS_RUST_TARGET})
    set(${_out} "$CACHE{_NROS_RUST_TARGET}" PARENT_SCOPE)
    return()
endif()
```

Nothing invalidates it. So whichever scope called the resolver FIRST decided the
triple for the whole build tree — permanently, across every later reconfigure,
because the memo lives in the CMake cache rather than in a target dir a clean
rebuild would remove.

`examples/workspaces/realtime-cpp/build-workspace-fixtures-nuttx` was configured
host-first, so it answered `x86_64-unknown-linux-gnu` while its own cache plainly
carried:

```
Rust_CARGO_TARGET:STRING=armv7a-nuttx-eabihf
_NROS_RUST_TARGET:INTERNAL=x86_64-unknown-linux-gnu
```

One stale string, two failures:

* the message FFI staticlib path is `<target-dir>/<triple>/<profile>/`
  (`NanoRosGenerateInterfaces.cmake`), so the glue crate was built and named
  under the host triple and ld rejected it on the ARM line;
* `nros_nuttx_include_root()` derives the NuttX arch from this triple, saw a
  host triple, matched neither `arm` nor `riscv`, and fell back to the shared
  tree — which is why 0551's "fifth site" looked like a separate unfixed site.
  It never was: the includes file is generated from
  `INTERFACE_INCLUDE_DIRECTORIES`, i.e. the same `NanoRos` property 0551 fixed.
  It was being fed a poisoned triple.

Proof, before any code change: `cmake -U _NROS_RUST_TARGET` + reconfigure flipped
the memo to `armv7a-nuttx-eabihf`, and the regenerated `nuttx_entry_includes.txt`
went from `third-party/nuttx/nuttx/include` to
`third-party/nuttx/nuttx/nros-nuttx-export-arm/include`.

## Fix

An EXPLICIT target now outranks the memo; the memo is consulted only when
nothing explicit is visible:

1. `Rust_CARGO_TARGET` (normal or cache) — authoritative
2. `CACHE{Rust_CARGO_TARGET}` — a `-D` on the configure line, visible from every
   scope, so it survives the `add_subdirectory()` boundary the normal variable
   does not cross
3. `_NROS_RUST_TARGET` — the memo
4. corrosion's cache copies
5. `rustc -vV` host

and the memo is rewritten on every resolution that reaches the bottom, so it
tracks the authoritative answer instead of freezing the first one.

This keeps what the memo is actually for — not re-running `rustc -vV` per call,
and giving a scope that cannot see the normal variable a consistent reading —
while making a stale one unreachable in any build that states its triple.
Existing poisoned trees self-heal on the next configure; no cache wipe needed.

The corrosion copies stay BELOW the memo deliberately: `Rust_CARGO_TARGET_CACHED`
was the HOST triple in the very tree whose requested target was ARM, so promoting
them above the memo would let a blind scope overwrite a good memo with corrosion's
host copy — the same bug facing the other way.

## Why it survived

`check-cargo-target-spelling` covers this resolver in seven arms and had **no
memo coverage at all**. It tested which SOURCE wins among the live variables and
never that a stale memo must lose. Three arms added:

* a stale memo loses to an explicit target;
* with nothing explicit, the memo is still used (demoting it must not disable it);
* the memo outranks corrosion's cache copy.

Verified non-vacuous: with the old precedence reinstated, the first arm fails
(`expected NROS_PROBE_TRIPLE=[armv7a-nuttx-eabihf]`) and the suite goes red.

## Blast radius

82 example build trees carry a memo; 50 name the host triple. Every cross-named
tree among those is `threadx-linux` or a threadx-linux workspace, where the host
triple is correct by design (ThreadX-on-Linux pins it). No victims besides the
NuttX workspace tree. The MECHANISM is platform-agnostic, though: any tree
configured host-first and then cross is wrong, permanently, and survives clean
reconfigures.

## Acceptance

* `just nuttx build-fixtures-arm` → rc=0, 5 artifacts built, including
  `examples/workspaces/realtime-cpp/.../nuttx_entry` — the exact leaf that
  failed the ARM link. Zero `file format not recognized`, zero
  `nuttx/config.h: No such file`. Verified 2026-08-13.
* Forcing `-D_NROS_RUST_TARGET:INTERNAL=x86_64-unknown-linux-gnu` on a
  reconfigure of that tree is corrected to `armv7a-nuttx-eabihf` rather than
  honoured.
* `bash packages/testing/nros-tests/tests/cargo_target_spelling.sh` — 10/10.
