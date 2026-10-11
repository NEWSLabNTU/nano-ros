---
id: 1816
title: "A `git grep` that could not search reads as \"no match\", so a forbidding gate passes outside a checkout"
status: open
type: bug
area: [tooling, testing]
severity: medium
found: 2026-10-11
related: [0726, 1077, 1476, phase-484]
---

## Measured

`git grep` has three exits, and only two of them are answers:

| exit | meaning                                     | stdout   |
| ---- | ------------------------------------------- | -------- |
| 0    | matched                                     | the hits |
| 1    | searched, matched nothing                   | empty    |
| 128  | did NOT search: not a repository, a pattern git's regex engine rejects, a bad pathspec | empty |

```
$ git -C /tmp grep -n foo;          echo rc=$?   # fatal: not a git repository … rc=128
$ git grep -E 'a{';                 echo rc=$?   # fatal: … Unmatched \{          rc=128
```

The natural Python spelling, `subprocess.run(["git", "grep", …]).stdout`,
reads 1 and 128 identically. Every gate built on it FORBIDS a pattern, so
"git did not run" becomes "the pattern is absent" and the gate says OK.

Found while rebasing phase-484 W7b: `check-deprecated-cmake-names` (on PR
#1926, not yet on main) reported OK over a `git archive` export of the tree,
where git searched nothing at all.

The same idiom on `origin/main` (`3acbe3aa82`), 14 call sites in 13 files:

- **read stdout only** — `check-book-identifiers` (×2),
  `check-cmake-verb-reachable`, `check-config-fallback-macros`,
  `check-config-header-producers`, `check-goal-cdr-stripped`,
  `check-rmw-doc-slot-names`, `check/check-artifact-path-literals`,
  `check/check-store-source-readers`, `gen-config-surface`;
- **map any non-zero to "none"** — `check-foreign-env-path-inputs`,
  `gen-pool-inventory` (`returncode == 0`), `nros-build-wiring`
  (`… if r.returncode == 0 else 0`);
- **already strict** — `nros-mem-report` (`assert returncode == 0`, which also
  refuses a legitimate empty result; it wants one).

So 13 of 14 fail open. Run from a directory that is not a checkout, each one
measured the empty set.

## Same class, one tool over

This is issue 0726's defect for `git grep` instead of `grep`: one exit code
means "absent", another means "I did not look", and a caller who does not ask
for the status cannot tell them apart. `scripts/lib/grep-q.sh` answers it for
shell `grep -q` and nothing answered it for `git grep` in either language.

## Fix (Python half — landed)

- `scripts/lib/git_grep.py` — `run` / `lines` / `any_match`. Exit 0 or 1 is an
  answer; anything else raises `GitGrepFailed` with git's own stderr. Its
  self-test builds a repository outside the checkout and proves both failure
  modes raise (not a repository; `-E 'a{'`).
- All 14 call sites are swept onto it. `nros-mem-report` keeps its "the scan
  found SOMETHING" assertion as an explicit non-empty check.
- `check-git-grep-status` (`just check git-grep-status`, on the fast line) is
  an AST walk over every tracked `.py`. It refuses a list/tuple literal holding
  `"git"` … `"grep"` outside the helper, and its self-test runs on every call.
- #1926 landed first with two more raw callers, `check-deprecated-cmake-names`
  and `check-cargo-json-reader`. The new gate named both on the rebase, as
  intended, and both are on the helper now — 16 sites in all.

## Open — the shell half

There are about 42 `git grep` invocations in 23 shell/`just` files. The common
spelling is

```bash
hits="$(git grep -n PAT -- spec || true)"     # 128 swallowed with 1
done < <(git grep -l PAT -- spec 2>/dev/null)  # status discarded outright
```

Both swallow 128 the same way. The usual shape is `set -e` plus a `|| true`
that exists to survive exit 1. `check-retired-submodule-refs.sh` already does
this correctly by hand: `|| rc=$?` and then `case` on 1 versus anything else.

The fix wants a shell twin (`scripts/lib/git-grep.sh`, like `grep-q.sh` beside
`git_grep.py`) and a gate. A subshell cannot exit its parent, so the helper
has to report through the status the caller already captures. This is not done
here; it is the remainder this issue stays open for.
