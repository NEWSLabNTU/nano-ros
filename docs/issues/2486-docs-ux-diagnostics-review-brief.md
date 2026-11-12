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
- **0118 and 0121, outside this slice.** RFC-0121's "Amends" table names
  "0118 D2" (the node rung), but its `amends:` frontmatter lacks 0118. 0118
  now carries the back-pointer. The frontmatter fix belongs to the
  workspace-system-model slice.

### Questions for the user

| Q | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| D1 | RFC-0123 and RFC-0124 overlap: 0124 consolidates 0123's book rules. Keep both, or fold 0123 into 0124? | **(a)** keep both: 0123 owns the two-book split, Internals' parts, `docs/` relations and publishing (D1, D5 to D7); 0124 owns the user book. **(b)** merge 0123 into 0124 and mark 0123 superseded. | 0124's own table names 0123's D1, D3, D5 to D9 authoritative. The two RFCs cite each other at every amended decision. | **(a).** The split and the Internals book are a different subject from the user book's organization. Merging moves no decision and breaks the links that cite 0123 D3 and D7, including the `check-user-surface-ids` and `book.toml` comments. |
| D2 | RFC-0123's work is done (phase-532 DONE). Flip it to `Stable`? | **(a)** flip now, updating the matching ARCHITECTURE.md section in the same commit (README drift rule). **(b)** stay Draft until D6's last step (deleting `docs/guides/`) is done. | `docs/guides/` holds 15 files: 14 one-release "moved" stubs and one HTML slide deck. | **(b)**, then flip. The stubs expire with the next release, and the flip can ride the commit that deletes the directory. |
