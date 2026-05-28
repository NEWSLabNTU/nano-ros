# Phase 188 — Book front-door presentation

**Goal.** Make the deployed mdBook (`NEWSLabNTU.github.io/nano-ros-book/`)
land a strong first impression in the first 30 seconds — *show*, not just
*tell* — and remove front-door drift. The persona funnel
(`start-here/choose-your-entry.md`) is already good and stays as-is; this
phase reworks the landing page (`introduction.md`) and the architecture
entry point.

**Status.** 188.A DONE (2026-05-28); `mdbook build book` clean (mermaid
renders, no broken links). Workstreams B (visual identity) + C (funnel/deploy
hygiene) scoped but deferred to a follow-up.

**Priority.** P2 — presentation / adoption. No product capability depends
on it, but the book is the public front door and the GitHub-Pages site is
where evaluators land first.

**Depends on.** Nothing in-code. Touches only `book/src/`. Mermaid is
already wired (`book/book.toml` → `[preprocessor.mermaid]` + `additional-js`).

---

## Overview

The book is content-rich (113-line `SUMMARY.md`, ~60 pages) and the
persona-based `choose-your-entry.md` funnel is strong. The weak spots are
all on the **landing page**:

- The intro **tells** (feature bullets + tables) but never **shows** a code
  snippet — bad for the "5-minute glance" persona the funnel explicitly
  targets.
- Mermaid is loaded but used in exactly one page (`concepts/two-layer-api.md`);
  the intro and `concepts/architecture.md` are wall-of-text/table with **zero
  diagrams**.
- **Drift:** intro Key Features said "**Dual** middleware — Zenoh or
  XRCE-DDS" while the RMW Backends section below it lists **three** backends
  (adds Cyclone DDS). The "dual" framing predates Cyclone becoming a
  first-class backend.
- The "Project Status" section is a vague "under active development"
  paragraph; the README already carries a crisp feature-status table that
  the maturity-signal persona actually wants.

## Architecture

All changes are confined to `book/src/`. No theme, CSS, or workflow change
in Workstream A — those are Workstream B. The hero code snippet mirrors the
canonical `examples/native/rust/talker/` (`src/lib.rs::run()`), trimmed to
the minimal idiomatic publisher so it stays copy-pasteable and drift-checkable
against a real example.

## Work Items

### 188.A — Front-door content (active)

- [x] **188.A.1 — Fix the "dual middleware" drift.** `introduction.md` Key
  Features: replace "Dual middleware — Zenoh or XRCE-DDS" with a
  three-backend "pluggable middleware" framing (Zenoh, XRCE-DDS, Cyclone
  DDS), consistent with the RMW Backends section lower on the same page.
- [x] **188.A.2 — Add a hero code snippet.** Insert a minimal Rust talker
  (~16 lines, faithful to `examples/native/rust/talker`) right after the
  opening paragraph of `introduction.md`, so an evaluator sees the API
  immediately. Link to the per-language First Node pages.
- [x] **188.A.3 — Add an architecture diagram.** A compact mermaid
  layer/flow diagram (app → core → RMW → platform, with the wire-compat
  edge) on the landing page, surfaced near the top. Lists the three real
  backends (no retired `dust-dds`).
- [x] **188.A.4 — Tighten "Project Status".** Replace the vague paragraph
  with the feature-status table (mirrors `README.md`), giving the
  maturity-signal persona a real answer.

**Files**
- `book/src/introduction.md` (all four items)

### 188.B — Visual identity (deferred follow-up)

- [ ] **188.B.1** Favicon + logo (`book/theme/`), wired via
  `[output.html]` so the site stops looking like a stock mdBook.
- [ ] **188.B.2** Accent CSS (`book/theme/custom.css` via `additional-css`).

### 188.C — Funnel + deploy hygiene (deferred follow-up)

- [ ] **188.C.1** Dead-link sweep of `choose-your-entry.md` + `SUMMARY.md`
  after the Phase 168/180 renames.
- [ ] **188.C.2** Set the GitHub repo `homepageUrl` to the book site so the
  repo → website funnel exists (currently empty).

---

## Acceptance

- A first-time visitor to `introduction.md` sees a working code snippet and
  an architecture diagram above the fold, plus an accurate feature-status
  table.
- No "dual middleware" / two-vs-three-backend contradiction remains on the
  landing page.
- `mdbook build book` succeeds (mermaid renders, no broken links introduced).

## Notes

- The hero snippet is intentionally a trimmed copy of a real example, not a
  bespoke sample — if the public API shifts, the example breaks first and
  this phase's snippet is the obvious next edit.
- Workstreams B and C were scoped in the same review but split out: B needs
  a logo design decision, C spans the GitHub repo surface. Neither blocks A.
