---
name: audit
description: Run the periodic codebase audit (quality / architecture / UX / gate reach) per docs/development/codebase-audit-checklist.md. Use when the user invokes /audit or asks for a tech-debt / antipattern / UX sweep, or a gate-reach re-audit. Args: [quick|deep|gates] [category letters, e.g. C,I,J]. quick (default) = grep-led triage + reader confirmation; deep = multi-agent fan-out with adversarial verify; gates = category K by mutation (phase-472's method).
---

# /audit — periodic codebase audit

Automates the audit cycle defined in
`docs/development/codebase-audit-checklist.md`. That checklist is the SSoT
for categories (A–K), detection greps, and severity rules — read it FIRST
each run; never duplicate its content here or drift from it.

Design + decisions: `docs/superpowers/specs/2026-07-16-audit-skill-design.md`.

## Arguments

`/audit [quick|deep] [categories]`

- Depth: `quick` (default), `deep`, or `gates`.
- Categories: comma-separated checklist letters (`A`–`K`), default all
  (`gates` implies `K`).
  Example: `/audit deep C,I,J`.

Deep mode spawns several agents. Invoking `/audit deep` is the opt-in for
that; never escalate to it on your own (see Guardrails).

## Procedure

### 1. Baseline

- Read the newest `docs/development/audit-findings-*.md` (if any) — the
  previous run's findings are the diff baseline.
- List open + archived issues so already-tracked findings are not re-filed:
  `just issues [text]` (offline, ~20 ms), plus `docs/issues/archived/`.
- Work in a worktree branched from a fresh `origin/main` (`git fetch origin`),
  never in a dirty main checkout.
- Where a checklist item names a gate, the item is enforced: do not re-judge
  its grep hits — ask the K question of the gate instead (checklist "A gate
  beats a grep").

### 2. Collect candidates

**quick** — for each selected category, run the checklist's detection greps
(where given) and scan the named directories. Spawn a small number of
parallel Explore agents (one per 2–3 categories) to READ each grep hit and
confirm or kill it — the grep is the net, not the verdict. Judgment-heavy
categories (C — incl. the C5 axis-agnosticism, C6 user-API-shape-vs-rclc/
rclcpp/rclrs/ament, and C7 rmw.h-vtable-mirror conformance reads — plus H,
J, F3, and the E5–E9 test-system sweep — coverage-matrix cross-read,
matrix-derivation, output-marker/isolation discipline, launch convention +
micro-test budget) get one reader each over their
named surfaces; the API-shape items compare against the upstream ROS
convention, so brief the reader to name the standard counterpart for every
flagged divergence.

**gates** — the checklist's category K, by mutation, as
`docs/roadmap/archived/phase-472-gate-reach-sweep.md` ran it: per gate, plant
the defect it names in a scratch tree (its own worktree when agents run in
parallel, so mutations cannot collide), run it the way its lane does, restore,
and refuse to continue unless `git status` is back at baseline. rc≠0 → CLEAN;
rc=0 → run a control before calling it a HOLE. Report a table per gate
(gate · mutation · rc · control rc · verdict) like the 2026-10-07 log.

**deep** — orchestrate with the Workflow tool. Economy principles: the MAIN
loop runs every detection grep first and hands each agent a pre-scoped brief
(hit list + exact paths — agents read and judge, never locate); categories
bundle by SURFACE so each file is read once; verification is tiered, single-
refuter (no vote panels). Finder lanes, models, and efforts:

| lane | categories | surface | model | effort |
| --- | --- | --- | --- | --- |
| cmake | A, C4, G2-cmake | `cmake/`, board/platform tomls | sonnet | low |
| core-code | B1/B3/B5/B6/B7, C5, I3/I5 | `packages/core` (pre-filtered) | sonnet | medium |
| API-shape | C1/C2, C6, C7, D | C/C++/Rust public API + vtable vs rclc/rclcpp/rclrs/rmw.h/ament | inherit | high |
| testing | E1–E9 | `packages/testing`, `fixtures.toml`, `.config/nextest.toml`, coverage matrix | sonnet | medium |
| docs/UX | F, H | book, justfile, activate | haiku | low |
| examples | J, I1/I2 | `examples/` sample | sonnet | medium |

Rules: never pay the flagship for locating, only for judging (API-shape is
the one comparison-against-external-convention task — it gets inherit+high);
haiku only where a wrong answer is cheap to catch on read (doc drift);
effort=low for mechanical confirmation lanes.

Each finder returns structured findings
(`category, file, line, severity, summary, fix_sketch, confidence`).
Verify stage, pipelined per finding (no barrier), ONE refuter each:
P1 → always refute (model inherit, effort low); P2 → refute only when the
finder's confidence is low or the fix sketch is expensive (sonnet, low);
P3 → report-only, unverified. A refuted finding is dropped with its
refutation noted in the report.

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

For each confirmed **P1/P2** finding that is not already tracked: reserve the
id with `just issue-new <slug>` — never by reading the highest number, which
races parallel sessions (seven collisions) — then fill in the file it creates
(frontmatter per `docs/issues/README.md`) and link the finding. Do NOT edit an
issue index: `docs/issues/open.md` is a generated, gitignored artifact (issue
0884). **P3 findings stay in the findings doc** — do not file.

### 6. Land

- `just format` on touched files if any code was changed (normally none —
  the audit only writes docs).
- Commit the findings doc + any new issues on the audit branch with a
  `docs(audit): ...` message, staging explicit paths (never `git add -A`).
- `main` takes no direct pushes (ruleset + merge queue). Push the branch and
  open a PR only when the operator asks; then `just ci gate` first, `gh pr
  merge --auto --rebase`, and confirm `autoMergeRequest` is set.

## Guardrails

- Never fix findings in the same run — report and file only (the operator
  schedules fixes). Exception: none.
- Respect CLAUDE.md pitfalls when reading (e.g. vendored `third-party/`,
  `*/generated/` are out of audit scope except for the C3 boundary check).
  Count what the tree holds with `git ls-files` / `git grep`, never a `find`
  from the checkout root (issue 1565).
- The checklist is itself audited (H1): an item citing a path, phase or gate
  that no longer exists is a P3 finding — report it in the findings doc rather
  than silently skipping the item.
- Deep mode is token-heavy; do not auto-escalate quick → deep. If quick
  produces a suspicious-but-unconfirmable finding, note it in the report and
  recommend a targeted deep run (`/audit deep <category>`).
