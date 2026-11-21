---
id: 2787
title: "The metadata probe links a node without the census CAN mock, so a node that calls the platform CAN driver cannot be probed"
status: resolved
type: bug
severity: medium
area: [cmake, build, boards]
related: [phase-560, issue-2633]
found: 2026-11-21
resolved: 2026-11-22
resolved_in: "fix(can): the metadata probe links the census CAN mock; every board without a CAN backend gets the fail-loud defaults (2787, 2788)"
---

**Home: [phase-560](../../roadmap/phase-560-contract-revision-follow.md) W9**, beside
the census CAN mock (PR #4491).

## What happens

The census host build links `nros_can_census_mock` when `NROS_LEAF_CENSUS_HOST`
is set (RFC-0132 D6). The per-component metadata probe is a different host
configure, and it links nothing for CAN. A consumer node whose code calls
`nros_platform_can_*` (the safety island's control-command CAN output, moved
onto `<nros/platform_can.h>`) fails there:

    undefined reference to `nros_platform_can_close'

and the probe reports that the node cannot be constructed on its declaration
defaults. The consumer's only workaround is to compile its CAN output out of
the host-probe configure.

## Wanted

The metadata probe links the same mock the census does, so any node that
calls the CAN driver can be probed.

## Acceptance

A probe of a node that opens and sends on the CAN driver succeeds, with the
mock linked; a regression row beside `just check can-census-mock`.

## Resolved, 2026-11-22

The probe project the CLI renders
(`orchestration/metadata_probe_cmake.rs`, `render_probe_cmakelists`) links
every probe executable with `nros_link_can_census_mock(<probe>)`, a function
`cmake/NanoRosCanImage.cmake` now exports: the census mock's C twin and the
device store, as an archive, so a node that never calls the driver links none
of it. `nros_entry_can_image`'s census branch calls the same function, so the
census and the probe link one mock. The probe was a plain `add_executable`,
never a `nano_ros_entry`, so it never reached the census branch; nor does it
set `NROS_LEAF_CENSUS_HOST`, which would also switch every other census hook.

Held by `just check can-census-mock` row C (a host target outside a census
build links the mock through the function and runs against it; the CLI's
probe project names the call) and the unit test
`every_probe_links_the_can_census_mock`. Both fail on the parent commit.
