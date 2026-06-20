---
id: 93
title: Zephyr testing-bin fixtures (logging-smoke) fail configure — py3.12 venv not on PATH (Zephyr 4.4 needs Python >=3.12)
status: resolved
type: bug
area: zephyr
related: [0087, phase-258]
resolved_in: "build-logging-smoke (zephyr-dev.just) prepends ZEPHYR_VENV_BIN to PATH on 4.4"
---

## Resolved (2026-06-20)

`just/zephyr-dev.just` `build-logging-smoke` (the only testing-bin zephyr fixture,
`logging-smoke-zephyr-native-sim`) now prepends the provisioned py3.12 venv to PATH
before its `west build` — `venvbin="{{ZEPHYR_VENV_BIN}}"` (4.4 → `.venv312/bin`,
empty on 3.7) + `export PATH="$(realpath "$venvbin"):$PATH"` — mirroring the
example-fixture path (`zephyr-ci.just`). On 4.4 `west`/cmake then resolve the 3.12
python so `find_package(Python3 3.12)` passes; on 3.7 the block is a no-op
(byte-identical). Verified: justfile parses; 3.7 (default) no-op. The 4.4 runtime
build-verify is CI / a 4.4-provisioned host (no `.venv312` on this host); the fix
reuses the proven example-path mechanism.

## Symptom (2026-06-20)

`just build-test-fixtures` (4.4 line) — the zephyr testing-bin fixture
`logging-smoke-zephyr-native-sim` fails at **configure**:

```
-- Application: packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim
CMake Error at .../FindPackageHandleStandardArgs.cmake:230 (message):
  Could NOT find Python3: Found unsuitable version "3.10.12", but required is
  at least "3.12" (found /usr/bin/python3 ...)
Call Stack: .../zephyr/cmake/modules/python.cmake:41 (find_package)
            .../ZephyrConfig.cmake (include_boilerplate)
            CMakeLists.txt:6 (find_package)
-- Configuring incomplete, errors occurred!
error: recipe `build-logging-smoke` failed with exit code 1
```

i.e. Zephyr 4.4 requires Python >= 3.12, but this fixture's build invoked
system `/usr/bin/python3` (3.10) instead of the provisioned `.venv312`.

## Cause

The 4.4 example-fixture path puts the py3.12 venv on PATH:
`just/zephyr.just:25` sets `ZEPHYR_VENV_BIN` (4.4 only) and `zephyr-ci.just:47-51`
`export PATH="$(realpath "$venvbin"):$PATH"` before the `west build`. The
**testing-bin** zephyr fixtures (`packages/testing/nros-tests/bins/*-zephyr-*`,
built via the `build-logging-smoke` / fixture-leaves path) don't go through that
PATH-prepend, so they configure with the system python (3.10) and Zephyr 4.4's
`find_package(Python3 3.12)` fails.

## Fix direction

Route the testing-bin zephyr fixture builds through the same py3.12 venv PATH
prepend the example fixtures use on the 4.4 line (factor the
`venvbin → PATH` step into a shared helper both call), or have the testing-bin
fixture recipe export `ZEPHYR_VENV_BIN`/PATH before its `west build`. Confirm
`logging-smoke` (and any sibling `*-zephyr-native-sim` test bins) configure on 4.4.

## Scope

Surfaced running host `test-all` for phase-258 on the 4.4 host-gcc line. Blocks
the zephyr testing-bin fixtures only (the 4.4 example fixtures build fine — they
get the venv). Env/build-wiring; CI's image (system py3.12) doesn't hit it.
Companion to [[0087-zephyr-3.7-native-sim-requires-sdk]] (both are 4.4/host
zephyr provisioning gaps).
