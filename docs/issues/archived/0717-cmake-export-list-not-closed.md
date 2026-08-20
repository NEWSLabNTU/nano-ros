---
id: 717
title: "the cmake `export -f` list is hand-maintained and has broken twice; nothing checked it closed over its call graph"
status: resolved
type: tech-debt
area: build, testing
related: [issue-0400, issue-0706, issue-0196]
---

## The class

`fixtures-build.sh` fans its cmake rows out to `make` workers. A make leaf is a
fresh bash holding only what `export -f` gave it, so a function CALLED by an
exported function but absent from the list is an unbound command — in the leaf,
and only in the leaf. Running the same code by hand works, which is exactly why
each instance surfaced from a lane rather than from a review.

Twice, both times as a helper added to an already-exported function:

* **0400** added `nros_cmake_guard_build_dir`, called by
  `nros_cmake_configure_if_needed` → `nros_cmake_guard_build_dir: command not
  found`;
* **0706** added `nros_cmake_toolchain_resolved_cc` and `nros_cmake_dir_cc`,
  called by `nros_cmake_guard_build_dir` → `nros_cmake_toolchain_resolved_cc:
  command not found`, which took out every NuttX C row of the tier-2 fixture
  build.

Each fix appended to the list: the reported site, not the class. 0706's is one
level deeper than 0400's — reachable only THROUGH another helper — so a check
that looked one call deep would have passed it.

## Why it went unchecked for so long

The CARGO half of the very same list already had a check:
`build_root_derivation.sh`'s make-leaf scenario reads the `export -f
nros_fixture_target_dir_flag …` statement and asserts its members. The cmake
statement two lines below it had none. Same file, same mechanism, same failure
mode, one list covered — issue 0196's shape.

## Fix

`scripts/check-cmake-export-closure.sh`, on the fast lane. It folds the
continuation lines of the `export -f nros_fixture_build_cmake …` statement,
collects every `name() {` definition across the three sourced files, and walks
the call graph TRANSITIVELY from the exported set, reporting any callee that is
defined here and not exported.

Verified against the real regression, not only against fixtures: deleting
`nros_cmake_toolchain_resolved_cc` from the live list reproduces 0706's failure
as `nros_cmake_toolchain_resolved_cc (called by nros_cmake_guard_build_dir)`.
`--self-test` covers five shapes including both historical depths, backslash
continuations, and a name merely mentioned in prose (which must NOT be reported).
