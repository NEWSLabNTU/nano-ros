---
id: 1071
title: "A pull request deleted four gates from `just/check.just` and every gate
  stayed green — `check-gate-lists` verifies the registry's SHAPE, never its size"
status: open
type: bug
area: tooling, ci
related: [1072, 0196]
---

## What happened

PR #431 (`feat/phase-420-w8-vendor-fetch-gate`) added one gate,
`vendor-fetch-pinned`, and **deleted four unrelated ones** — recipes and
`fast-serial:` registry entries both:

* `box-sync-covers-tracked-source`
* `build-type-spelling`
* `entry-session-name`
* `format-macro-scope`

Measured against the branch's own merge-base, so this is not a rebase artifact:

```
                                 base   branch tip
box-sync-covers-tracked-source      1             0
build-type-spelling                 1             0
entry-session-name                  1             0
format-macro-scope                  1             0
```

All four had landed on `main` before the branch forked. The branch had rewritten
`just/check.just` against an older copy of the file; the loss was not
deliberate, and nothing in the commit message mentions it.

`entry-session-name` is the gate for issues 1003/1017 — *every emitted
`run_components` call must NAME a session* — whose defect "survived from
2026-06-13 to 2026-09-03". It would have gone back to unguarded.

## Why nothing caught it

`check-gate-lists` is the gate that reads the registries. Its verdict:

```
check-gate-lists OK — 2 registry(ies), 232 gate(s), one per line and sorted.
```

232, where `main` has 236. **It passed.** It asks whether the list is
one-name-per-line and sorted — properties a deletion satisfies perfectly. The
count is printed and nothing compares it to anything.

That is the repo's own recurring shape, one level up from issue 0196: the gate
exists, it runs, it is green, and the rule it enforces is narrower than the rule
anyone reading its name would assume. A registry that only ever grows has no
mechanism saying so.

The two neighbouring controls do not cover it either:

* `check-gate-selftests` is already a ratchet, but over "how many gates run
  their own selftest", not over how many gates exist. Deleting a gate that has
  a selftest makes that ratchet *easier* to satisfy.
* `check-test-scripts-have-callers` catches a `tests/*.sh` whose recipe was
  dropped — the sibling failure, from `e569d9a55` — but only for shell scripts
  under `tests/`. A gate whose recipe AND `scripts/check-*.py` both go leaves
  nothing stranded to find.

## Swept

Every open pull request, gate names at merge-base vs branch tip: **#431 was the
only one.** No other branch loses a gate.

```
python3 - <<'PY'   # full script in the issue history; short form:
# for each open PR: gates(merge-base) - gates(branch tip) must be empty
PY
```

## What would fix it

A ratchet on the registry's size, in the shape `check-gate-selftests` already
uses: a committed baseline count that may only increase, with a
`--write-baseline` escape for a deliberate retirement that says why. Retiring a
gate is rare and always intentional; re-stating the number is the right price.

Worth deciding at the same time whether the ratchet is on the COUNT or on the
NAME SET. The name set is strictly better — it catches a delete-plus-add that
nets to zero, which is exactly the shape #431 had (one added, four removed, net
−3 and still passing) — and costs a longer baseline file.

## Also worth noting

#431's four deletions and its one addition sat in the same file as **twelve of
the twenty-two conflicting pull requests** measured in issue 1072.
`just/check.just` is the tree's single busiest merge target, which is both why
the branch had a stale copy of it and why nobody diffing the PR would have
looked past the hunk they cared about.

Repaired in the branch (restored from `origin/main`, W8's own gate re-applied on
top, 236 gates, `check-gate-lists` green); the missing ratchet is what this
issue tracks.
