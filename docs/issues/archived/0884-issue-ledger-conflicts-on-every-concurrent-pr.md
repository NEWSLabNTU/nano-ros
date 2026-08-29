---
id: 884
title: "The open-issue ledger conflicted on every concurrent filing — a shared
  registry inside an authored file, with a running total on top"
status: resolved
type: tech-debt
area: build, testing
related: [phase-395, phase-396, issue-0871]
resolved_in: "issue 0884"
---

## Symptom

Every pull request that filed or resolved an issue conflicted with every other
one, on `docs/issues/README.md`. Measured on `origin/main` — two agents filing
unrelated issues, each regenerating the index:

    OLD design merge rc=1
    UU docs/issues/README.md

This session hit it on four consecutive rebases. phase-395 W1 had already
attacked the worst form (a 4,170-line hand-written list) by generating the rows;
what remained still collided.

## Two causes, and the small one was the worse one

**A running total.** The block opened with `NN open.` — ONE line that every
issue-touching PR rewrites at the same position. That is the worst possible
shape for concurrent edits, and it fails in two different ways: differing counts
keep both lines under a union merge, and MATCHING counts merge silently to a
WRONG total. Measured: two branches each taking `3 open.` to `4 open.` produced
`4` for five issues — a clean merge with a false number.

**Generated rows inside an authored file.** `README.md` is prose plus a
generated block. A merge strategy that suits generated rows (union) is unsafe on
prose, so no per-file strategy could be applied while the two shared a file.

## Fix

* The generated list moved to **`docs/issues/open.md`** — 100 % generated, no
  authored prose. `README.md` keeps the conventions and points at it.
* **`docs/issues/open.md merge=union`** in `.gitattributes`. `union` is a
  BUILT-IN git driver, so it needs no per-clone `git config` — unlike a custom
  merge driver, which would silently not apply for anyone who had not run a
  setup step.
* **The count line is gone.** It is derivable by counting rows and tells a
  reader nothing the list does not.

Union is safe here only because the file is entirely generated: the generator
re-sorts and de-duplicates, and `check-issue-index` is the backstop that catches
anything a union leaves behind. That reasoning is written into
`.gitattributes`, `open.md` and the gate, because the safety depends on the
separation holding.

## Verified, both directions

    NEW design   concurrent merge rc=0   NO CONFLICT   both rows present
    OLD design   merge rc=1              UU docs/issues/README.md

Same two-agent scenario, run against this branch and against `origin/main`.
`check-issue-index` OK, 42 open rows = 42 files.

Also checked: a RESOLUTION racing a FILING merges correctly — the resolved row
is dropped and the new row kept (non-overlapping hunks never reach the union
driver at all).

## What this does not solve

Two agents editing the SAME issue file still conflict, as they should — that is
a genuine disagreement about content, not registry contention.
