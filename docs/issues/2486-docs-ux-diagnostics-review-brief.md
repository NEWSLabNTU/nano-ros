---
id: 2486
title: "Phase-555 W2 decision brief, docs-ux-diagnostics: what the review of the area's RFCs leaves for the user to rule"
status: open
type: tech-debt
area: [docs, design]
found: 2026-11-13
related: [phase-555, rfc-0027, rfc-0029, rfc-0116, rfc-0117, rfc-0118, rfc-0123, rfc-0124, rfc-0127]
---

# Phase-555 W2 decision brief: docs-ux-diagnostics

[Phase-555](../roadmap/phase-555-design-doc-review.md) W2 reviews each
area's RFCs. Mechanical fixes go straight into the review PR. Anything that
would change a design comes here, as one brief for the area, for the user
to rule.

Reviewers: the AUO book session (RFC-0118, 0123, 0124 and 0127, with their
pairs) and nros 243 (RFC-0027, 0029, 0116 and 0117, with theirs). Each
reviewer appends their own section.

## Verdicts (AUO book session, 2026-11-13)

| RFC | Verdict | `area:` | Evidence |
| --- | --- | --- | --- |
| 0118 target execution model | **keep** | moved to `platform-board-rtos` | `run`, `flash`, `monitor`, `bench scan\|list` and the reserved `debug` are in `nros --help` as D2 states them. `--expect-regex`, `--router auto\|none\|<locator>`, `--prefix` and `--plan` are on `nros run`. `WaitFor` is `usb:` or a port path (`nros-orchestration-ir/src/target_exec.rs`). Phases 528 and 529 are archived DONE, and phase-533 (the hardware runner) waits on bench boards. The facts it owns sit in the board descriptor and the bench file, so it moves to the board area. Its siblings RFC-0114 and RFC-0115 are there too. |
| 0123 two books by audience | **keep** | confirmed | Phase-532 is DONE. `book-internals/` and `check-book-audience.py` exist, and `docs.yml` deploys Internals at `/internals/`. `book.toml` has the D7 redirects, and 60 user pages use the `## Design background` allowance. RFC-0124 amends D2 and D4 and lists 0123's D1, D3, D5 to D9 as authoritative. The review marked Appendix A's user-book half superseded and recorded the open questions' outcomes. |
| 0124 book charter | **keep** | confirmed | Phase-537 is OPEN, and the book's `SUMMARY.md` has its eight sections. Two stale facts are fixed: D8's PX4 record (#3169 merged on 2026-11-07, so the row and track are on `main`), and Appendix A, which still listed the Pico W as "Twin and hardware" after RFC-0115 Amendment 1. |
| 0127 starter image | **keep** | confirmed | Ruled 2026-11-11/12. Phase-551 is IN PROGRESS with W0, W1, W2 and W7 done. The image target is in `ci/docker/`. No pair names it. |

Pairs (all consistent, no design change):

- **T1 0118–0119.** Two subjects, not one. 0118 says what a runner is asked
  to do (the RunPlan, the ladder, the verbs), and 0119 says how a runner is
  packaged and built. Each cites the other at the joins (0118 D2 and D5;
  0119 D3 and D6). Similarity is 0.08. Neither merges.
- **0114–0123, 0114–0124, 0115–0124.** These are reconciled in RFC-0124's
  "Reconciled RFCs" table. The Pico W fix above came from 0115–0124.
- **0115–0118.** 0118's shape table already records Wokwi's removal by
  RFC-0115 Amendment 1.
- **0118–0123, 0119–0123, 0120–0123, 0121–0123.** 0123 D9's rules still hold,
  and RFC-0124 consolidates them. The pages D9 named under its "Run on a
  Target" part now sit where RFC-0124 D1 puts them: `guide/platforms/flash-a-real-board.md`,
  `extending/runner-and-command-packages.md`, and `reference/system-toml.md`
  for `[image.<id>.run|flash|console]`. RFC-0121 D5's `nros run <node>` is
  what the tutorial teaches.
- **0117–0123, on N4 (AUO book session).** The table 243 cites (0123 D2,
  the `plan resolve|check|explain` row) is the user-book part table that
  RFC-0124 D1 already superseded, and D2's banner says so. Whatever N4
  rules, that table stays as history and is not edited. The live user book
  spells the verbs as `nros --help` does: `reference/cli.md` is generated
  from the tree.
- **0118 and 0121, outside this slice.** RFC-0121's "Amends" table names
  "0118 D2" (the node rung), but its `amends:` frontmatter lacks 0118. 0118
  now carries the back-pointer. The frontmatter fix belongs to the
  workspace-system-model slice.

### Questions for the user

| Q | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| D1 | RFC-0123 and RFC-0124 overlap: 0124 consolidates 0123's book rules. Keep both, or fold 0123 into 0124? | **(a)** keep both: 0123 owns the two-book split, Internals' parts, `docs/` relations and publishing (D1, D5 to D7); 0124 owns the user book. **(b)** merge 0123 into 0124 and mark 0123 superseded. | 0124's own table names 0123's D1, D3, D5 to D9 authoritative. The two RFCs cite each other at every amended decision. | **(a).** The split and the Internals book are a different subject from the user book's organization. Merging moves no decision and breaks the links that cite 0123 D3 and D7, including the `check-user-surface-ids` and `book.toml` comments. |
| D2 | RFC-0123's work is done (phase-532 DONE). Flip it to `Stable`? | **(a)** flip now, updating the matching ARCHITECTURE.md section in the same commit (README drift rule). **(b)** stay Draft until D6's last step (deleting `docs/guides/`) is done. | `docs/guides/` holds 15 files: 14 one-release "moved" stubs and one HTML slide deck. | **(b)**, then flip. The stubs expire with the next release, and the flip can ride the commit that deletes the directory. |

## Verdicts (nros 243, 2026-11-13)

| RFC | Verdict | `area:` | Evidence |
| --- | --- | --- | --- |
| 0027 ROS 2 user workflow | **supersede-by 0121** (question N1) | moved to `workspace-system-model` | Its own status line hands layout authority to RFC-0024/0025, and its notes rename half its vocabulary. Its "Phase 222 removed `nros build` / `nros run`" note is false today: both verbs are in the user tier (RFC-0116 D7, RFC-0117 D2, RFC-0118 D2). The project shape it argues for is now decided by RFC-0121 (every project is a colcon workspace, `system.toml` only in a bringup). `ARCHITECTURE.md:342` still routes "`nros new` scaffolding" here. Its subject is the workspace and build pipeline, not docs or UX. |
| 0029 zonal vehicle architecture | **archive** to `docs/research/` (question N2) | moved to `platform-board-rtos` | A survey (domain vs zonal E/E, vendor boards, TSN stacks, demo paths) that decides nothing: no decision, no phase (`implements-tracked-by: []`), last reviewed 2026-02. Its one design claim, where nano-ros fits a zone controller, is cited only by `ARCHITECTURE.md:383` and RFC-0123 as background. RFC-0001 already sends TSN readers to `docs/research/tsn-safety-island-assessment.md`, the series this belongs in. |
| 0116 adoption purposes | **amend** (question N3) | confirmed | D1 (three purposes) and D9 (adoption probes) are decisions this RFC still owns, and phase-523 is still running D9. D2, D5, D6 and D7 each carry "Consolidated in RFC-0124" plus later amendments (RFC-0123 D3, RFC-0121, RFC-0118 D2, issue 2305), and their bodies now contradict their own banners. D6's body still scaffolds a standalone package ("No workspace… a real leaf… `nros new --workspace`"), which RFC-0121 retired. Mechanical fix made: D2 records ruling A of 2026-11-12, which settles issue 2305's open question (build-stopping refusals carry no id). `last-reviewed` updated. |
| 0117 CLI command tiers | **amend** (question N4) | confirmed | Phase-526 is archived, and the tree it built matches D1, D3, D5 and D6. D2's table has drifted from the shipped tree (`nros --help` built from main on 2026-11-13). `explain` is a top-level Inspect verb, not `plan … explain`. `nros plan <bringup>` takes no `resolve\|check\|explain` subcommands. `check` is top-level, per the 2026-11-07 amendment, so the table's `plan … check` is stale. In Toolchain, `update` is top-level (RFC-0104 D7) and `pin` takes `[VERSION]`, not `show\|bump\|update`. Open question 2 (`list --kind`) is still unbuilt. |

Pairs:

- **0117–0119.** Consistent. 0119 cites 0117 D1, D2 and D4, and 0117's
  2026-11-07 amendment, for `nros check <package>` and the hidden dev tier.
  No overlap.
- **0117–0123.** Consistent with each other, and both drift the same way:
  0123's part table (line 115) also teaches `plan resolve|check|explain`.
  Whatever N4 rules, 0123's table follows it in the same change.
- **0117–0124.** Consistent. 0124 Principle 5 takes 0117's user tier as
  authoritative, and D8's PX4 note (no dev verb typed or told) is 0117 D3.

### Questions for the user (nros 243)

| Q | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| N1 | RFC-0027 (Stable, 2026-05) argues a workflow whose shape RFC-0121 now decides. Supersede it, or keep it as rationale? | **(a)** `supersede-by 0121`: move it to `archived/` with a forward pointer, and route `ARCHITECTURE.md:342` and RFC-0024's two mentions to RFC-0121. **(b)** keep it, rewriting its CLI note and status line. | Its status line already defers layout to 0024/0025. Its CLI note is false today. 0121 decides the project shape, and 0024/0025 the layout. | **(a).** A Stable RFC whose notes say "read every name as something else" misleads more than it explains, and its rationale survives in 0121's Summary and Problem. |
| N2 | RFC-0029 is a market and technology survey, not a design. Archive it as an RFC and keep it as research? | **(a)** move it to `docs/research/zonal-vehicle-architecture.md` (a `git mv`, keeping history), drop its row from the RFC index, and repoint `ARCHITECTURE.md:383` and RFC-0123 line 140. **(b)** keep it as an RFC. | It has no decision and no phase. `docs/research/` already holds its TSN sibling, which RFC-0001 links. | **(a).** The RFC series is for decisions, and the survey keeps its value where research lives. |
| N3 | RFC-0116's D2, D5, D6 and D7 are consolidated elsewhere, and their bodies contradict their banners. Rewrite them as pointers? | **(a)** keep D1, D9 and the evaluation table as this RFC's own decisions. Replace the bodies of D2, D5, D6 and D7 with a short statement of what holds now, each naming its authoritative home (RFC-0124 P5/P12, RFC-0121, RFC-0118 D2, the diagnostic-code RFC). **(b)** leave the history in place under its banners. | D6's body describes the standalone-package front door RFC-0121 retired. D2's open question was ruled 2026-11-12, and that ruling is now recorded under its banner. | **(a).** A reader who skips a banner reads a retired design as current. History stays in git and in the changelog. |
| N4 | RFC-0117 D2's table no longer matches the shipped user tier. Record the tree, or move the verbs to the table? | **(a)** record the shipped tree. Inspect: `doctor · board · config · plan · check · explain`. Toolchain: `toolchain · pin [VERSION] · update · locate`. **(b)** restore the table: `explain` → `plan explain`, `update` → `pin update`, with the current spellings kept as aliases (D5). | `explain` and `update` are top-level in `nros --help`. RFC-0104 D7 decided `nros update`, and the diagnostic-code RFC (PR #3629) extends `nros explain` to codes, which a `plan` subcommand could not carry. | **(a).** Both verbs were decided after the table, by RFCs that own them. Moving them back would make `nros explain NROS-0412` a plan subcommand. |
