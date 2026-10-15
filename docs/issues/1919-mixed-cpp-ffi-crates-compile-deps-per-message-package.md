---
id: 1919
title: "`artifact-identity-budget` reads 10 copies where it pins 5: the mixed workspace's C++ FFI crates are written by two cmake build roots"
status: open
type: bug
area: [build, cmake]
severity: low
found: 2026-10-15
related: [0499, 0513, 0647, 1832, phase-340]
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
