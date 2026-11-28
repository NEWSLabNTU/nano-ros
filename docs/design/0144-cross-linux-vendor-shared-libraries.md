---
rfc: 0144
title: "C and C++ nodes on a cross Linux board, linking a vendor's shared libraries"
status: Draft
area: boards
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-589]
supersedes: []
superseded-by: null
---

# RFC-0144 — C and C++ nodes on a cross Linux board, linking a vendor's shared libraries

**Status:** Draft. Awaiting the user's ruling on Q1–Q4. Work: phase-589 W8.

## Summary

The `linux-aarch64-musl` board builds Rust images only. Its CMake module
refuses C and C++, because without a cross toolchain the configure would
build a program for the host and call it the board's. The user's next step
is C and C++ nodes that call the SoC vendor's media SDK and NPU runtime
(HiSilicon MPP V1.0.1.1: `ss_mpi_*`, plus `libss_mpi_km.so` and
`libsvp_acl.so`). Those ship as musl SHARED objects, so a node that links
them cannot be a static image.

This RFC gives the board a cross C/C++ toolchain on the CMake road. It also
adds a per-image link mode (static by default, dynamic when the image says
so), and a sysroot the build and the on-host run both find through the one
locate ladder. Which libraries a node links, and from where, stays in that
node's own `CMakeLists.txt`. nano-ros adds no table for it.

## Background

- The Rust road: rustc targets `aarch64-unknown-linux-musl` with
  `+crt-static`, links with `rust-lld`, and compiles any C a build script
  needs with zig (`zig-cc --target=aarch64-linux-musl`). The result is a
  static ELF that needs nothing on the device.
- The CMake road for cross boards: a board names a toolchain file in
  `[board.cmake] toolchain_file`. The file resolves its compiler with
  `nros_cross_toolchain_resolve` (an override variable, then the provisioned
  default) and reports which one it used.
- The device: Linux 5.10, musl 1.2.3, BusyBox. Its dynamic loader is
  `/lib/ld-musl-aarch64.so.1`. The vendor libraries were presumably built
  with a musl gcc of the vendor's. Whether their SDK ships it to users is
  not recorded here (to confirm with the user).

## Design

### D1 — A cross C/C++ toolchain for the board

The board's descriptor gains `[board.cmake] toolchain_file =
"cmake/toolchain/aarch64-linux-musl.cmake"`, and its CMake module stops
refusing. The toolchain file sets `CMAKE_SYSTEM_NAME Linux` and
`CMAKE_SYSTEM_PROCESSOR aarch64`, then resolves the compiler in this order:

1. **An override prefix** (`NROS_LINUX_MUSL_TOOLCHAIN_PREFIX`, a vendor
   musl gcc's prefix), resolved by
   `nros_cross_toolchain_resolve` like every other cross board.
2. **The provisioned zig**, through its one-word entry points (`zig-cc`,
   `zig-c++`, `zig-ar`), with `--target=aarch64-linux-musl`.

The configure prints the compiler it chose and why, as the other cross
boards do.

### D2 — The link mode is per image, static by default

`[image.<id>] link = "static" | "dynamic"`, default `static`.

- **static:** today's image, on both roads.
- **dynamic:** both roads change together.
  - On the Rust road, rustc builds `-C target-feature=-crt-static` and links
    with the C toolchain's driver, not `rust-lld`, which has no musl startup
    objects or dynamic loader.
  - On the CMake road, the Rust runtime is still a staticlib (built with
    `-crt-static`), and the C++ driver links the executable dynamically, with
    `--dynamic-linker=/lib/ld-musl-aarch64.so.1`.

The mode is STATED, not inferred from the node's `CMakeLists.txt`. The
configure checks the statement against what the node links. A static image
whose link set has a `SHARED` or `IMPORTED SHARED` library, or a `*.so`
path, is refused, with a message naming `link = "dynamic"`. A dynamic image
that links nothing shared builds, with a note. The opposite rule (infer the
mode, state nothing) is in Alternatives.

### D3 — The sysroot comes from the locate ladder

A dynamic image needs the device's libraries at link time, and the vendor's
headers at compile time. Both live under the vendor SDK's sysroot. It is a
located resource, `sysroot.linux-aarch64-musl`, on the one ladder (arg > env
> project `[sources]` > …), so `nros locate sysroot.linux-aarch64-musl --why`
answers where it came from.

- **The toolchain file** passes `--sysroot=<dir>` when one is located. An
  override toolchain (D1.1) usually has its own and needs none.
- **Search paths and include directories** for the vendor's libraries stay
  in the node's `CMakeLists.txt` (`target_include_directories`,
  `target_link_libraries(... ss_mpi svp_acl)` with `link_directories` or
  `find_library` under `CMAKE_SYSROOT`). That is the acceptance line's
  "named only in its own CMake".
- **No sysroot:** a dynamic image refuses to configure, naming the resource
  and the ways to state it. A static image never asks.

### D4 — Running a dynamic image on this machine

`nros run` under `qemu-user:aarch64` sets `QEMU_LD_PREFIX` to the located
sysroot. qemu then finds `ld-musl-aarch64.so.1` and the vendor's libraries
there. This proves the image loads and links on the host. It is not evidence
that the vendor's media or NPU calls work, since those need the SoC, so the
device run stays the acceptance. Deploying stays the image's
`[image.<id>.run] command` (RFC-0118 D4b).

## Questions for the user

- **Q1 — Default compiler for C/C++: zig or the vendor's gcc?** (And does
  the vendor SDK ship a musl cross gcc at all?)
  - Recommend zig by default, with the vendor gcc as the D1.1 override.
  - **The libc caveat:** zig compiles against ITS musl's headers, which are
    newer than the device's 1.2.3. A symbol added after 1.2.3 links, then
    fails to resolve on the device at load time.
  - **The C++ caveat:** zig's libc++ is a different C++ ABI from the vendor's
    libstdc++, so no C++ type may cross into a vendor library. MPP and
    `svp_acl` are C APIs, so this is not a problem today.
  - **The alternative:** default to the vendor toolchain when its prefix is
    found. That is exact on both counts, but only exists where the SDK is
    installed.
- **Q2 — The link mode: stated (D2) or inferred?** Recommend stated, plus the
  configure check, so the image says what it is and a mismatch is a refusal,
  not a different binary.
- **Q3 — The Rust road in a dynamic image.** Recommend that `link =
  "dynamic"` applies to both roads (D2), so one image never mixes them. The
  alternative is to keep the Rust road static-only and make `dynamic` a
  CMake-road word.
- **Q4 — The sysroot resource name.** `sysroot.<board>` (proposed), or a key
  in `[image.<id>]`. Recommend the ladder resource: the path is the user's
  machine, not the image.

## Acceptance (phase-589 W8)

A C++ node on `linux-aarch64-musl` that links a shared library named only in
its own CMake builds dynamically against the board's sysroot, with nothing
set by hand beyond stating the sysroot. It starts under `nros run` with
`QEMU_LD_PREFIX` set, and it runs on the device (device-only line).

## Alternatives

- **Infer dynamic from the link set.** No statement to write, but the
  binary's shape changes silently when a dependency changes from `.a` to
  `.so`, and the Rust road cannot see a CMake link set at all.
- **A nano-ros table of vendor libraries** (`[image.<id>.link] libs = [...]`).
  Rejected: CMake already says this, and a second spelling of a link set is
  the drift class every carrier in this tree has had.
- **Ship the vendor's toolchain as an SDK dist.** It is the vendor's to
  distribute, not ours.
