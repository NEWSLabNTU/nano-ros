---
id: 712
title: "`export -f` closure is ungated, so a new callee breaks make leaves at build time"
status: open
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
