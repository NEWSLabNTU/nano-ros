---
id: 2581
title: "Public API docs (`pub` rustdoc, module docs, installed headers) cite internal ids"
status: open
type: tech-debt
severity: low
area: [ux, docs, api]
related: [issue-2443, issue-2551, issue-2554, phase-523]
found: 2026-11-15
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
User ruling A (2026-11-12) keeps phase, issue and RFC ids out of
user-reachable text. [Issue 2551](archived/2551-user-surface-text-beyond-function-flow-cites-ids.md)
took them out of the rustdoc that build scripts generate. The docs an app
author reads in `cargo doc`, an IDE hover or a header still carry them.

## What

`check-user-surface-ids` reads literals and the book, not documentation.
Measured on 2026-11-15, with the reading below: **7,631 ids in 555 files**.

| Where | Ids |
|---|---|
| `packages/api/nros-cpp` (headers, mostly) | 2,781 |
| `packages/api/nros-c` (headers and cbindgen's `nros_generated.h`) | 1,453 |
| `packages/core/nros-node` | 986 |
| `packages/core/nros-orchestration-ir` | 456 |
| `packages/rmw/zenoh` | 300 |
| `packages/api/nros` | 270 |
| the rest (about 40 crates) | 1,385 |

The reading: in every crate an app links (everything under `packages/`
except `cli/`, `tooling/` and `testing/`, whose docs never reach an app
author), the `///` lines of a `pub` item (not `pub(crate)`), every `//!`
line, and the comment text of every installed header under `include/`.
Vendored, test, bench, example and build trees are skipped, as the library
subjects skip them.

Over-approximations, kept so the reading stays simple: `//!` of a private
module and `///` of a `pub` item in a private module are counted, though
`cargo doc` does not show them. Gaps, stated: `/** */` docs in Rust, and a
`pub` item inside an inline `#[cfg(test)]` module.

## Plan

1. A counted ratchet, per file, in `.config/api-doc-ids-ratchet.json`:
   a count may only fall, and a fall must be written back with
   `--update-ratchet`, so a sweep cannot be undone unnoticed. (This change.)
2. Sweep, user API first: `nros-cpp` and `nros-c` headers (cbindgen's
   header regenerates from the swept rustdoc), `nros`, then `nros-node` and
   the rest. Each sweep states the fact the id stood for, or moves the id
   to a `//` comment, and checks the lanes and test greps that read the
   changed text.
3. At zero, drop the ratchet file: the subject becomes baseline zero like
   the others (issue 2443's path).

[Issue 2554](archived/2554-generated-humble-drifts-from-emitter.md) is the same
problem in the tracked generated `humble` crates and is swept with it.
