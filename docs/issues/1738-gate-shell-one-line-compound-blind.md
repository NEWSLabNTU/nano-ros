---
id: 1738
title: "Two shell-reading gates cannot see a one-line compound statement (`if …; then …; fi`)"
status: open
type: bug
area: [tooling]
severity: medium
found: 2026-10-07
related: [phase-472, issue-1077, issue-0650]
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../development/audit-findings-2026-10-07.md)).

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
