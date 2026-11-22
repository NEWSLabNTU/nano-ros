---
id: 2821
title: "The Zephyr interpreter resolver ignores the line's Python floor, so `build-c-port` configures 4.4 under 3.10"
status: resolved
type: bug
severity: medium
area: [zephyr, build, tooling]
related: [issue-0093, issue-0698, issue-1043]
found: 2026-11-22
resolved: 2026-11-22
---

## What

On the 4.4 line, `NROS_ZEPHYR_VERSION=4.4 just zephyr build-c-port` stops at
configure (host 241, 2026-11-22, PR #4549's smoke):

    Could NOT find Python3: Found unsuitable version "3.10.12", but required
    is at least "3.12" (found /usr/bin/python3)

with `-DWEST_PYTHON=/usr/bin/python3` on the cmake line. The 4.4 FIXTURE
road builds on the same host.

## Why

Two answers existed to "which interpreter runs west for this line", and they
disagreed:

- `nros_zephyr_python` (`scripts/build/zephyr-python.sh`) took the first of
  `NROS_PYTHON`, `scripts/zephyr/.venv` and `python3` on PATH that could
  import west. It never asked the interpreter's VERSION, so it returned the
  system 3.10 on both lines.
- The 4.4 line's 3.12 venv was known only to the just variable
  `ZEPHYR_VENV_BIN`, keyed on the version string (`.venv312` under the 4.4
  workspace). Each recipe that wanted it had to read it by hand:
  `zephyr-ci.just` (the fixture road) prepended it to PATH, and the dev
  `build-one` recipe ran west through it. `build-c-port` used only the
  resolver, then ran a bare `west`. West passes its own `sys.executable` to
  cmake as `WEST_PYTHON`, so the whole configure ran under 3.10.

The bare `west` was the system one because the workspace's `env.sh`, which
`build-c-port` sources, prepends `~/.local/bin` to PATH.

This is the class issue 0093 already met once: a 4.4 recipe that does not
hand-read the venv variable runs the wrong interpreter. Its fix added the
variable to one more recipe. The same shape repeats per recipe, because the
knowledge lives in the variable and not in the resolver. Two more hand-joined
spellings of the venv path sat in `scripts/zephyr/check-copy-out.sh` and
`tests/zephyr-kconfig-stamp-tests.sh`. `scripts/check-python-deps.py`
re-spelled the resolver's candidate order, and so answered for 3.10 on 4.4 too.

## Fix

- The floor is READ, not authored. Each Zephyr tree states it in
  `cmake/modules/python.cmake` (`PYTHON_MINIMUM_REQUIRED`: 3.10 on 3.7, 3.12
  on 4.4). `nros_zephyr_python_floor` reads it from the selected line's
  workspace, through the one workspace ladder.
- `nros_zephyr_python [<workspace>]` takes a candidate only when it imports
  west AND meets that floor. It tries the workspace's own venv first
  (`nros_zephyr_workspace_venv`, the one spelling of `.venv312`, which the
  venv provisioner uses too). On 4.4 the system 3.10 is passed over; on 3.7
  it is taken.
- `ZEPHYR_VENV_BIN` is deleted. `build-c-port`, `build-one` and
  `check-copy-out.sh` run west as `"$py" -m west`, the form `build-one`
  already used on 4.4 so that the system ROS python stays `python3` for
  msg2idl. The fixture road's `nros_zephyr_activate` now resolves the venv
  itself. The kconfig stamp test asks the resolver.
- `check-python-deps.py` asks the shell resolver instead of copying its order.
- Gate `just check zephyr-python-floor`
  (`tests/zephyr-python-floor-tests.sh`): synthetic workspaces stating a
  floor. Its negative control is a floor no candidate meets, which must
  resolve to nothing; removing the floor test from the resolver turns that
  row red (measured).

## Not this issue

241's run also printed an `env.sh` complaint about the retired checkout-root
variable. That text comes from an `env.sh` written by an older
`scripts/zephyr/setup.sh`. The current generator no longer exports or checks
the retired name (RFC-0103 D6), so re-running setup for that workspace
regenerates the file.

## Acceptance

- `NROS_ZEPHYR_VERSION=4.4 just zephyr build-c-port` configures under the
  workspace's 3.12 venv and builds (host 241).
- The same recipe on 3.7 still builds under the system python.
- The 4.4 fixture road still builds a native_sim image.
- `just check zephyr-python-floor` is green, and red under the mutation
  above.

## A second defect the first run found

On the first run the fix worked for 4.4, and a 3.7 `build-c-port` in the same
tree failed with 4.4's floor. Its call stack was in the 4.4 tree's
`python.cmake` while `ZEPHYR_BASE` named 3.7. The recipe used one in-tree
`tests/zephyr-c-smoke/build` for both lines, so the 3.7 configure reused a
cache naming the 4.4 ZephyrConfig. The smoke build is now `ZEPHYR_C_PORT_DIR`,
under the line's own build root. A dir whose cache names another Zephyr tree is
rebuilt with west's `-p always`, and the recipe says why.

## Verified (2026-11-22)

- Host 241, at 1822af86c4 merged with main:
  - 4.4 `build-c-port` rc 0;
  - THEN 3.7 `build-c-port` in the same tree rc 0 (the cross-line negative
    control);
  - the 4.4 patch set re-applied (rc 0);
  - a 4.4 native_sim fixture image linked;
  - `just check zephyr-python-floor` 6/6.
- Host 133, full `just ci gate` at 1822af86c4: green.
