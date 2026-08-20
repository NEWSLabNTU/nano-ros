---
id: 712
title: "`export -f` closure is ungated, so a new callee breaks make leaves at build time"
status: resolved
type: tech-debt
severity: medium
area: build
related: [issue-0706, issue-0400]
---

# 0712 — nothing checks that an exported shell function's callees are exported

`scripts/build/fixtures-build.sh` ships shell functions to `make` workers with
`export -f`. A make leaf is a **fresh bash holding only what `export -f` gave
it**, so a function that calls a sibling defined in the same file works in the
parent and dies in the leaf.

Three occurrences, all the same shape:

| when | the callee that was not exported |
| --- | --- |
| issue 0400 | `nros_cmake_guard_build_dir` |
| phase-340 B2 | `nros_fixture_platform_is_shared` |
| issue 0706 (2026-08-20) | `nros_cmake_toolchain_resolved_cc`, `nros_cmake_dir_cc` |

The third took out the whole tier-2 fixture build. `threadx_linux`, `freertos`,
`qemu` and `native` had already passed; the NuttX C rows then failed with

```
environment: line 20: nros_cmake_toolchain_resolved_cc: command not found
```

which names the callee but not the cause, and appears only after a long build.

## Why it keeps happening

The rule is invisible at the point of violation. Adding a helper to
`cmake-cache-guard.sh` and calling it from `nros_cmake_guard_build_dir` is a
local, obviously-correct edit; the `export -f` list that must also change is in
a different file, and nothing connects them. Every author has to remember.

The comment above the list says the make-leaf scenario in
`build_root_derivation.sh` "reads THIS list" — **that script no longer exists.**
It survives only in `docs/roadmap/archived/phase-{334,340,350}-*.md`. Whatever
coverage it had is gone, and what it covered was the target-dir list at the
second `export -f` site, never the cmake one that broke here. So the comment
reads as "this is gated" while nothing is.

## What a gate looks like

Cheap and static — no build required. For each `export -f` list in the build
shell files, walk each exported function's body, collect calls to names defined
in those same files, and require them to be exported too. A one-off run of
exactly this after fixing 0706 covered 11 exported functions across
`fixtures-build.sh`, `cmake-cache-guard.sh`, `fixtures-target-dir.sh` and
`build-root.sh`, and found the two missing names and nothing else — so the
check is both tractable and precise.

Two details worth keeping when it is written:

- Parse the continuation lines. Every one of these lists is wrapped with `\`,
  so a naive line-based reader sees only the first row and passes vacuously.
- Self-test both directions (the `check-skip-marker-matching.py` pattern): a
  closure checker that stops checking passes silently, which is the failure
  shape this issue is about.

Belongs on the `just check` fast line — it is a text scan, and the class it
catches otherwise costs a full fixture build to discover.

## Fixed already

The 0706 instance is fixed (`5a63eb193`); this issue is only about the missing
gate. Reopening the same defect a fourth time is the outcome to avoid.

## Resolution (2026-08-20)

`scripts/check-export-f-closure.sh`, on the `check-fast` line. It reads EVERY
`export -f` list in `scripts/build/*.sh` (continuations folded), takes the union
as the set a make leaf receives, and walks each exported function's body
TRANSITIVELY for calls to names the same files define — reporting any that no
list exports, with its caller and its defining file.

Current state: **49 exported names across five files**
(`build-root.sh`, `cmake-incremental.sh`, `fixtures-build.sh`,
`fixtures-target-dir.sh`, `run-gates-parallel.sh`), closure holds.

### It supersedes the #0717 gate, whose coverage was narrower than its own claim

`check-cmake-export-closure.sh` checked ONE entry point,
`nros_fixture_build_cmake`, and justified the scope by saying the cargo half was
"already covered by `build_root_derivation.sh`'s make-leaf scenario".

Both halves of that sentence needed checking, and this issue got one of them
wrong too:

* **That script exists** — this issue said it "no longer exists", but it moved to
  `packages/testing/nros-tests/tests/build_root_derivation.sh`. It does read the
  `export -f nros_fixture_target_dir_flag` list out of `fixtures-build.sh`,
  continuation lines included.
* **What it proves is much narrower than "covered".** It EXECUTES
  `nros_fixture_target_dir_flag` in a fresh bash with that list applied and
  compares the result to the parent. That exercises the call path those
  arguments take; a helper reached only on a branch not taken is invisible to
  it. It also has one hardcoded membership assertion, for
  `nros_fixture_strip_authored_target_dir`.

So the real prior coverage was: one static closure over one entry, plus one
execution of one path, against thirteen `export -f` statements. That is issue
0196's shape — a gate narrower than the rule it enforces — and it is why the
scope is now "every list", not "the next entry point someone remembers".

### Verified

* Self-test, both directions, 8 cases: closed list passes; a directly-called
  helper missing is reported (0400); one reachable only THROUGH another helper is
  reported (0706, the case a one-level check passes); a `\`-continuation reads
  as one list; SEVERAL `export -f` statements in one file form ONE set
  (`fixtures-build.sh` has six — treating each as its own closure would report
  every cross-list call as missing); a helper defined in a SIBLING file must
  still be exported (the 0400/0706 geometry); a name merely mentioned in prose is
  not a missing export; and the real build files yield a non-empty set, so a
  checker that stopped reading cannot pass silently.
* **Falsified against the real tree**, not only synthetically: deleting
  `nros_cmake_toolchain_resolved_cc` from the live list reproduces issue 0706
  exactly —

  ```
  nros_cmake_toolchain_resolved_cc (called by nros_cmake_guard_build_dir,
                                    defined in scripts/build/cmake-cache-guard.sh)
  ```

* Confirmed it RUNS in `check-fast` by name in the log — the #0717 gate was
  wired into `check-build` (the compile tier, PR + nightly only) while both that
  issue and this one said "fast line", so a text scan sat behind minutes of
  compilation and nobody noticed. A gate in the wrong tier is a quieter version
  of the same defect this issue is about.
* No forked `grep -q` in the membership test: it is a bash `case`, so issue
  0726's "verdict from a grep that did not run" cannot apply inside the BFS.
