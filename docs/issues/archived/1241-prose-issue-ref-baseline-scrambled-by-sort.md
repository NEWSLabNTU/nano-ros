---
id: 1241
title: "A ratchet's classification header is content no gate reads, so a `sort`
  during a conflict resolution destroyed it on `main` and every lane stayed
  green"
status: resolved
type: bug
area: [ci, docs]
found: 2026-09-09
resolved: 2026-09-09
related: [0989, 0206, 1161]
---

## What

`.config/prose-issue-ref-baseline.txt` on `main` had its header comments
interleaved among its data rows. Its first seven lines were seven bare `#`, and
the `CLASSIFIED 2026-09-08` block that says WHY each row is there was scattered
between the rows it classifies:

```
#
#
#   0417  RESERVED, NEVER WRITTEN. `refs/issue-ids/0417` exists on origin, so
#   1080  the id was claimed with `just issue-new` and no file was ever
# A RATCHET, not an allowlist: this file may only shrink. Each row is
docs/issues/1092-rmw-shape-gate-licenses-deviation-without-pinning-it.md:9999
...
#         issue 9999, which DOES NOT EXIST". Filing 9999 would destroy the
just/check/docs.just:1110
```

Every line is present; the file is sorted alphabetically as a whole. It arrived
in `0a02601b5`, a commit about something else, and is the signature of a
conflict resolved by running `sort` over the file.

## Why nothing caught it

`check-prose-issue-refs` reads the baseline as a SET of rows and discards
comments. So the gate's verdict is identical before and after — and the thing
that was destroyed is precisely the content no gate reads.

That is what makes it worth a gate rather than care. The rows are
`<path>:<id>` and say nothing about why. The header is the only place that
distinguishes:

* **IN FLIGHT** — the issue file is coming on another branch;
* **NEGATION** — issue 9999 is deliberately unresolvable, because issue 1092
  describes a mutation test whose control is "a gap reason naming issue 9999,
  which DOES NOT EXIST"; filing it would destroy the control;
* **RESERVED, NEVER WRITTEN** — 0417 and 1080, ids claimed with `just issue-new`
  whose files were never committed.

Without it, a reader cannot tell which lines are debt, and the ratchet degrades
into an allowlist — the exact failure its own header warns about.

Sorting has this effect only because comments and rows share a file. It is
harmless to the rows: they are a set.

## The second way the same content is lost

`--write-baseline` REPLACED the header with a hard-coded default, so
regenerating the file after adding one row deleted every classification a
person had written. Two mechanisms, one casualty.

## Fix

`check_baseline_shape` refuses a baseline in which any comment line follows the
first data row — the weakest rule that catches a whole-file sort, permitting
any header a person writes and refusing the shape no person writes. It carries
five self-test cases on the normal path, including the sorted-file case, so its
failure path is exercised on every run.

`--write-baseline` now keeps an existing header and rewrites only the rows,
falling back to the default only when there is no header to keep. Verified
idempotent: regenerating an already-correct file leaves it byte-identical.

`main`'s file is restored from `ace79ba9d` (the last commit before the scramble)
plus the one row `0a02601b5` legitimately added,
`docs/issues/1231-…:1164` — measured row-wise, nothing else changed and nothing
was lost.

## What this does not cover

The sibling ratchets — `.config/doc-commit-citations-baseline.txt`,
`.config/capability-skip-baseline.txt`, `.config/prose-issue-ref-baseline.txt`'s
own relatives — carry the same authored-header-plus-derived-rows shape and have
no such check. `check_baseline_shape` takes a list of lines and nothing else, so
adopting it in those gates is a call each one can make; none of them has been
scrambled yet, and a gate added for a defect that has not occurred is a
different decision from one added for a defect that has.
