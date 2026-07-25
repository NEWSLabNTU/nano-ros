<!--
Copyright (c) 2026, NEWSLab NTU.
SPDX-License-Identifier: Apache-2.0

AGENTS.md TEMPLATE — copy this into your fork/workspace root as `AGENTS.md`
(and/or `CLAUDE.md`) so your AI agents follow the same pipeline we do.

This is the *contributor agent* guide. It is deliberately short: it says who
does what, names the five-stage pipeline, and states the one hard rule about
tests. Project-specific command names are marked {{LIKE_THIS}} — replace them
(nano-ros defaults are shown in comments). Keep the pipeline + roles + test
rule intact; that is the part that makes many agents on one codebase sane.
-->

# AGENTS.md — contributor agent guide

We develop heavily with AI agents, and we welcome human contributors equally.
This works because the *labor is split by what each side is good at* and every
durable decision is written down in a known place.

## Division of labor

Agents are excellent at **writing and changing code**. Humans are better at
**deciding what is correct**. So:

| Stage | Owner | Why |
| --- | --- | --- |
| RFC design | **Human-led** | judgment, trade-offs, API taste, cross-cutting intent |
| Implementation survey | **Agents** (fan out) | reading the whole tree fast, mapping touch-points |
| Phase implementation | **Agents** (fan out) | writing the code — the part already better than human |
| Testing | **Human-led** | deciding what "correct" means; tests are the contract |
| Issue triage | Agents draft · **human triages** | agents spot + record; humans prioritize |

**Agents propose, humans dispose.** An agent never invents a design decision,
relaxes a test, or merges its own work.

## The pipeline

```
RFC  ──▶  (impl survey)  ──▶  phases  ──▶  (test)  ──▶  issues
design      work breakdown      code        proof       findings
```

Each stage has a fixed input, output, and home in the docs. Each stage can run
**many agents in parallel** (see "Fan-out", below).

### 1. RFC — the design decision *(human-led)*

A design decision becomes a numbered, living RFC in `docs/design/NNNN-slug.md`.
The RFC owns the *rationale*: the problem, the option space, the chosen shape,
and the open questions. Status moves `Draft → Stable → Superseded`.

```yaml
---
rfc: 0000
title: "Short decision title"
status: Draft          # Draft → Stable → Superseded
since: YYYY-MM
implements-tracked-by: []   # phase doc slugs that carry the work
supersedes: []
---
## Summary        — what & why, one paragraph
## Motivation      — forces, constraints (no_std, RTOS, wire-compat…)
## Design          — interfaces, shapes, invariants
## Alternatives    — what else was on the table, why it lost
## Open questions   — numbered; empty when status is Stable
## Changelog
```

> **Rule — rationale lives in an RFC, never only in a phase doc.**
> **Drift rule — flipping an RFC to `Stable` updates the matching
> `ARCHITECTURE.md` section in the same commit.**

### 2. Implementation survey — turn the RFC into a plan *(agents, fan out)*

Before code, agents survey the codebase against the RFC and produce a **work
breakdown**: which files/subsystems are touched, in what order, with what
acceptance test per item. The output is a **phase doc** in
`docs/roadmap/phase-NNN-slug.md` that names the RFC it implements.

This is a natural fan-out: one survey agent per subsystem / RMW / platform; a
human (or a synthesis agent) merges their findings into the phase doc.

### 3. Phases — write the code *(agents, fan out)*

The phase doc is the queue. Work items are numbered checkboxes; agents pick them
up, implement, and check them off with a one-line proof.

```markdown
# Phase NNN — <title> (implements RFC-MMMM)

**Goal.** Implement RFC-MMMM: <one sentence>.
**Status.** In progress (YYYY-MM). NNN.1 DONE.
**Depends on.** RFC-MMMM, <prereqs>.

## Work breakdown
### NNN.1 — <item> — DONE
- [x] <change> — **proof:** <test/script that passes>
### NNN.2 — <item>
- [ ] <change>
- [ ] <acceptance script to add>

## Acceptance Criteria
- [ ] <observable, scriptable outcome>
```

One agent per work item. Use isolated git worktrees when agents touch files in
parallel; never put two agents on the same file at once.

