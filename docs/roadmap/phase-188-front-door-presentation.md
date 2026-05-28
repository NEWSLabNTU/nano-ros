# Phase 188 — Book front-door presentation

**Goal.** Make the deployed mdBook (`NEWSLabNTU.github.io/nano-ros-book/`)
land a strong first impression in the first 30 seconds — *show*, not just
*tell* — and remove front-door drift. The persona funnel
(`start-here/choose-your-entry.md`) is already good and stays as-is; this
phase reworks the landing page (`introduction.md`) and the architecture
entry point.

**Status.** 188.A + 188.C DONE (2026-05-28); `mdbook build book` clean
(mermaid renders, no broken links). Workstream B (visual identity) scoped
but deferred (needs a logo decision).

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

### 188.D — Accuracy follow-on + README badges

- [x] **188.D.1 — Architecture-page backend drift.**
  `concepts/architecture.md` RMW-layer block listed "Zenoh, XRCE-DDS,
  **DDS**, Cyclone DDS" — the standalone "DDS" was retired dust-dds
  (Phase 169). Dropped it → "Zenoh, XRCE-DDS, Cyclone DDS, or a custom
  backend". (Checked `concepts/no-std.md` too: its "all **four** backend
  crates" is correct — the table lists exactly 4 zenoh/xrce-family crates,
  not dust-dds. Left as-is.)
- [x] **188.D.2 — README badges.** Added a badge row under the title: CI
  (Actions `ci.yml`), Book (→ site), `no_std`, Rust edition 2024, ROS 2
  Humble | Iron. **License badge intentionally omitted** — see note.

**Files**
- `book/src/concepts/architecture.md`, `README.md`

> **License inconsistency — RESOLVED in 188.E below.**

### 188.E — License resolution (dual default + ROS carve-outs)

- [x] **188.E.1 — Policy.** Confirmed house default `MIT OR Apache-2.0`
  (already the root `[workspace.package]` value, and 153 of 181 tracked
  crates). Audited all 181: 15 declared `Apache-2.0`, 13 declared nothing.
- [x] **188.E.2 — Carve-outs kept Apache-2.0** (genuinely derived from
  Apache-2.0 ROS 2 sources): `rcl-interfaces`, `lifecycle-msgs` (generated
  from ROS msgs), `nros-c` (rclc-compatible C API).
- [x] **188.E.3 — Fixed 12 drifters → `MIT OR Apache-2.0`:** `nros`,
  `nros-cpp`, `nros-sizes-build`, `zpico-alloc`, the 7 `examples/zephyr/rust/*`,
  and `multi-package-workspace/.../pkg_rust_publisher` (original code with no
  Apache-only reason; the zephyr examples had drifted from the template while
  every other example tree was already dual).
- [x] **188.E.4 — Filled 6 license-less hand-written crates** with the dual
  SPDX (`book/rustdoc-driver`, the two `native/rust/*-async` examples,
  `nros-tests`, `nros-nuttx-ffi`, the `nros-serdes` cmake template).
  **Skipped generated crates** (`rcl-interfaces/generated/*`,
  `wake-latency-cortex-m3/generated/*`) per the don't-modify-generated rule,
  and the internal `tests/simple-workspace/*` fixtures.
- [x] **188.E.5 — Added root `LICENSE-MIT` + `LICENSE-APACHE`** (was none →
  GitHub `licenseInfo: null`) and expanded the README License section
  (file links + carve-out note + contribution clause).
- [x] **188.E.6 — README license badge** can now be added safely; deferred to
  the same follow-up as the rest of the badge row unless requested.

All 18 touched manifests pass `cargo verify-project` (the cmake template is
valid TOML; cargo only rejects its non-`Cargo.toml` filename). The 128
already-correct hardcoded-dual crates were left untouched (consistent with
the examples that must hardcode — standalone example crates can't inherit
`license.workspace`).

### 188.B — Visual identity (deferred follow-up)

- [ ] **188.B.1** Favicon + logo (`book/theme/`), wired via
  `[output.html]` so the site stops looking like a stock mdBook.
- [ ] **188.B.2** Accent CSS (`book/theme/custom.css` via `additional-css`).

### 188.C — Funnel + deploy hygiene

- [x] **188.C.1** Dead-link sweep of `choose-your-entry.md` + `SUMMARY.md`
  after the Phase 168/180 renames. **Clean** — 0 broken across 29 + 81
  `.md` link targets (verified `tmp/linkcheck.sh`); no fixes needed.
- [x] **188.C.2** Set the GitHub repo `homepageUrl` to the book site so the
  repo → website funnel exists (was empty). Also filled the empty
  **description** and added **20 topics** (`ros2`, `embedded`, `rust`,
  `no-std`, `rtos`, `zephyr`, `freertos`, `nuttx`, `threadx`, `dds`,
  `zenoh`, `cyclonedds`, `micro-ros`, `cortex-m`, `esp32`, `riscv`, …).
  Homepage → `https://newslabntu.github.io/nano-ros-book/`.

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
