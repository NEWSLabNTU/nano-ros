---
id: 1919
title: "`artifact-identity-budget` reads 10 copies where it pins 5: the mixed workspace's C++ FFI crates are written by two cmake build roots"
status: resolved
type: bug
area: [build, cmake]
severity: low
found: 2026-10-15
resolved: 2026-10-16
related: [0499, 0513, 0522, 0616, 0647, 1832, 1922, phase-340]
---

## Measured

On a `just build-test-fixtures lane=tier2` tree (origin/main plus phase-497
W1, which touches no cmake, codegen or FFI layout), the second
`build-test-fixtures` run in the same worktree stopped at its `check::fast`
pre-pass:

```
artifact-identity budget: FAIL
  counted 286 of 286 rlib(s) — those written since started_at=2026-10-15T06:13:45Z (issue 0499).
  identities written into more than 5 target dirs in examples/workspaces/mixed/build:
  byteorder 40539fc21caa10e9: 10 copies
  hash32 610706771588f232: 10 copies
```

The ten copies of each identity are two sets of five, one per cmake build root:

- `examples/workspaces/mixed/build/nros-metadata/metadata-probe-cmake/build/nros-codegen/nano_ros_cpp_ffi_<pkg>/target/…`
  for `action_msgs`, `builtin_interfaces`, `example_interfaces`, `std_msgs`
  and `unique_identifier_msgs`;
- `examples/workspaces/mixed/build/threadx-linux-zenoh/cmake/pkg/<pkg>/nano_ros_cpp_ffi_<msg>/target/…`
  for the same five message packages.

## Why it is a regression and not a recount

`scripts/check-artifact-identity-budget.sh` pins `CEILING_COPIES=5`, and its
header says why five: *the five `src/*/nano_ros_cpp_ffi_*/target/` trees each
write their own copy of the SAME hash* (phase-340 item 8, lowered 12 → 6 → 5 on
a rebuilt tree). That was ONE cmake root. Today two roots each write all five:
the metadata probe's own cmake configure
(`build/nros-metadata/metadata-probe-cmake`) and the image build
(`build/<image>/cmake`). The same compilation is now written ten times.

## Why nothing caught it

The gate needs a built tree, and no CI lane builds the `mixed` workspace's
threadx-linux image (issue 1832 is that class). Locally it passes on the FIRST
build of a worktree, because the window then holds only fresh artifacts; it
fails on the next `build-test-fixtures`, whose pre-pass reads the previous
build's rlibs.

## Direction

Either the metadata probe reuses the image build's FFI crate target dirs (one
cargo target dir per FFI crate root, issue 0616's rule, and the probe and the
image are the same root), or the budget is re-measured with the probe counted
and the reason written next to the number. Re-measure before choosing; do not
raise the ceiling without the second root named in the comment.

## Resolution (2026-10-16): a recount after all, not a regression

Re-measured as the Direction asks, and the title's premise does not hold.

- **The build did not change; the gate's reach did.** The probe has run its
  own cmake configure, FFI crates included, since its C++ driver landed
  (`c803fcfdae`, 2026-07-27), before `CEILING_COPIES=5` was recorded
  (2026-08-07). That ceiling was read on `build-workspace-fixtures*`, a tree
  the probe never wrote into. The gate first read `<ws>/build` on 2026-10-15
  (`b802401930`), and this issue was filed after it.
- **The two sets of five are two cargo roots by construction.** The probe
  and the image are separate cmake projects, and each FFI crate's manifest
  sits in its own project's binary dir, so the probe's
  `nano_ros_cpp_ffi_std_msgs` and the image's are different workspace roots.
  Issue 0616 forbids one target dir serving two roots. Sharing would need
  one crate root, written by the probe's configure and by every image's,
  which couples projects that today regenerate their FFI sources
  independently. In the lanes it would also save nothing, because the probe's
  tree is deleted before any image builds (next point).
- **The lanes do not keep it.** `fixtures-build.sh` and
  `workspace-fixtures-build.sh` set `NROS_METADATA_PROBE_CACHE=0`, which
  deletes the probe's build tree after a successful probe (issue 0522). A
  developer's `nros sync` keeps it, and that is where the duplicate sits.
  The lanes see it only after a probe that failed (kept as evidence), which
  is how this issue's tier-2 tree had it.

Measured on the mixed workspace: after the C/C++ sidecars were moved aside,
`nros sync` re-probed 6 components and kept the tree. Main's gate then failed
at 10 copies (`nros_serdes 047aafddc7b57a18`, `heapless 5d75f2c4b3b8cbf2`),
five under `posix-zenoh-linux/cmake/pkg/*/nano_ros_cpp_ffi_*` and five under
`nros-metadata/metadata-probe-cmake/build/nros-codegen/nano_ros_cpp_ffi_*`.

So the gate changed, not the build: issue 1922's copy axis gives each build
root its own ceiling of 5. The probe root is still read, so a sixth copy
inside it fails. On that same tree the gate passes: worst identity 5/5 in
any root.
