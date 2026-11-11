---
id: 2450
title: "A jobserver unit test wrote its fake `make` by hand and failed with ETXTBSY under load; the gate for that class skipped `#[cfg(all(test, ...))]` modules"
status: resolved
type: bug
severity: medium
area: [testing, cli, gates]
related: [0476, 2421, 2221]
found: 2026-11-12
resolved_in: "this change: the test writes its stubs through `write_executable_stub`, and `check-executable-stub-writes` reads every test-only cfg"
---

## Measured

`builder::jobserver::tests::a_make_43_cache_is_repaired_to_the_44_make_first_on_path`
(issue 2421) failed once in another session's `cli-tests` run, then passed
alone and on a rerun. Reproduced here, on main `f9db6bcb3f`, by running the
crate's `builder::` tests 30 times at `--test-threads 64`. **3 of 30 runs
failed**, each at the same line:

```
panicked at nros-cli-core/src/builder/jobserver.rs:760:9:
assertion `left == right` failed
```

That line is `assert_eq!(tool_version(&old), Some((4, 3)))`: the fake
`make --version` did not run. `tool_version` maps a spawn error to `None`.

## Cause

This is issue 0476's race. The test wrote each fake `make` with
`std::fs::write`, then made it executable with `set_permissions(0o755)`, and
then executed it. `cargo test --lib` runs the tests as threads. A sibling
test that forks while this thread holds the write descriptor gives its child
a copy of it until the child's own exec, and an `execve` of the stub inside
that window fails with `ETXTBSY`. `O_CLOEXEC` closes the copy at exec, not at
fork. The fix for the class already existed:
`nros_launcher::test_support::write_executable_stub` writes the stub from a
child process, so this process never holds the descriptor.

**Why the gate for the class did not refuse it.** `check-executable-stub-writes`
(the 0476 gate) treats code as test code only after a line matching
`#[cfg(test)]` exactly. The jobserver module is `#[cfg(all(test, unix))]`, so
the gate never read it. The tree has 165 compound `cfg` lines naming `test`.
After the fix the widened gate finds exactly one hand-written stub, this one.

## Fix

- The test writes both fake makes through `write_executable_stub`. Measured
  after: **0 of 60** runs failed under the same loop.
- `check-executable-stub-writes` counts as test code any `#[cfg(...)]` whose
  predicate is `test` or an `all(...)` naming `test` outside a string. It does
  not count `not(test)`, or a top-level `any(test, ...)`, which is also built
  without `test`. Its self-test covers each of these.

Sweep: `git grep -nE "^\s*#\[cfg\((all|any)\([^]]*\btest\b" -- '*.rs'` lists
the compound test cfgs the gate now reads; `python3
scripts/check/check-executable-stub-writes.py` lists any hand-written stub
among them.
