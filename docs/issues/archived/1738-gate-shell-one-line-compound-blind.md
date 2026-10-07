---
id: 1738
title: "Two shell-reading gates cannot see a one-line compound statement (`if …; then …; fi`)"
status: resolved
type: bug
area: [tooling]
severity: medium
found: 2026-10-07
related: [phase-472, issue-1077, issue-0650]
resolved_in: "gate-reach follow-up (scripts/lib/shell_statements.py)"
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../../development/audit-findings-2026-10-07.md)).

| gate | passes (rc 0) | fails (rc 1) |
| --- | --- | --- |
| `lane-skip-protocol` | `if [ -z "${FREERTOS_DIR:-}" ]; then echo "freertos: skip: …"; exit 0; fi` on ONE line in `just/freertos.just` | the same as a multi-line `if` |
| `pipefail-sigpipe-assertions` | `_f() { if ! printf '%s' "$1" \| grep -q x; then echo n; fi; }` on one line in a pipefail script | the same as a multi-line `if` |

Both read a shell LINE as one statement. `lane-skip-protocol`'s `ANNOUNCES`
needs `echo` at line start or after `[{;&|]` (not after `then`), and
`SAME_LINE_EXIT` is anchored at end of line, so `; fi` defeats it;
`pipefail-sigpipe-assertions`' `last_pipeline_stage` takes the rest of the line
(`; then …; fi; }`) as the last stage.

## Fix direction

One shared shell-statement splitter (split on `;`, `&&`, `||` and the
`then/do/else` keywords outside quotes, beside `scripts/lib/comments.py`'s
shell word-start rule), used by both gates and by any other line-based shell
reader, with a one-line-compound row in each selftest.

## Resolution

One shared splitter, `scripts/lib/shell_statements.py` (`statements(line)`):
cuts a line at every unquoted, depth-0 `;`, `;;`, `&&`, `||`, `&`, peels the
compound keywords that can open a statement (`then`/`do`/`else`/`{`/`}`/`fi`/
`done`/`esac`/`name() {`) into `lead`, and records the separator on each side.
Quotes and comments come from `comments.py`'s shell stripper — no new quote
model. No existing helper fitted: `comments.py` blanks text but does not cut
statements, and `per_item` / `harvest` are about calls and inventories.

- `check-lane-skip-protocol`: an announce is a STATEMENT matching
  `^(echo|printf)…skip`; same-line verdict = the NEXT statement is `exit 0`
  (keyword-only statements kept, so `…; fi; exit 0` is not the skip's exit).
- `check-pipefail-sigpipe-assertions`: the last pipeline stage is computed per
  statement, and "status read" is decided per statement (`if/elif/while/until/!`
  head, a following `then`/`do`, an adjacent `&&`, `|| continue|break|…`).
- Both gates run `shell_statements.self_test()` on the normal path, plus a
  one-line-compound row each.

| mutation | old rc | new rc |
| --- | --- | --- |
| one-line `if …; then echo "freertos: skip: …"; exit 0; fi` in `just/freertos.just` | 0 | 1 |
| control: the same as a multi-line `if` | 1 | 1 |
| one-line `_f() { if ! printf … \| grep -q x; then echo n; fi; }` in `scripts/check-decoupling.sh` (pipefail) | 0 | 1 |
| disarm the splitter (no cuts) | — | both gates' selftests fire (2 / 1) |

The tree had no live one-line offender of either kind: both gates stay green.
