---
id: 533
title: "The west fixture lane never resolved its bringups' SystemModels, and `|| true` hid the failure until a test blamed a missing binary"
status: resolved
type: bug
severity: high
area: testing, build
related: [issue-0510, issue-0380, phase-330]
resolved_in: "issue-0533 (sync each bringup before the west build)"
---

## Symptom

`cli_bringup_zephyr_adapter_shim_boots_native_sim` fails, and says nothing about
the real cause:

```
Error: BuildFailed("Test fixture binary not prebuilt:
  build/west-fixtures/west_bringup_zephyr/zephyr/zephyr.exe
Run `just build-test-fixtures` first.")
```

`just build-test-fixtures lane=all` HAD run, and reported success.

## What actually happened

The lane does try to build it. The west build fails at CONFIGURE:

```
CMake Error at zephyr/cmake/nros_system_generate.cmake:178 (message):
  nros codegen-system failed (rc=1):
  Error: codegen-system:
  `…/multi_pkg_workspace_zephyr/demo_bringup/system.toml`
  declares system semantics but no SystemModel was found. It is a BUILD
  ARTIFACT (phase-330 W4), so generate it rather than committing one:
      nros sync
```

`just/zephyr-ci.just` calls the script as

```sh
ZEPHYR_BASE="$workspace/zephyr" bash scripts/build/west-fixtures.sh || true
```

deliberately — a failed FVP build without the SDK should not fail the lane. But
`|| true` swallows every failure, including this one, so the lane printed
"Zephyr test fixtures built successfully", wrote no stamp, and the breakage
surfaced hours later as a test blaming a missing binary. Same masking shape as
#0510's px4 lane.

## Why it broke

`39d007dfc feat(phase-330 W4.a+W5+W7): the SystemModel is a pure build artifact`
stopped committing models — correctly (#0380: hand-edited committed models were
the bug). Every consumer was supposed to resolve one instead. This lane never
learned to, and nothing noticed because of the `|| true`.

The fixture's last touch IS that commit, so it has been broken since.

## Why `nros sync` at the workspace root does not work

```
Error: sync: no `src/<pkg>/package.xml` and no `package.xml` at root under
  …/multi_pkg_workspace_zephyr — expected colcon-style workspace or single-pkg dir
```

These fixtures keep their packages at the ROOT (`demo_bringup/`, `talker_pkg/`,
`listener_pkg/`) rather than under `src/`, which sync rejects outright.

## Fix

Sync runs INSIDE each bringup directory, not at the workspace root. That works
because the shim resolves a bringup with no enclosing colcon workspace as its
OWN workspace (`_nros_system_detect_self_pkg` in `nros_system_generate.cmake`),
so `<bringup>/build/nros/models/<bringup>/` is exactly where the configure then
looks. The loop keys on `system.toml`, so a fixture that declares no system
semantics is untouched.

Verified from scratch — model and build dir both deleted:

```
$ rm -rf …/demo_bringup/build build/west-fixtures/west_bringup_zephyr
$ ZEPHYR_BASE=… bash scripts/build/west-fixtures.sh
   built …/build/west-fixtures/west_bringup_zephyr
west fixtures built (2/2).
$ cargo nextest run -p nros-tests --test cli_bringup_zephyr
  PASS [2.089s] cli_bringup_zephyr_adapter_shim_boots_native_sim
```

## Left open: the `|| true` is still there

The masking is not fixed, only this instance of it. A lane step whose failures
are invisible will hide the next one exactly as well — and the reason for the
`|| true` (an SDK-gated FVP build) is real, so the fix is per-fixture tolerance
rather than blanket tolerance: a fixture that CAN build on this host should fail
the lane when it doesn't. Worth its own issue if a third instance appears; #0510
was the first, this is the second.
