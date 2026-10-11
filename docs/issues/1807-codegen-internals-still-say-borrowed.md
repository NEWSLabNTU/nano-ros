---
id: 1807
title: "The code generator's internal predicates and template keys still say `borrowed` after the storage-mode rename"
status: open
type: tech-debt
area: [codegen]
severity: low
found: 2026-10-11
related: []
---

## Measured

[phase-390](../roadmap/archived/phase-390-storage-mode-rename-inline-heap-view.md)
renamed the RFC-0033 storage modes to `inline` / `heap` / `view` in every type,
trait and API a user names (W1-W5). Its W6, the generator's own vocabulary, was
recorded and never started. The phase was archived on 2026-10-11 with W6 moved
here.

The internal identifiers that still say `borrowed`, as counted by phase-390:

| identifier | sites |
| --- | ---: |
| `is_borrowed` | 59 |
| `has_borrowed` | 19 |
| `borrowed_c_type` | 8 |
| `has_borrowed_{goal,feedback,request,response,result}` | 35 |
| `borrowed_read_fn` | 7 |

Re-count before starting:
`git -C packages/cli grep -c borrowed -- 'rosidl-*/src/**'`. On 2026-10-11 the
largest holders were `rosidl-codegen/src/templates.rs` (62),
`generator/common.rs` (49) and `generator/mod.rs` (44).

Nothing a user sees is wrong. This is internal consistency only.

## The footgun

Most of these are minijinja context keys, so each name exists twice: as a field
on a Rust context struct and as a name in a `.jinja` template. minijinja renders
an unknown key as empty instead of failing. Renaming one side and not the other
gives a template that still renders, without the branch. The golden corpus
catches that only for the message shapes it covers, and phase-390 W2 found
those were fewer than advertised.

## How to do it

One commit per key. Re-record the golden corpus and diff it in each commit; the
diff must be empty. `SlotBorrowing`, `nros_cdr_borrow_*` and
`borrow_loaned_message` are verbs and stay as they are (RFC-0033 records why).
