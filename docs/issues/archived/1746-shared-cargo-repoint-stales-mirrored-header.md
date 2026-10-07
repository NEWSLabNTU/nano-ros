---
id: 1746
title: "A re-configure that re-points the shared cargo dir twice leaves a native C++ leaf's mirrored sizes header from the WRONG key, and its first build links `undefined reference to nros_config_variant_sz_*`"
status: resolved
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-07
resolved_in: 2026-10-08
related: [0088, 0268, 0834, 0945, 0987, 1729]
---

## What

Found while proving issue 1729: an edit to a cmake module re-configures every
native leaf, and on the next `just build-test-fixtures lane=tier1` the native
stage failed on `examples/native/cpp/talker/build-{zenoh,xrce,cyclonedds}` —
one per run, deterministic, never a race:

```
/usr/bin/ld: CMakeFiles/cpp_talker.dir/src/main.cpp.o:(.data.rel.ro+0x8):
  undefined reference to `nros_config_variant_sz_bd634ed6d9ad21fb'
```

`libnros_cpp.a` defines `nros_config_variant_sz_c8b5e57577881998`
(`NROS_EXECUTOR_SIZE 20592`, sized from the leaf's sizing descriptor); the
mirrored `<build>/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
said `NROS_EXECUTOR_SIZE 90576` — the crate default, i.e. a build with NO
descriptor — and was newer than the archive, so the restat'd mirror rule did
not run and `main.cpp.o` recompiled against it.

## The lead

The configure log of the failing dir prints the shared-cargo link flipping
TWICE in one configure (`cmake/NanoRosCorrosion.cmake`,
`nros_share_corrosion_cargo_dir`):

```
-- nano-ros: shared-cargo key changed, re-pointing <build>/cargo
  was: ...|NROS_DECLARED_INFRA_QUERYABLES=none|NROS_DECLARED_NODES=1|NROS_SIZING_DESCR...
  now: ...|profile=release|target=x86_64-unknown-linux-gnu
...
-- nano-ros: shared-cargo key changed, re-pointing <build>/cargo
  was: ...|target=x86_64-unknown-linux-gnu
  now: ...|NROS_DECLARED_INFRA_QUERYABLES=none|NROS_DECLARED_NODES=1|NROS_SIZING_DESCR...
```

The key is computed once before the descriptor knobs are known and once after,
so for a window of the configure the leaf points at the no-descriptor key's
directory, whose header is the 90576 one. Not yet traced: which step copies
the header in that window. A second plain `ninja -C <dir>` converges (the
cargo step re-runs and the mirror follows), which is why it reads as a flake.

## Acceptance

The key is computed once (after the knobs it names are known), or the mirror
has a real edge on the key, so a re-configured leaf's FIRST build links. A
regression test re-configures a native C++ leaf and builds it once.

## Resolution

**Root cause — the configure-time HEAL, not the build-time mirror.**
`packages/api/nros-cpp/CMakeLists.txt` ran a configure-time heal
(`execute_process` of `mirror-generated-header.sh`, issues 0268/0985) at its own
scope. On a re-configure, `nros-c`'s configure has just re-pointed `<build>/cargo`
at the PROVISIONAL key (base + knobs). The deferred `_nros_entity_facts_flush`
re-points it at the final key (base + knobs + entity facts, issue 1700) only at
the END of the configure. The heal ran in that window. It copied the
provisional key's header (`NROS_EXECUTOR_SIZE 90576`, no descriptor) over the
right one, with a new mtime. The build-time mirror's only file input is
`libnros_cpp.a`, which was now OLDER than its output, so ninja skipped it. In
other words, a repair for drift (0268) caused drift and then suppressed its own
fix, which is 0985's shape coming back through the issue-1700 re-key.

Measured, without a wipe: `cmake <dir>` on a settled
`examples/native/cpp/talker/build-zenoh` printed `re-pointing` twice, and the
mirrored header went 20592 → 90576 with an mtime 5 s after the archive.
`ninja -t query` names the archive as the mirror's only input. One `ninja`
then failed the link on `nros_config_variant_sz_bd634ed6d9ad21fb`. A second
build settles only when something else rebuilds the archive.

**Fix.** The heal is QUEUED (`nros_config_header_heal`, in
`cmake/NanoRosEntityFacts.cmake`) and executed by
`_nros_config_header_heal_flush`. When a facts flush is scheduled, the flush
calls it right after its re-key. Otherwise it runs from its own deferred call.
So no configure-time reader sees a provisional key. Sweep: the mirror script
has exactly one configure-time caller (`nros-cpp`). The nros-c mirror is
build-time only, and the NuttX/Zephyr callers of `nros_shared_cargo_dir` pass
`--target-dir` directly, with no link and no re-key. The double re-point itself
is left alone: it is now harmless, and removing it would change the key
semantics issue 1700 settled.

**Guards.**
- `check-config-header-single-writer` now also refuses a configure-time
  `execute_process` of the mirror script anywhere but the deferred flush, even
  when the script is named through a variable. It fails on the pre-fix
  `nros-cpp` (measured).
- Case G of `tests/cmake-resolved-seed-tests.sh` runs the real flush and the
  real mirror script over a provisional → final link. It also runs an
  immediate heal as a control that reproduces the defect. Against the pre-fix
  module it fails 2 checks (measured).

**Proof.** With the same settle → `cmake <dir>` → one `ninja` sequence, all
three `examples/native/cpp/talker/build-{zenoh,xrce,cyclonedds}` dirs now link
on the first build. Each run re-pointed twice, the archive was not rebuilt,
and the header stayed at 20592. A/B on `build-zenoh`, rebased onto main of
2026-10-09: with only `nros-cpp/CMakeLists.txt` and `NanoRosEntityFacts.cmake`
swapped back to main's, the first build failed (`undefined=1`, header 90576).
With the fix it linked. In both runs the configure re-pointed twice and the
archive was not rebuilt.

**A sibling the proof run hit: a write-once generated file.** The first
`lane=tier1` fixture build on this branch failed somewhere else. Every
workspace build dir configured before phase-482 W1 failed with `include could
not find requested file: .../cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`.
`_nros_emit_workspace_find_stubs` writes each
`<build>/nros-find-stubs/Find<pkg>.cmake` with an ABSOLUTE include path, under
`if(NOT EXISTS)`, so a stub kept the first configure's path forever. It is the
same class as the heal: a configure-time product that a re-configure does not
bring current. It is now write-if-different. A sweep of the `if(NOT EXISTS)` +
`file(WRITE)` sites in `cmake/` and `zephyr/cmake/` found two others. Neither
embeds a path, and both are constant content.
