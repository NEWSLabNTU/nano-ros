# Phase 164 — Reader-Persona Entry Pages + Capability / Board Matrices

**Goal.** Close the four-persona audit gaps surfaced after Phase 163's
book starters revision. Different readers — hobbyist, starter,
serious engineer, business decision-maker — need different entry
points and different artifacts. The book today optimises for one
persona (a starter) and underserves the other three.

**Status.** In progress — 164.A.1 + 164.B.1 + 164.D.1 + 164.F.1 + 164.G.1 landed 2026-05-19. 164.B.2/.3, 164.C.1, 164.E.1, 164.F.2, 164.H.1 pending.

**Priority.** P2 — onboarding quality. Phase 163 nailed the starter
path; this phase widens the funnel to the other three audiences.

**Depends on.** Phase 163 (book starters revision — closed).

---

## Audit findings (2026-05-19 multi-persona pass)

Four agents read the book as personas and reported their friction:

| Persona | Top single fix |
|---|---|
| **Hobbyist** (glance) | "Can I use this right now?" board matrix in intro |
| **Starter** (wants to ship) | "First 10 Minutes" troubleshooting flowchart |
| **Serious engineer** (capabilities) | Production Readiness Checklist |
| **Business reader** (decision-making) | Supplier × Board × RTOS matrix |

Cross-cutting blockers (multiple personas):
- QEMU-only performance numbers (parked in Phase 162 — hardware-gated)
- Cyclone DDS "no services / actions" buried in a footnote
- `rmw-backends.md` opens with "Only one backend can be active at
  compile time" — directly contradicts the multi-backend bridge
  story documented elsewhere
- No persona-tailored landing pages — every reader enters through
  the same `Introduction`
- No comparison vs micro-ROS / rmw_zenoh / ros2_rust for evaluators

---

## Work items

### 164.A — Hobbyist quick-check matrix

- [x] **164.A.1** Add a "Can I use nano-ros right now?" table to
      `introduction.md` (or a new `book/src/start-here/quick-board-check.md`)
      listing common dev boards with concrete status (ESP32-C3,
      ESP32-S3, RP2040, STM32F4-Discovery, MPS2-AN385, Pixhawk 4,
      etc.). Columns: Setup time, Languages supported, Example in
      repo, ROS 2 interop verified. Rows are vendor / board models.

### 164.B — Starter troubleshooting recipe

- [x] **164.B.1** New page `book/src/getting-started/troubleshooting-first-10-min.md`
      with a flowchart of common first-build errors and their fixes.
      Covers: `unresolved import nros`, `Failed to open session`,
      `unknown target`, "build hangs" patterns. Link from every
      starter page's preamble.
- [x] **164.B.2** Every starter page's "Run" section names the
      readiness signal (e.g. "the talker prints `Published: 1`
      within 5 s of `cargo run`; if you see no output, zenohd
      isn't running — start a router first").
- [x] **164.B.3** Add expected stderr / log output for each
      starter so users can compare against "what success looks
      like."

### 164.C — Serious-engineer production readiness checklist

- [ ] **164.C.1** New page `book/src/concepts/production-readiness.md`
      (or `book/src/internals/production-readiness.md`) listing
      hardware-validated RT metrics, platform-specific validation
      checks, RMW certification per target, safety + verification,
      interop, and failure-recovery requirements as a copy-out
      checklist for adoption decisions.

### 164.D — Business supplier × board × RTOS matrix

- [x] **164.D.1** New page `book/src/reference/supported-boards.md`
      with a procurement-grade table: vendor, board model, MCU
      family, RTOS support, nano-ros status (tested / ready /
      untested / unsupported), example link. Surfaces in the
      Reference section.

### 164.E — Comparison vs alternatives

- [ ] **164.E.1** Add a comparison section to
      `book/src/concepts/ros2-comparison.md` or a new
      `book/src/concepts/comparison-vs-alternatives.md` listing
      micro-ROS, rmw_zenoh, ros2_rust, Eclipse Cyclone DDS Lite
      (where applicable). Axes: API surface, supported platforms,
      RT scheduling story, formal verification, license, governance.

### 164.F — Surface Cyclone DDS limitations + cross-RMW story

- [x] **164.F.1** Move the Cyclone DDS "no services / actions",
      "2× CDR roundtrip", "deferred status events" facts out of
      the in-doc footnote and into a dedicated subsection at the
      top of the Cyclone DDS entry in `user-guide/rmw-backends.md`.
- [ ] **164.F.2** Rewrite the cross-RMW bridge subsection in
      `user-guide/rmw-backends.md` (and link to
      `user-guide/cross-backend-bridges.md`) so the multi-backend
      story is front-and-centre.

### 164.G — Resolve "only one backend" contradiction

- [x] **164.G.1** Fix the opening sentence of
      `user-guide/rmw-backends.md`: replace "Only one backend can
      be active at compile time" with the accurate "Each node
      picks an RMW backend at build time; one binary can link
      multiple backends and bridge between them (see
      [Cross-backend Bridges](./cross-backend-bridges.md))."

### 164.H — Persona-tailored entry section

- [ ] **164.H.1** Add a top-of-book "Choose your entry" landing
      page that routes hobbyists, starters, evaluators, and
      decision-makers to the right first page. Replace the bare
      `[Introduction]` with an explicit fork.

---

## Files

### New

- `book/src/start-here/quick-board-check.md` (164.A)
- `book/src/getting-started/troubleshooting-first-10-min.md` (164.B.1)
- `book/src/internals/production-readiness.md` (164.C.1)
- `book/src/reference/supported-boards.md` (164.D.1)
- `book/src/concepts/comparison-vs-alternatives.md` (164.E.1)
- `book/src/start-here/choose-your-entry.md` (164.H.1)

### Modified

- `book/src/introduction.md` — 164.A pointer to quick-board-check.
- `book/src/user-guide/rmw-backends.md` — 164.F + 164.G.
- `book/src/SUMMARY.md` — add new pages to TOC.
- Every Linux + Embedded starter page — 164.B.2 readiness signal +
  164.B.3 expected output.

---

## Acceptance criteria

- [ ] Hobbyist board matrix exists and lists ≥ 8 common boards.
- [ ] Starter troubleshooting page reachable from every starter's
      preamble; covers ≥ 4 common first-build errors.
- [ ] Production readiness checklist exists with ≥ 5 categories.
- [ ] Vendor × board × RTOS matrix exists with ≥ 10 board rows.
- [ ] Comparison table vs micro-ROS / rmw_zenoh / ros2_rust exists.
- [ ] Cyclone DDS limitations surfaced in the first paragraph of
      its rmw-backends entry, not in a footnote.
- [ ] Multi-backend bridge story consistent across rmw-backends.md
      and cross-backend-bridges.md — no contradictions.
- [ ] "Choose your entry" landing page exists with 4 routes.
- [ ] `mdbook build` clean.

---

## Implementation order

Quick wins first (164.G + 164.F.1 + 164.B.2 — typing-level edits),
then the four new artifact pages (164.A / 164.B.1 / 164.D.1 /
164.C.1) in that order of audit-priority impact. 164.E and 164.H
land last.
