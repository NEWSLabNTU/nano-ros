---
id: 2059
title: "`just test-ignored` never places nros-c's config header, so rosidl-codegen's three C syntax checks have failed every night since issue 1824"
status: resolved
type: bug
area: [ci, testing, codegen]
severity: medium
found: 2026-10-25
resolved: 2026-10-25
related: [1824, 1507, 0328]
---

## What

`rosidl-codegen`'s `heap_compile_check` has three `#[ignore]`d tests that run
`cc -fsyntax-only` over generated C: `generated_c_with_dheader_wrap_syntax_checks`
and its service and action siblings. Their generated headers include
`<rclc/types.h>`, which includes nros-c's per-build
`rclc/detail/nros_config_generated.h`.

Issue 1824 (`d88b77e66e`) moved that header into cargo's `$OUT_DIR` only and
made the tests take its directory from `NROS_C_CONFIG_INCLUDE`, refusing when
it is unset. `just test-ignored` is the one lane that runs them (host-tests'
nightly `unit` job, issue 1507), and it was not changed to place the header.
So all three refused every night:

```
NROS_C_CONFIG_INCLUDE=None holds no rclc/detail/nros_config_generated.h.
```

The refusal itself was right: a missing header must not read as a codegen bug.
The lane was what was missing.

## Measured

- host-tests on main, run 40157702519 (2026-10-25): `just test-ignored` red on
  `generated_c_service_with_dheader_wrap_syntax_checks` and
  `generated_c_action_with_dheader_wrap_syntax_checks`. Nextest stops at the
  first failure, so it reported 9 of 21 tests run, and the third C check never
  ran.
- The same two in run 40164384448 (the phase-162 branch, same day).
- Locally with `--no-fail-fast`: 21 run, 18 passed, all three C checks failed.
  Nothing else in the lane was hiding behind them.

## Resolution

`just test-ignored` places the header before it runs anything, through the one
shell spelling, `nros_cargo_placed` (`scripts/build/cargo.sh`), into
`build/heap-compile-check`, with the features the test's own refusal names
(`alloc,platform-posix,rmw-cffi`). It then exports the directory that
`nros_locate_artifact c-include --placed-dir` answers. It does this for a
whole run and for `just test-ignored rosidl-codegen`, and stops if the header
was not placed instead of letting the three tests report it.

Measured after: `just test-ignored` exits 0. The three runs report 3/3, 21/21
and 2/2 passed, and all seven `heap_compile_check` tests pass.

## The class

A test that refuses without an artifact a build step produces needs the lane
that runs it to produce that artifact. Issue 1824 swept the build-side
consumers of the retired header path, but not this test-side one: its only
runner was a recipe, not a build script, so no build-side sweep would find it.
