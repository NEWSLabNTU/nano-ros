---
id: 91
title: Native C fixture link fails — GNU ld (binutils 2.38) internal BFD error merging the Rust staticlib
status: resolved
type: bug
area: build
related: [phase-258]
resolved_in: "nano-ros-posix.cmake prefers lld for the native link when present + lld added to the CI base image"
---

## Resolved (2026-06-20)

The BFD bug is a GNU ld (binutils 2.38) defect, not nano-ros code; `lld` does not
hit it. Nano-ros-side mitigation:

- **`cmake/platform/nano-ros-posix.cmake`** — `nros_platform_link_app` now prefers
  `lld` for the native executable link when `ld.lld` is on PATH (`-fuse-ld=lld`),
  auto-detected (`find_program`). No-op when lld is absent (byte-identical, verified:
  the native C talker configures clean with `NROS_NATIVE_USE_LLD` OFF on a host
  without lld). Opt out with `-DNROS_NATIVE_USE_LLD=OFF`.
- **`ci/docker/ci-base/Dockerfile`** — added `lld` to the apt set, so the CI image's
  native links use it (consistent + future-proofs against a binutils regression).

A fresh Ubuntu-22.04 dev host hitting the bug installs lld once
(`sudo apt install lld`); the cmake then auto-prefers it — no manual `-fuse-ld` or
binutils bump. (The lld-avoids-the-bug link could not be reproduced on this host —
binutils 2.38, no lld, no sudo — so the runtime proof is CI-side / on an
lld-equipped host; the mechanism is the standard `-fuse-ld=lld`.)

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
