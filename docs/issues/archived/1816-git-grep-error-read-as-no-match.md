---
id: 1816
title: "A `git grep` that could not search reads as \"no match\", so a forbidding gate passes outside a checkout"
status: resolved
resolved: 2026-10-15
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

## Fix (shell half — landed 2026-10-15)

About 40 shell `git grep` calls swallowed 128 the same way: `hits="$(git grep
… || true)"`, `done < <(git grep …)` and `if git grep -q …`. Measured over a
`git archive` export of `origin/main`, `check-feature-set-ssot`,
`check-no-absolute-model-paths`, `check-generated-leaf-regenerable` and
`ci/absolute-path-check` each said OK with no repository to search; after the
sweep each exits 2 there.

- `nros_git_grep <var> [-C <dir>] <git grep args…>` in `scripts/lib/grep-q.sh`,
  beside the existing `nros_git_grep_q`. Exit 0 or 1 is an answer, stored in
  `<var>`; anything else prints git's error and EXITS the caller with 2. The
  output comes back through a named variable, not `$(…)`, because an `exit`
  inside a command substitution ends only that subshell (the same reason
  `nros_grep_count` is shaped that way). Its locals are prefixed: a first
  version declared `local out`, and `printf -v out` then assigned the helper's
  own copy, so `check-retired-submodule-refs` (whose caller variable is also
  `out`) read nothing and went red.
- All 36 command-position calls in 21 files are swept onto it, or onto
  `nros_git_grep_q` where the caller only asked whether something matched. The
  one POSIX `sh` caller, `scripts/stage-sdk-root.sh`, cannot source a bash
  helper, so it spells the same check inline (`|| gg_rc=$?`, refuse above 1).
- `check-git-grep-status` now also scans tracked `*.sh`, `*.bash`, `*.just`,
  `justfile` and workflow `*.yml` for a `git grep` / `git -C <dir> grep` in
  command position on a non-comment line. Run over `origin/main` before the
  sweep it named exactly the 36 swept sites; its self-test holds seven raw
  shapes red and six prose/helper lines green on every run.
