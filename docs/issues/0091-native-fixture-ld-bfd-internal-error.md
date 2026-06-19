---
id: 91
title: Native C fixture link fails — GNU ld (binutils 2.38) internal BFD error merging the Rust staticlib
status: open
type: bug
area: build
related: [phase-258]
---

## Symptom (2026-06-19)

`just build-test-fixtures` (the `native` leaf, `fixture-native-c-zenoh`) — linking
the `c_talker` example executable fails inside GNU `ld`:

```
/usr/bin/cc -O3 -DNDEBUG  .../main.c.o  .../nros_app_register_backends.c.o -o c_talker \
   libstd_msgs__nano_ros_c.a libbuiltin_interfaces__nano_ros_c.a \
   nano_ros/packages/core/nros-c/libnros_c.a -lgcc_s -lutil -lc -lpthread -lm -ldl \
   nano_ros/nros_platform_posix_build/libnros_platform_posix.a -lrt
/usr/bin/ld: BFD (GNU Binutils for Ubuntu) 2.38 internal error, aborting at
   ../../bfd/merge.c:939 in _bfd_merged_section_offset
/usr/bin/ld: Please report this bug.
collect2: error: ld returned 1 exit status
```

## Cause

A **GNU ld / binutils 2.38 bug** (`_bfd_merged_section_offset`, `merge.c:939`),
not nano-ros code — `ld` itself prints "Please report this bug". Triggered while
merging mergeable sections (likely `.debug_str` / `.rodata.cst`) of the large Rust
staticlib `libnros_c.a` on this host's binutils 2.38. Reproduces in the native C
fixture link; newer binutils (e.g. the CI runner image) does not hit it.

## Workarounds / fix direction

Toolchain-side, pick one:
- Upgrade binutils past the 2.38 merge bug (newer Ubuntu / a provisioned ld), or
  use `lld` for the native link (`-fuse-ld=lld`).
- Or reduce mergeable-section pressure from the Rust staticlib on the native link
  (strip debug / `-Wl,--no-keep-memory` style mitigations) if an upgrade isn't an
  option for the supported baseline.

A nano-ros-side mitigation (prefer `lld` when present for the native fixture link,
or document the binutils baseline in the `apt-packages` doctor check) would make
fresh Ubuntu-22.04 hosts build the native fixtures without a manual toolchain bump.

## Scope

Environment/toolchain — surfaced running host `test-all` for phase-258. Blocks the
native C fixture link on binutils 2.38; the native cpp/mixed *workspace* fixtures
(different link sets) build fine, and CI's image is unaffected.
