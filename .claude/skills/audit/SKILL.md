---
name: audit
description: Run the periodic codebase audit (quality / architecture / UX) per docs/development/codebase-audit-checklist.md. Use when the user invokes /audit or asks for a tech-debt / antipattern / UX sweep. Args: [quick|deep] [category letters, e.g. C,I,J]. quick (default) = grep-led triage + reader confirmation; deep = multi-agent fan-out with adversarial verify.
---

# /audit — periodic codebase audit

Automates the audit cycle defined in
`docs/development/codebase-audit-checklist.md`. That checklist is the SSoT
for categories (A–J), detection greps, and severity rules — read it FIRST
each run; never duplicate its content here or drift from it.

Design + decisions: `docs/superpowers/specs/2026-07-16-audit-skill-design.md`.

## Arguments

`/audit [quick|deep] [categories]`

- Depth: `quick` (default) or `deep`.
- Categories: comma-separated checklist letters (`A`–`J`), default all.
  Example: `/audit deep C,I,J`.

## Procedure

### 1. Baseline

- Read the newest `docs/development/audit-findings-*.md` (if any) — the
  previous run's findings are the diff baseline.
- List open + archived issue titles (`docs/issues/`, `docs/issues/archived/`)
  so already-tracked findings are not re-filed.
- `git fetch origin` and note origin's highest issue id across open +
  archived (parallel sessions push to main; ids must not collide).

### 2. Collect candidates

**quick** — for each selected category, run the checklist's detection greps
(where given) and scan the named directories. Spawn a small number of
parallel Explore agents (one per 2–3 categories) to READ each grep hit and
confirm or kill it — the grep is the net, not the verdict. Judgment-only
categories with no grep (C, H, J, F3) get one reader each over their named
surfaces.

**deep** — orchestrate with the Workflow tool: one agent per selected
category, each briefed with that category's checklist text verbatim plus the
severity rubric, returning structured findings
(`category, file, line, severity, summary, fix_sketch`). Then an adversarial
verify stage: for each finding, an independent agent tries to REFUTE it
(wrong reading? already fixed? by-design per an RFC/CLAUDE.md?); majority-
refuted findings are dropped. Pipeline categories → verify (no barrier).

Both depths: every reported finding needs `file:line`, severity (P1–P3 per
the checklist rubric), a one-line statement, and a fix sketch.

### 3. Classify against baseline

Match candidates to the previous findings log + the issue tracker:

- **new** — not in baseline, not tracked.
- **carried-over** — in baseline (or an open issue), still present.
- **resolved-since-last** — in baseline, no longer reproducible.

### 4. Report

Write `docs/development/audit-findings-<YYYY-MM-DD>.md`:

- Header: date, depth, categories run, baseline file, commit sha.
- Findings ranked P1 → P3, each tagged new/carried-over, formatted per the
  checklist (`category · file:line · severity · one-line · fix sketch`).
- A `Resolved since last run` section.
- A short `Coverage` note naming anything skipped (category filter, greps
  that could not run) — no silent truncation.

### 5. File issues

For each confirmed **P1/P2** finding that is not already tracked: create
`docs/issues/<next-id>-<slug>.md` (frontmatter per `docs/issues/README.md`
conventions, next id = max(local, origin) + 1), link the finding, and add the
README index entry. **P3 findings stay in the findings doc** — do not file.

### 6. Land

- `just format` on touched files if any code was changed (normally none —
  the audit only writes docs).
- Commit the findings doc + any new issues locally with a
  `docs(audit): ...` message. **Never push** — the operator pushes.

## Guardrails

- Never fix findings in the same run — report and file only (the operator
  schedules fixes). Exception: none.
- Respect CLAUDE.md pitfalls when reading (e.g. vendored `third-party/`,
  `*/generated/` are out of audit scope except for the C3 boundary check).
- Deep mode is token-heavy; do not auto-escalate quick → deep. If quick
  produces a suspicious-but-unconfirmable finding, note it in the report and
  recommend a targeted deep run (`/audit deep <category>`).
