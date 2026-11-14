---
id: 2532
title: "Release composition: where the launcher is released, and whether the codegen seam is stabilised"
status: open
type: design-question
severity: low
area: [release]
related: [rfc-0097]
found: 2026-11-14
---

**Moved here from RFC-0097's "Open" section** when the RFC became Stable
(phase-555 W3, rule A1(a)). D2/D3's extraction of the ABI-bearing crates was
declined ("NOT scheduled"); D4 and D7 landed (phases 443, 541).

1. **Where the launcher's release lives.** The `nano-ros` release ships the
   toolchain; a launcher meant to outlive it may want its own repository, as
   `nano-ros-sdk` holds tools. D4 does not depend on the answer.
2. **Whether the codegen seam is stabilised on purpose.** 13 changes in 60 days
   is a fast-moving internal interface, which is fine while it is internal. It
   matters only if D3 is revisited, so it is parked with D3.
