---
id: 1536
title: "The `zephyr_self_pkg_sibling` compile-check fixture fails its
  west-configure, reported but NOT reproduced in a main checkout"
status: open
type: bug
area: testing, zephyr
severity: medium
found: 2026-09-28
related: [1501, 1521, phase-470]
---

## What this is

`examples/fixtures.toml`:

```toml
[[compile_check_fixture]]
id = "zephyr_self_pkg_sibling"
builder = "west-configure"
dir = "packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling"
west_subdir = "caller"
west_board = "native_sim/native/64"
west_extra = "-DCONF_FILE=prj.conf"
output = "nros-system/system_config.h"
```

A phase-470 W5.b1 run reported this record failing during
`just zephyr build-fixtures` with:

```
No SOURCES given to target: app
```

and attributed it to `fix(#1501): the sibling self-pkg routes as cargo, and 1501
files where resolved issues live` (2026-09-25) — whose thesis is that the sibling
`alpha_pkg` declares `nros_cargo` and carries no `CMakeLists.txt`, while
"the sibling `caller/` app is what" consumes it. That commit does touch this
fixture (`fixtures/zephyr_self_pkg/sibling/alpha_pkg/package.xml`), so the
mechanism is at least coherent: if `alpha_pkg` no longer contributes C sources,
the `caller/` Zephyr `app` target can legitimately end up with none.

## Evidence status — read this before acting

**The `No SOURCES` failure is a second-hand observation from an agent worktree
and was NOT reproduced in a main checkout.** It is recorded here because the
report was specific and the blamed commit checks out, not because it has been
confirmed. Four attempts in the main checkout each stopped on a *different*
precondition before reaching the fixture, and a fifth reached a configure but
not a faithful one:

1. in-tree `nros` CLI stale (branch switch) — `just setup-cli`;
2. same again after the next branch switch;
3. `nros-launch-resolve` built from a different `play_launch` commit than `nros`
   (issue 0409's guard), tripped by restoring that submodule's recorded pin
   earlier the same day;
4. `NROS_ZEPHYR_FIXTURE_FILTER=zephyr_self_pkg_sibling` → `no records matched
   filter`. **That filter selects `[[fixture]]` west leaves; this is a
   `[[compile_check_fixture]]`, a different record type in a different lane** —
   so the filter can never match it, and the "no records matched" is correct
   rather than a symptom;
5. invoking `west build` directly on `sibling/caller` fails earlier, at
   `codegen-system: … alpha_pkg/system.toml declares system semantics but no
   SystemModel was found`, because the lane runs `nros sync` first and a direct
   invocation does not. That is the harness's prep step missing, not the defect.

So the reproduction recipe is still unknown, and **step 4 is worth keeping
whatever the outcome**: there is no obvious way to build one compile-check
fixture by name, which is why five attempts went to the environment instead of
the question.

## Why it went unnoticed

Same class as issue 1521. `[[compile_check_fixture]]` records run in the
compile-check lane, which lives in `check-build` — `schedule` /
`workflow_dispatch` only. No `pull_request` and no `merge_group` event runs it,
so a break here is invisible to everything a contributor sees before merging,
and stays invisible afterwards.

## What to do

1. **Reproduce it, or show it is already gone.** Someone with a warm Zephyr
   workspace should run the compile-check lane and say which. If it does not
   reproduce, close this and leave the note about the filter (below), which is
   independently true.
2. If it reproduces: the fix is presumably to give `caller/` its own sources, or
   to restore whatever `alpha_pkg` contributed before it was routed as
   `nros_cargo` — decide from `#1501`'s intent, not from this issue's guess.
3. **A way to run ONE compile-check fixture by id.** `NROS_ZEPHYR_FIXTURE_FILTER`
   covers `[[fixture]]` only. Without a sibling for `[[compile_check_fixture]]`,
   verifying one of these costs a full lane, which is how an unverified report
   like this one ends up filed instead of answered.

## Acceptance

- The lane is green, or this issue is closed with the measurement that shows the
  failure is gone.
- One compile-check fixture can be built by id, and the command is recorded here.
