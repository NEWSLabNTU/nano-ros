---
id: 1744
title: "Both Zephyr SDK dists need host libraries their `system = [..]` does not declare (dist-runtime-deps is red on a clean tree)"
status: resolved
type: bug
area: [build, tooling, zephyr]
severity: low
found: 2026-10-07
related: [issue-1739, issue-1452, issue-0928, issue-0610, issue-1259]
resolved_in: "upstream dists are held to their run set (issue 1744)"
---

## What was measured

`check-dist-runtime-deps` ran on this host's clean tree for issue 1739. It is
reached only from `just workspace doctor`, so no workflow had run it. Its
output had two parts.

1. **A gate defect, fixed in the 1739 change.** About half the findings were
   not sonames. The `ldd` reader took the first word of any line holding `=>`
   or `not found`, so it misread two kinds of line:
   - A loader diagnostic, for example
     `…/sysroots/x86_64-pokysdk-linux/lib/librt-2.27.so: version
     `GLIBC_PRIVATE' not found`, became a "soname" ending in `:`.
   - The dist's OWN relocated loader, which `ldd` prints as an absolute
     `PT_INTERP` path, read as an undeclared host library.

   The reader now matches only `<soname> => <path|not found>` lines. It
   treats a dependency that resolves INTO the dist as shipped.

2. **A real gap, which this issue tracks.** With the misreadings gone, both
   pinned Zephyr SDK dists still need host libraries that neither `system`
   list names:

   - **`[tool.zephyr-sdk]` 0.16.8 (12 sonames).** `libffi.so.8`,
     `libicu{data,uc}.so.70`, `liblzma.so.5`, `libp11-kit.so.0`,
     `libpcre.so.3`, `libpython3.8.so.1.0`, `libtasn1.so.6`. Four more have a
     `[prereq.*]` that the entry does not list: `libexpat1`, `libpcre2`,
     `libselinux1` and `libtinfo6`.
   - **`[tool.zephyr-sdk-1-0-1]` 1.0.1 (~50 sonames).** The same family, plus
     X11/xcb/wayland, PulseAudio/ALSA, `libpython3.12`, `libsystemd` and
     others. Its host qemu is built with SDL/GTK/audio.

Most of these arrive TRANSITIVELY through a host library. For example, the
pokysdk qemu links host `libglib-2.0`, and that library needs `libffi`/`libpcre`.
The gate counts the transitive closure on purpose, because issue 1452 notes
that qemu's `libpcre2` arrives only through `libselinux`.

## Needs a ruling

There are three options:

- Declare the closure. Add the `[prereq.*]` rows, with their apt names per
  release, to each entry's `system`.
- Rule that an UPSTREAM, un-repacked dist (both Zephyr SDK rows; see the
  comment on `[tool.zephyr-sdk]`) is measured against its documented host
  requirements, not its full `ldd` closure.
- Scope the gate to the programs nano-ros actually invokes from these dists.
  Those are the cross compilers, which `smoke` already probes, and not the
  bundled host qemu / gdb-py.

Until that ruling, `dist-runtime-deps` stays lane-exempt. It is reached by
`just workspace doctor`, and putting it on a workflow now would put a
known-red gate on the lane.

## Resolution

