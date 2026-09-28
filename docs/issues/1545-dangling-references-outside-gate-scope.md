---
id: 1545
title: "Dangling references in files every gate's scope stops short of — including a book page red on `main`"
status: open
type: bug
area: [docs]
severity: low
found: 2026-09-28
related: [phase-472, 1085]
---

Each class has a gate; each gate's scope stops short of where these live.

- **`book/src/reference/c-api.md:72`** quotes `nros_executor_register_client()`,
  which the tree no longer defines. `check-book-identifiers` is **rc=1 on `main`**
  — VERIFIED — but it lives in `just/check/docs.just`, which no gating lane runs,
  so nothing surfaced it. The page last changed 2026-09-09.
- **24 dead `just` references** outside `check-doc-recipe-refs`'s scope, including
  the retired `build-zenohd` recipe — the gate's own motivating example — in
  `packages/api/nros-c/docs/*.md` and the nros-cpp equivalents, and 13 in
  `tests/README.md`.
- **RFC-0034** (`docs/design/0034-platform-layer-split.md:335`) defines
  `[issue 0006]: ../issues/0006-rtos-dual-heap.md`; the file moved to `archived/`,
  so 8 clickable links 404. Reference-style definitions are unread by
  `check-markdown-links` — issue 1085's shape.
- **6 dangling issue ids** across 11 citations outside `check-prose-issue-refs`'s
  directories (the root `justfile`, `.rs`, `.hpp`, workflows).
- **2 dead ledger citations** — `types.json` and `service.json` cite files that do
  not exist at those paths.

## Fix

Fix the references; widen each gate's scope (phase-472 W5). The book red is a
one-line doc fix and the cheapest item here.
