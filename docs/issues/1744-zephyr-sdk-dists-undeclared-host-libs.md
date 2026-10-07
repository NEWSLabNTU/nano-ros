---
id: 1744
title: "Both Zephyr SDK dists need host libraries their `system = [..]` does not declare (dist-runtime-deps is red on a clean tree)"
status: open
type: bug
area: [build, tooling, zephyr]
severity: low
found: 2026-10-07
related: [issue-1739, issue-1452, issue-0928, issue-0610]
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

## Resume checkpoint (2026-10-08)

Work paused for token budget. **The ruling (2026-10-07):**
- The Zephyr SDK is upstream's. We provision a pinned convenience copy; a user may bring their own via `ZEPHYR_SDK_INSTALL_DIR`, which `scripts/lib/zephyr-sdk.sh` honours first.
- We do NOT declare its full `ldd` closure. `check-dist-runtime-deps` checks only what nano-ros RUNS from it: the cross gcc/binutils, plus `dtc` from `sysroots/x86_64-pokysdk-linux/usr/bin/dtc` (measured in an mps2/an385 build cache).
- The SDK host qemu is unused; tests use our own qemu.
- Everything else in the SDK gets one NOTE pointing at upstream's host requirements. Repacked dists stay fully strict.
- Add a `dtc` smoke row, and put the gate back on a lane.

At pause, a WIP commit `38a90860c8` "wip(#1744): scope dist-runtime-deps for upstream dists to their run set" sits UNPUSHED on branch `fix/1744-zephyr-sdk-run-set`, in worktree `/mnt/mx500/aeon/nros-worktrees/p1742`. To resume from that worktree:
1. Finish the steps above.
2. Prove by mutation, push, open a PR, and merge.

The version assumptions stay:
- 0.16.8 (3.7 line) and 1.0.1 (4.4 line);
- the `gnu/` layout prefix in 1.x;
- CI's image uses 0.17.4 for 3.7, which Zephyr's compatibility rule allows (a 1.x SDK refuses trees that ask for less than 1.0).
