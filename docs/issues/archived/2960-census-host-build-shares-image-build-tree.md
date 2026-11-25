---
id: 2960
title: "The census host build shared the image's `build/<coord>/cmake` while configured as a different program, so each `nros build` of the image relinked the census binary and cross configures read the census stale"
status: resolved
type: bug
severity: high
area: [cli, census, ci, fixtures]
found: 2026-11-25
resolved: 2026-11-25
related: [1419, 1252, 0616, 2378, 2400, 2461, phase-530]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md) W10.**
Release blocker B32 for nros12.

## What happened

In run-matrix 45422927491 (main at 730dfbebeb, the `tier2-nightly` job,
runner 118), four fixture rows failed to build:
`workspace-cpp-threadx-linux-{service-server,service-client,action-server,action-client}-cyclonedds`
in `examples/workspaces/cpp`. Two tests downstream of them failed as well.
Each row stopped at CMake configure with the same refusal:

```
nano_ros_entry(threadx_service_server_cyclonedds_entry): the census and the
contract disagree, or the census no longer describes the code.

Error: census stale: binary
`build/posix-zenoh-linux-service_bringup-linux_service_server/cmake/linux_service_server_entry`
changed since the census was taken (recorded sha256:db97ac07…, now sha256:85b9fb7f…)
```

A cross image checks its census against the NATIVE image built from the same
launch file. The census records that binary's sha256 as a freshness input
(`collect_inputs`, role `binary`, issue 1419). `census-prepass.sh` takes every
census before the platform stages start in parallel. The `linux` stage then
builds those same native images as its own fixture rows (runner 118's
linux.log, line 5414 onward: configure and build of the cpp
`posix-zenoh-linux-service_bringup-linux_service_server` dir).

## Cause (measured)

The census `take` built the native host image into the SAME directory as
`nros build <bringup>:linux` and the native fixture row, `build/<coord>/cmake`.
It configured that image under `NROS_LEAF_CENSUS_HOST`, which makes the
configure a different program:

- `cmake/NanoRosCanImage.cmake` links the census CAN mock instead of the
  board's CAN defaults;
- `cmake/NanoRosResolved.cmake` reads no resolved seed;
- `cmake/NanoRosCensusStep.cmake` and `NanoRosLeafEntityFacts.cmake` skip
  their steps.

So each build flipped the tree to the other program. Measured on main
`fe007626ed`:

- census take, then the native row's `nros build`: the row re-ran the
  configure, regenerated the entry sources and relinked
  `linux_service_server_entry` (mtime 02:29:31 → 02:31:40);
- a second row build right after it: no configure, no relink. `nros build` is
  stable on its own, and only the take disturbs it;
- here the two programs came out byte-identical (sha256 `554a7eb0…`). On
  runner 118 they did not (`db97…` census, `85b9…` after the linux stage),
  so the census was stale after the linux stage even with no race.

A second, smaller difference: `nros build` gives every cmake configure
`-DNROS_IMAGE_STACK_FLOOR_{IMAGE,MODE}` (`stack_floor_raise::arm_cmake`,
issues 2400/2461) and arms the cargo road's stack walk (`stack_floor::arm`,
RUSTFLAGS/CFLAGS in the build env), and `take` did neither.

Not #4798 (the threadx-linux stack floors), the first suspect: the rows stop at
configure, before any threadx code compiles.

## Fix

User ruling 2026-11-25, recorded in RFC-0120 D1: the census host build gets
a build tree of its own, one configuration per build tree (issue 0616's rule
for cargo target dirs).

- `take` moves its CMake plans' `-B`/`--build` to
  `build/nros/census-image/<coord>` (`census_image_build_dir`), and builds,
  finds the binary and writes the R42 host-build record there. The census
  records, and the check hashes, the binary it actually ran. The image's
  `build/<coord>/cmake` is never touched by a take, and `nros run`, the fixture
  resolvers and `workspace_artifact_dir` keep reading it.
- The name does not share a namespace with the owned-entry road
  (`build/nros/census-host/<bringup>/<image>`) or the leaf road
  (`<leaf>/build/census-host`).
- A host-build record that still names `build/<coord>/cmake` (written before
  this) is treated as no record, so the census is re-taken into its own tree
  instead of being rebuilt in the image's.
- `nros build` and `take` arm their plans through one helper,
  `arm_for_real_build`.

Tests (`cmd/entity_census.rs`): the tree per coordinate; a rebuilt shared tree
leaves the census fresh while a change to the census tree's binary makes it
stale; an old shared-tree record is not re-run (mutation-checked: with the
filter removed it runs the recorded step).

Measured end to end on host 142 (see the PR): take, then the native row build,
then the threadx cyclonedds row.

Follow-up, release blocker B35 (found on a build tree from before the fix):
an old census was NOT re-taken. It recorded the image's shared binary,
`build/<coord>/cmake/<entry>`, and while nothing had rebuilt that tree it
hashed equal and read fresh, so `take --image` (the fixture prepass, `nros
build`) skipped it. The image's own row build then moved the bytes, the census
went stale, and the cross configure and the in-build step refused it: the
host-build record that could have re-taken it is ignored on purpose (above),
so nothing would. Every persistent runner tree had such censuses. Fix:
`census_freshness` reads a census whose `binary` input is in that layout as
stale whatever its bytes (`binary_in_shared_tree_layout`), so the take that
runs first re-takes it into `build/nros/census-image/<coord>`. The owned and
leaf roads' census binaries never match. Tests: the old layout reads stale
with its bytes unchanged and a current-layout census reads fresh
(mutation-checked); one case per road for the predicate.

Follow-up, not measured: the cargo road keeps a shared tree. If take then
`nros build` relinks a Rust workspace image, it needs the same split.

Follow-up, landed (census review): the retarget refused nothing. A plan
whose configure or build command spelled the image's tree any other way
than `cmake_build_dir` (absolute, a trailing slash) kept building into
`build/<coord>/cmake` while the take recorded the census tree, which is
this issue again with no message. `retarget_to_census_tree` now returns an
error naming the step, the plan and both dirs when a command present on
the plan names the image's tree in no argument. Test:
`the_census_retarget_moves_an_exact_spelling_and_refuses_any_other` (the
exact spelling moves both commands; an absolute `-B` and a trailing-slash
`--build` are each refused).
