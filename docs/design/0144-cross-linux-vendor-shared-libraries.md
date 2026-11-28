---
rfc: 0144
title: "C and C++ nodes on a cross Linux board, linking a vendor's shared libraries"
status: Stable
area: boards
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-589]
supersedes: []
superseded-by: null
---

# RFC-0144 — C and C++ nodes on a cross Linux board, linking a vendor's shared libraries

**Status:** Stable. Ruled 2026-11-29 (Q1–Q4 as recommended; Q1 adds a
load-time symbol check, D5), then reviewed against the tree the same day
(the review fixed D1's resolver shape, D2's check and loader key, D3's row
kind and carriers, D4's run-plan channel and D5's search path). Work:
phase-589 W8.

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
  `[board.cmake] toolchain_file`. The file resolves a GCC-shaped compiler
  with `nros_cross_toolchain_resolve` (override variable, then the SDK
  store's pinned `<prefix>-gcc`, then `<prefix>-gcc` on PATH), and reports
  it with `nros_cross_toolchain_report`, which applies the GCC 11 floor to
  C++. `check-cross-toolchain-provenance` (R1-R6) holds every toolchain file
  to that pair, or to the licence-gated shape `nros_gated_toolchain_report`.
- The CMake road for `linux-aarch64-musl` refuses today
  (`cmake/board/nano-ros-board-linux-aarch64-musl.cmake`).
- The device: Linux 5.10, musl 1.2.3, BusyBox. Its dynamic loader is
  `/lib/ld-musl-aarch64.so.1`. The vendor libraries were built with a musl
  gcc of the vendor's; whether a user's SDK carries that gcc varies, which is
  why it is an override and not the default (Q1).

## Design

### D1 — A cross C/C++ toolchain for the board

The board's descriptor gains `[board.cmake] toolchain_file =
"cmake/toolchain/aarch64-linux-musl.cmake"`, and its CMake module stops
refusing. The toolchain file sets `CMAKE_SYSTEM_NAME Linux` and
`CMAKE_SYSTEM_PROCESSOR aarch64`, then takes the first of:

- **D1.1 — the override.** `NROS_LINUX_MUSL_TOOLCHAIN_PREFIX`, a vendor musl
  gcc's prefix, through `nros_cross_toolchain_resolve`'s override rung. The
  GCC 11 floor still applies to C++: a vendor gcc below it builds C, and a
  C++ entry is refused naming the floor (issue 1936's R5/R6). The vendor gcc's
  version is the user's, so the configure reports it rather than assumes it.
- **D1.2 — the provisioned zig.** A new zig shape in the helper,
  `nros_zig_toolchain_resolve` / `nros_zig_toolchain_report`. It resolves
  `[tool.zig]`'s pinned store path (`<store>/zig/<pin>/zig-cc`, `zig-c++`,
  `zig-ar`), passes `--target=aarch64-linux-musl`, and reports `zig version`,
  with no GCC floor. It has NO PATH rung. A stray `aarch64-linux-musl-gcc` on
  PATH must not win over the pin, and the GCC helper's PATH rung would let it.
  `check-cross-toolchain-provenance` gains the pair as a third accepted shape
  (R1, R2 as for GCC; R3 checks `TOOL zig` names `[tool.zig]`; R4 checks
  resolve and report agree).

The configure prints which of the two it chose and why, as the other cross
boards do.

The zig default has two caveats. The user ruled them acceptable, with D5 as
the guard:

- **Newer headers.** zig compiles against its OWN musl's headers, which are
  newer than the device's 1.2.3. A function added to musl after 1.2.3
  compiles and links, then fails to resolve when the device loads the image.
  D5 catches it at build time when the sysroot carries the device's libc.
- **A different C++ ABI.** zig's C++ standard library is libc++. The
  vendor's libraries were built against libstdc++. No C++ type (a
  `std::string`, a `std::vector`, an exception) may cross into or out of a
  vendor library. MPP and `svp_acl` are C APIs, so this costs nothing today.
  A vendor library with a C++ API needs D1.1.

### D2 — The link mode is per image, static by default

`[image.<id>] link = "static" | "dynamic"`, default `static`. It is parsed
where the other nano-ros image keys are: `ImageBlock`, the `leaf_system`
walker, and `[image_defaults]` precedence. It is a nano-ros build fact, not
a SystemModel field.

- **static, Rust road:** today's image: `+crt-static`, `rust-lld`.
- **static, CMake road:** new, since this road refused before. The Rust
  runtime staticlib is built `+crt-static`, and the C++ driver links the
  executable `-static`.
- **dynamic, both roads together** (Q3), so one image never mixes.
  - **Rust road:** the board descriptor carries a second cargo overlay,
    applied in place of the static one's `[target]` table when the image is
    dynamic: `-C target-feature=-crt-static`, `linker` = the D1 C driver,
    `-C link-arg=--target=aarch64-linux-musl` (cc-rs's `CFLAGS_<triple>` never
    reaches the link), the dynamic loader below, and `--sysroot` (D3).
    `rust-lld` cannot do this link: in a dynamic build rustc hands a bare ld
    flavor no self-contained crt objects, no dynamic loader and no `libc.so`
    search path, which the C driver supplies.
  - **CMake road:** the staticlib is built `-crt-static`, and the C++ driver
    links dynamically.
  - **The loader is one board fact:** `[board.linux] dynamic_loader =
    "/lib/ld-musl-aarch64.so.1"`. Both roads read it, so no toolchain file or
    rustflag spells it.
  - **Measured in W8.b, not assumed:** what std's unwinder needs at run time
    under `-crt-static` on musl (a `libgcc_s` / `libunwind` the device must
    have), and whether zig supplies it at link.

**The check is on the linked ELF**, after either road links. A static image
whose ELF has a `PT_INTERP` or any `DT_NEEDED` is refused, naming `link =
"dynamic"`. A configure-time scan of the link set cannot classify a bare name
like `target_link_libraries(n ss_mpi)`, which is the common case here. A
dynamic image that needs no shared library builds, with a note. The
opposite rule (infer the mode, state nothing) is in Alternatives.

### D3 — The sysroot is a user-provided located resource

A dynamic image needs the device's libraries at link time and the vendor's
headers at compile time, both under the vendor SDK's sysroot. It is a
located resource, `sysroot-linux-aarch64-musl`. The name uses dashes, since a
dotted name reads as nested TOML tables both in the index and as a
`[sources]` key. `nros locate sysroot-linux-aarch64-musl --why` names its rung.

- **A new index row kind, `[provided.<name>]`,** for a resource nobody can
  provision. Its fields are `env` (here `NROS_SYSROOT_LINUX_AARCH64_MUSL`)
  and a description. Its rungs are arg > env > project `[sources]`, with no
  store rung and no checkout rung. Its refusal names those three ways to
  state it, never `nros setup --source`, which would be a false remedy.
  `[source.<name>]` cannot hold it: that row requires a `version` and refuses
  unknown fields.
- **Carriers, per road** (each one's absence is a refusal, never a silent
  host build):
  - **`nros build`, CMake road:** the configure command gets
    `-DNROS_SYSROOT=<dir>` and `-DNROS_LINK_MODE=<mode>`, cache variables the
    toolchain file reads.
  - **A bare `cmake` of the leaf:** the toolchain file asks the same
    questions itself. It includes `NanoRosLocate.cmake`, whose `nros locate
    --format cmake` answers the sysroot; `nros dev image leaf-system` answers the
    link mode from the leaf's `system.toml`.
  - **Cargo road:** `nros-cargo.toml`'s dynamic overlay carries
    `-C link-arg=--sysroot=<dir>`.
- **Search paths and include directories** for the vendor's libraries stay
  in the node's `CMakeLists.txt` (`target_include_directories`,
  `target_link_libraries(... ss_mpi svp_acl)`, `find_library` under
  `CMAKE_SYSROOT`). That is the acceptance line's "named only in its own
  CMake".
- **No sysroot:** a dynamic image refuses, naming the resource and the three
  ways to state it. A static image never asks.

### D4 — Running a dynamic image on this machine

`nros run` under `qemu-user:aarch64` gives qemu the sysroot as its loader
prefix:

- The run plan carries it as `args["ld_prefix"]`, with its rung in
  `provenance`. `RunPlan` has no environment channel, and does not need one.
- The runner passes it as qemu's `-L <dir>`, which means the same as
  `QEMU_LD_PREFIX`. `nros-runner.toml` declares the argument, so the runner
  descriptor still agrees with the runner.

qemu then finds `ld-musl-aarch64.so.1` and the vendor's libraries there.
This proves the image loads and links on the host. It is no evidence that
the vendor's media or NPU calls work, since those need the SoC, so the
device run stays the acceptance. Deploying stays the image's
`[image.<id>.run] command` (RFC-0118 D4b).

### D5 — The build checks the image's dynamic symbols against the sysroot

After a dynamic image links, `nros build` checks that the device can load
it, using only what the device's loader would see:

1. **The objects:** start at the image. Load each `DT_NEEDED` from the
   sysroot, searching musl's default path (`/lib`, `/usr/local/lib`,
   `/usr/lib`), the sysroot's `etc/ld-musl-aarch64.path` if present, and the
   object's own `DT_RUNPATH`, all under the sysroot. Repeat for each loaded
   library's own `DT_NEEDED`, to closure. The directories the node's CMake
   linked from are NOT searched: the device's loader never sees them.
2. **The symbols:** collect every non-weak undefined dynamic symbol of every
   loaded object. musl resolves each against all loaded objects, so each
   must be exported by one of them.
3. **The verdict:** a symbol nothing exports, or a needed library not found,
   refuses the build, naming the symbol (or library) and where it looked.

So a newer-musl function (D1's first caveat) is a build error rather than a
load failure on the device. musl has no symbol versioning, so the check is
by name. When the sysroot carries no `libc.so` (no device libc to check
against), the build says the check is not verified, never that it passed. A
static image has no dynamic symbols and skips the step.

It is implemented with the `object` crate, already a `nros-cli-core`
dependency. `check-dist-runtime-deps.py` parses `DT_NEEDED` for another job:
the host-tool closure of an SDK dist, in another language and on another
lane. So the two are separate readers, and each is the only one on its own
side.

## Rulings (user, 2026-11-29)

- **Q1 — Default compiler for C/C++: zig or the vendor's gcc?** RULED:
  zig by default, the vendor musl gcc as the D1.1 override. Record the two
  caveats (D1), and check symbols at load time where feasible (D5).
  - **The libc caveat:** zig compiles against ITS musl's headers, which are
    newer than the device's 1.2.3. A symbol added after 1.2.3 links, then
    fails to resolve on the device at load time.
  - **The C++ caveat:** zig's libc++ is a different C++ ABI from the vendor's
    libstdc++, so no C++ type may cross into a vendor library. MPP and
    `svp_acl` are C APIs, so this is not a problem today.
  - **The alternative:** default to the vendor toolchain when its prefix is
    found. That is exact on both counts, but only exists where the SDK is
    installed.
- **Q2 — The link mode: stated (D2) or inferred?** RULED: stated per image,
  `link = "dynamic"`. A static image that links a shared library is
  refused. The image says what it is, and a mismatch is a refusal, not a
  different binary. The review moved the check from the configure to the
  linked ELF (D2): a configure cannot classify a bare library name.
- **Q3 — The Rust road in a dynamic image.** RULED: `link = "dynamic"`
  switches both roads (D2), so one image never mixes them.
- **Q4 — Where the sysroot is named.** RULED: a located resource,
  `sysroot.linux-aarch64-musl`, on the locate ladder (D3). The path is the
  user's machine, not the image. The review spelled it
  `sysroot-linux-aarch64-musl`, since a dotted name reads as nested TOML
  tables, and gave it a row kind of its own (D3).

## Acceptance (phase-589 W8)

A C++ node on `linux-aarch64-musl` that links a shared library named only in
its own CMake builds dynamically against the board's sysroot, with nothing
set by hand beyond stating the sysroot. D5 refuses a symbol the sysroot does
not export. The image starts under `nros run` with `QEMU_LD_PREFIX` set, and
it runs on the device (device-only line).

## Alternatives

- **Infer dynamic from the link set.** No statement to write, but the
  binary's shape changes silently when a dependency changes from `.a` to
  `.so`, and the Rust road cannot see a CMake link set at all.
- **A nano-ros table of vendor libraries** (`[image.<id>.link] libs = [...]`).
  Rejected: CMake already says this, and a second spelling of a link set is
  the drift class every carrier in this tree has had.
- **Ship the vendor's toolchain as an SDK dist.** It is the vendor's to
  distribute, not ours.