Ruled: the Zephyr SDK is UPSTREAM's product, provisioned as a pinned
convenience. A user may bring their own (`ZEPHYR_SDK_INSTALL_DIR`, which
`scripts/lib/zephyr-sdk.sh` honours first) or another toolchain. So nano-ros
does not declare the SDK's full host `ldd` closure; it checks what it RUNS. The SDK's
bundled host qemu is never run (tests use nano-ros's own qemu), so it is
outside that set.

- **The run set is declared, in an existing field.** It is each entry's `smoke`
  probes, the "these must work" set `nros setup` already executes. Both entries
  gained `as` and `ld` rows per triple and a `dtc` row:
  `sysroots/x86_64-pokysdk-linux/usr/bin/dtc` on 0.16.8 and
  `hosttools/sysroots/x86_64-pokysdk-linux/usr/bin/dtc` on 1.0.1 (1.0.x moved
  the host tools under `hosttools/`). Both paths and banners
  (`DTC 1.6.0-dirty` / `DTC v1.7.0+`) were measured on the installed SDKs.
  `front` was not reused: it links a program into `$NROS_HOME/bin`, which would
  put the Zephyr compilers on every user's PATH.
- **`SmokeCheck.hosts`** (new, optional; the `[tool.*]` schema is
  `deny_unknown_fields`). `dtc` is host-shaped: it lives under
  `sysroots/<arch>-pokysdk-linux/`, and macOS gets no host tools at all. Without
  a scope, a probe measured on x86_64 would fail `nros setup` on linux-arm64
  and macOS. The `dtc` rows say `hosts = ["linux-x86_64"]`. The arm64 path is
  unmeasured, so it has no row.
- **The gate.** `check-dist-runtime-deps` holds an UPSTREAM dist (no dist row on
  the nano-ros-sdk mirror; `index_packages.is_upstream_dist`) that DECLARES
  `smoke` to the `DT_NEEDED` closure of those programs
  (`index_packages.run_programs`, `subdir`-resolved and host-filtered). The rest
  of the archive gets ONE note per dist ("upstream dist, host tools not invoked
  by nano-ros: N soname(s) not checked") and never a finding.
  - A declared run program missing from the dist IS a finding.
  - An upstream dist with no `smoke` (`ninja`) keeps the full closure:
    narrowing is opt-in by declaration.
  - Repacked dists keep the full strict closure, unchanged.
  - `scripts/lib/index_packages.py` stays the only reader of the index fields.
- **What the run set needed: nothing.** Every program in both SDKs' toolchain
  directories except gdb links only the base glibc/gcc runtime: gcc, cc1,
  collect2, lto-wrapper, as, ld and the rest of binutils. gdb needs
  `libpython3.8`/`libexpat`/`libz` on 0.16.8 and `libpython3.12`/`libncursesw`/
  `libtinfo` on 1.0.1. `dtc` likewise. So no `system = [..]` row was added.
- **Lane.** The `.config/gate-lane-exempt.txt` row is gone. The gate is on
  `build-serial`, which `gate.yml` runs nightly over the store that job
  provisions. It takes ~1.5 s warm over this host's full store.

| mutation | rc |
| --- | --- |
| clean tree (15 dists, both Zephyr SDKs scoped) | 0 |
| add `arm-zephyr-eabi-gdb-py` to 0.16.8's run set | 1: `libexpat.so.1`, `libpython3.8.so.1.0` |
| drop `libpcre2` from REPACKED `[tool.qemu] system` | 1: `libpcre2-8.so.0` |
| classify every dist as repacked (full closure) | 1: the original 1744 findings for both SDKs |
| ignore the declared roots in `dist_scope` | self-test FAIL (2 rows) |

The self-test runs six store-level rows on the normal path, over a scratch
store of real ELF copies:
- an upstream host tool outside the run set is not a finding, and is counted;
- the same bytes in a repacked dist ARE a finding;
- an upstream run program's own undeclared library is a finding;
- a missing run program is a finding;
- an upstream dist with no `smoke` keeps the full closure.

No compiler or `dtc` needs a declared library, so "drop a library the run set
needs from the declaration" has no real-store subject. The `uprun` self-test
row is that mutation on real bytes, and the `gdb-py` row above is it on the
real store.

### Assumptions that stay

- The pins are 0.16.8 (Zephyr 3.7) and 1.0.1 (4.4).
- 1.0.x uses the `gnu/` toolchain prefix.
- `dtc` sits at the `sysroots` / `hosttools/sysroots` path above.

All three are encoded in the index rows and move with a pin bump. CI's image
uses SDK 0.17.4 for the 3.7 line. Zephyr's compatibility rule allows that,
since `SDK_VERSION` is a minimum and `FindZephyr-sdk.cmake` accepts a newer
SDK. The gate measures the PINNED version in `~/.nros/sdk`, so it does not
measure that image's SDK.
