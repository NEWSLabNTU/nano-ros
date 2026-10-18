---
id: 1961
title: "A workspace fixture row ran with errexit OFF, so a failed `nros build` or `cmake` printed `built:` over the previous build's binary"
status: resolved
type: bug
area: [build, tests]
severity: high
found: 2026-10-18
resolved: 2026-10-19
related: [1249, 1466, 1671, 1946, 0997]
---

## What happened

Building issue 1946's acceptance fixture, `nros build` FAILED its configure
(`nros codegen source-language` could not place an unknown keyword) and
`scripts/build/workspace-fixtures-build.sh` went on:

```
-- Configuring incomplete, errors occurred!
Error: configure failed: `cmake -S build/freertos-cyclonedds-mps3-an536-freertos-cfg691ce9d54a ...` exited exit status: 1
     built: examples/workspaces/cpp/build/freertos-cyclonedds-mps3-an536-freertos-cfg691ce9d54a/cmake/mps3_an536_slirp_entry
     row-time: workspace-cpp-mps3-an536-freertos-slirp 4s
```

exit 0. The `built:` line is the script's own success check, `[ -x
"$build_subdir/$entry" ]`, passing over the binary the PREVIOUS build left
there. A lane built this way reads green over a configure that never ran, and
every test it feeds then measures a museum binary — the class CLAUDE.md warns
about under "A test result is only about the tree its FIXTURES were built
from", produced by the builder itself rather than by a rebase.

## Why

The script is `set -euo pipefail`, and each row's body was

```bash
local row_rc=0
(
    cd "$repo_root/$dir"
    ...
    "$nros_cli" "${nros_args[@]}"
    ...
) || row_rc=$?
```

Bash suspends `set -e` for every command executed within a command on the
left of `||` (or `&&`, or in an `if` condition) — the subshell and everything
in it included. So the body of every workspace fixture row ran with errexit
off, and only the steps that tested their own status (`|| return 2`) could fail
the row. `nros build`, `nros_cmake_configure_if_needed` and `cmake --build`
did not.

Measured with a probe (`set -euo pipefail`, a function whose body is a subshell
running `false; echo continued`):

| shape | called plainly | called from `if f; then` |
| --- | --- | --- |
| `( … ) \|\| rc=$?` | continued, rc=0 | continued, rc=0 |
| `( set -e; … ) & wait "$!" \|\| rc=$?` | stopped, rc=1 | continued, rc=0 |

No idiom survives a caller that is itself in an `if`/`||` — bash's rule
reaches every command the function runs.

Sweep: `git grep -nE '^\s*\) \|\| [a-z_]+=\$\?' -- 'scripts/**/*.sh' 'just/*.just'`
finds this one site.

## Fix

- The row body runs as `( set -e; … ) & wait "$!" || row_rc=$?` — an
  asynchronous subshell is not part of the `||` list, so it keeps the `set -e`
  it states (the form `nros_fixture_row`'s ledger path already used, issue
  1671). Both call sites (`nros_ws_row` from the serial walk and from a group
  worker) call it plainly.
- Because that cannot hold under a conditional caller, the three commands that
  produce the artifact — both `nros build` calls, the CMake configure and
  `cmake --build` — now check their own status and name the row on failure.

## Verified

- Negative: the row with the cyclonedds submodule one commit off its pin (a
  real refusal, `the build finished, but it resolved off the pin set`) now
  ends `!! workspace-cpp-mps3-an536-freertos-slirp: nros build failed (rc=1)`,
  prints no `built:`, and the script exits 1. Before the fix the same state
  printed `built:` and exited 0.
- Positive: the same row at the pin builds and exits 0.
- The whole `linux` workspace lane, with errexit now on for a body that had
  never run under it (so a statement relying on a harmless non-zero status
  would surface here): 79 rows, **77 built**. The other two are real failures
  this issue had been hiding — `workspace-rust-native-bridge` and
  `workspace-rust-native-bridge-xrce` do not compile on `main` (issue 1962);
  the pre-fix script reports the same E0433 and then `built:` over a binary
  from 2026-10-15, exit 0. So after this lands those two rows go red until
  1962 is fixed, which is the point.
