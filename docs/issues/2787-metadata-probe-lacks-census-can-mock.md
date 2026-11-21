---
id: 2787
title: "The metadata probe links a node without the census CAN mock, so a node that calls the platform CAN driver cannot be probed"
status: open
type: bug
severity: medium
area: [cmake, build, boards]
related: [phase-560, issue-2633]
found: 2026-11-21
---

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md) W9**, beside
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
