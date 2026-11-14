---
id: 2529
title: "Build-cache layout: three questions RFC-0070 left open"
status: open
type: design-question
severity: low
area: [build]
related: [rfc-0070]
found: 2026-11-14
---

**Moved here from RFC-0070's "Open" section** when the RFC became Stable
(phase-555 W3, rule A1(a)). R1-R5 landed (phases 334, 340, 344, 350, 383).

1. **Coordinates for the other kinds.** `compile-check`, `install` and the
   `tools/` prefixes are ad-hoc but stable. R5 gave the kind NAME a rule (16 of
   18 kinds follow it); whether those kinds need a coordinate is separate and
   still open.
2. **A per-profile root.** Should `$NROS_BUILD_ROOT` be per-profile at the top
   level rather than under `cargo/`? The models and west trees are
   profile-independent today.
3. **In-place vendored builds.** RFC-0070's step-4 gate needs an allowlist for
   vendored trees that build in place (`third-party/`), which cannot follow R1.