### 4. Test — prove it, mechanically *(human-led)*

See the hard rule below. A phase item is **not done** until a mechanical script
proves it. Humans own what gets tested; agents may write the script, but the
*decision of what "correct" means* is a human/RFC call.

### 5. Issues — record what's broken or missing *(agents draft, human triages)*

A bug, limitation, or tech-debt found anywhere becomes an issue in
`docs/issues/NNNN-slug.md`. Issues cross-link the RFCs/phases that inform or
close them.

```yaml
---
id: 0
status: open           # open → resolved | wontfix
type: bug              # bug | enhancement | tech-debt
area: codegen          # codegen | rmw | memory | cmake | zephyr | nuttx | …
related: [rfc-0000, phase-000]
resolved_in:           # phase or commit, set when resolved
---
```

Resolved issues set `status` + `resolved_in` and move to `docs/issues/archived/`.

## Fan-out — many agents per stage

The pipeline is built for parallelism. Within a stage, split work so agents
don't collide:

- **Survey:** one agent per subsystem / platform / RMW; merge into one phase doc.
- **Implementation:** one agent per work item; isolate with a git worktree when
  edits overlap; serialize only on shared files.
- **Issues:** agents file independently; a human dedupes and prioritizes.

Each agent returns a **conclusion + the proof** (a passing script, a file:line,
a checked box), not a narrative. The next stage consumes the artifact, not the
chat.

## Tests are mechanical scripts — the one hard rule

A test is a **script that mechanically checks a fact**. Not a vibe, not a
read-through, not "looks right". This is what lets agents move fast without a
human re-reading every diff.

- **A script, every time.** Reusable checks live in the test suite
  (`{{TEST_DIR}}`  ·  nano-ros: `packages/testing/nros-tests/tests/` for Rust,
  `tests/` for shell). A green script is the proof a work item is done.
- **Fail on unmet preconditions.** Use `assert!` / `bail!` / the project skip
  helper. A bare `eprintln!` + `return` reports PASS — never do that. Same at
  runtime: panic, don't silently early-return.
- **No compilation inside a test.** Never `cargo`/`cmake`/`west`/`idf.py` build
  at run time. Compile in the build stage (fixtures); the test consumes the
  prebuilt artifact. "Does it compile?" → make it a build-step fixture and
  assert the artifact exists.
- **Name by behavior, not phase number.** `rust_talker_to_cpp_listener_delivers`,
  not `phase212_n9_*`. Phases go stale; cross-ref a phase in a comment, never
  the identifier.

If you can't express the check as a script, the work item isn't specified yet —
push it back to the RFC/phase, don't merge on faith.

## Commands

Replace with your project's recipes (nano-ros defaults shown):

- `{{CHECK}}`        — format + lint all surfaces   ·  nano-ros: `just check`
- `{{TEST_FAST}}`    — fast dev test tier            ·  nano-ros: `just test`
- `{{TEST_ALL}}`     — full matrix (slow, heavy)     ·  nano-ros: `just test-all`
- `{{CI}}`           — what CI runs before merge      ·  nano-ros: `just ci`
- `{{FIXTURES}}`     — prebuild test fixtures         ·  nano-ros: `just build-test-fixtures`

CI runs **light checks on every push** (format, lint, unit) and the **heavy
`{{TEST_ALL}}` matrix only on `main` / nightly / manual trigger** — it is
time- and disk-heavy (full RMW × platform matrix + fixtures). Run light locally;
reach for the heavy tier before a cut, not every commit.

## Before you hand off / open a PR

- [ ] Design decision captured in an RFC (not just a phase doc).
- [ ] Work item checked off in its phase doc, with a passing script as proof.
- [ ] New behavior covered by a **mechanical script** test; preconditions fail loudly.
- [ ] No compilation inside tests; fixtures built in the build stage.
- [ ] `{{CHECK}}` clean; relevant `{{TEST_FAST}}` (or scoped tier) green.
- [ ] Anything broken/deferred filed as an issue, cross-linked to its RFC/phase.
- [ ] Durable knowledge filed in the right series — never grown into this file.
