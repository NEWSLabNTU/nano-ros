---
id: 2533
title: "Vendored source resolution: should an interop override announce itself, and should the unread `links` keys go?"
status: open
type: design-question
severity: low
area: [build]
related: [rfc-0101]
found: 2026-11-14
---

**Moved here from RFC-0101 §8, open questions 1 and 3**, when the RFC became
Stable (phase-555 W3, rule A1(a)). Question 2 (how far D3's gate reaches) was
answered by issue 1560's resolution.

1. **Should an override that lands on a pinned-for-interop tree announce
   itself?** `CYCLONEDDS_SOURCE_DIR` silently replaces a pin that is an interop
   decision (issue 0507). Issue 0500's lesson (read the printed line, never
   infer) argues for printing the origin. Nothing in the tree does it today.
2. **Should the eight unread `links` keys be removed?** RFC-0101 D2 says not to
   declare a key nothing publishes on. Six of the eight are generated message
   crates, whose key codegen emits, so removing them is a codegen change rather
   than eight edits.
