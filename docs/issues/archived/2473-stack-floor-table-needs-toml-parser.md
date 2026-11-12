---
id: 2473
title: "The stack-floor CHECK read its table with tomllib/tomli, so a stock Ubuntu 22.04 (Python 3.10) user's mps2-an385-zephyr build failed to link"
status: resolved
type: bug
severity: high
area: [memory, build, tooling, zephyr, freertos]
found: 2026-11-12
related: [issue-1364, issue-2400]
---

## What happened

The book was walked on an installed release on Ubuntu 22.04. The mps2-an385-zephyr Rust talker
failed to link:

```
ModuleNotFoundError: No module named 'tomli'
CMake Error at …/nros/stack-floor/check.cmake:50 (message):
  nano-ros: stack floor of talker/rust/zenoh@mps2-an385-zephyr: see above
```

Cause:

- `scripts/nros-stack-floor.py` read `config/stack-floors.toml` with `tomllib`, which is Python
  3.11+, and fell back to the third-party `tomli`.
- Ubuntu 22.04's system Python is 3.10, and a stock host has neither module.
- The CHECK runs on the user's own Python. On Zephyr that is the Python Zephyr's build found; on
  FreeRTOS it is the Python CMake finds. So every image whose CHECK ran failed, whether measured
  or not.
- native_sim passed only because the CHECK does not run there.

This is issue 1364's class, one layer further out. 1364 was fixed by giving the CI images a TOML
parser. That never reaches a user's host, and no gate asked what a USER build imports.

## Fix

1. **The build reads a JSON twin.**
   - `config/stack-floors.json` is generated from the authored TOML by
     `python3 scripts/nros-stack-floor.py --sync-json config/stack-floors.toml`.
   - `cmake/NanoRosStackFloor.cmake` passes it to `--table`, which every road's command line
     shares (FreeRTOS and Zephyr), and the script loads it with the standard library's `json`.
   - The TOML stays the file people edit. `--validate-table`, on the fast line as
     `check stack-floor-table`, refuses a twin that is missing or does not match it, and names the
     command that regenerates it.
   - Only that maintainer side still parses TOML: `tomllib`, else `tomli`, else a one-line
     SystemExit, never a traceback.
2. **A gate for the class: `check build-python-stdlib`.**
   - Its reach is DERIVED: every `.py` a tracked CMake file names outside comments, plus the repo
     modules those scripts import, transitively.
   - Rule: each import is in Python 3.10's standard library, is a repo module, or sits in a `try`
     whose handler catches an import error.
   - It runs on any Python: 3.10's set is the running interpreter's
     `sys.stdlib_module_names` minus the modules added since, and it refuses to answer on a Python
     newer than its table knows.
   - Against the pre-fix tree it reports exactly
     `scripts/nros-stack-floor.py:753: imports 'tomli'`.
3. **A missing module degrades like a missing tool.**
   - Under `--check-stack`, an `ImportError` while the walk runs becomes the same SystemExit as a
     missing `llvm-readobj`.
   - So under ruling (A) an UNMEASURED image warns and builds; a measured one still fails.
4. **Ruling A on messages.** The issue ids in the script's user-facing messages are gone.

## Verification

- `python3.10 -S scripts/nros-stack-floor.py --self-test` passes. `-S` drops site-packages, which
  leaves a stock 3.10 with no `tomli`. Before the fix it died on `ModuleNotFoundError: No module
  named 'tomli'`.
- The self-test's own tables are JSON now, so it runs on that Python too.
- Build, measured. `examples/templates/cpp-port-minimal-publisher/mps2-an385-freertos` was built
  with `nros build` under a `python3.10 -m venv` that has no `tomli`. CMake found that venv's
  Python, as it would find a stock 22.04's.
  - **Before:** the link failed with `ModuleNotFoundError: No module named 'tomllib'` … `'tomli'`
    from `load_toml`. The image is unmeasured, so ruling (A) says it should only have warned.
  - **After:** the build succeeds. The image warns UNMEASURED and reports
    "OPEN stack 65536 B holds the floor 15896 B (unmeasured)".
  - The FreeRTOS image stands in for the reported Zephyr talker. A Zephyr build in such a venv
    would also lack west's own packages, and both roads run the same `_nros_stack_floor_command`.
