---
id: 2396
title: "The census build step refuses every build of a C/C++ cross workspace image whose model folds in no contract, so a freshly scaffolded FreeRTOS talker cannot build"
status: resolved
type: bug
severity: high
area: [build, census, cmake, cli]
found: 2026-11-10
related: [phase-530, issue-2376, issue-2269]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)** (W6, the census step). Found while replaying issue 2269's QoS lab in C.

## What fails

On main (5028793dc6), a freshly scaffolded C or C++ talker on a cross board
does not build:

```text
nros new sensor_ws --board mps2-an385-freertos --template talker --lang c
cd sensor_ws && nros build
...
[  1%] nano-ros: census step (.../build/nros/models/sensor_node_bringup/system_model.yaml)
Error: the census `.../system_model.census.json` is stale (no census at `...`), and no
host build is recorded beside it to re-take it during a build.
```

The bringup declares no host image, and its model folds in no contract.
Both of the census's other readers agree there is nothing to do:

- the configure's `ws entity-census check --require-fresh` prints
  `census check: no contract -- ... nothing for a census to reconcile` and
  passes;
- `nros dev image entity-census take --image sensor_node_bringup:freertos`
  prints `no contract -- ... nothing for a census to reconcile`.

The build step (`take --model <m> --build-step`, `take_model_in_build`) does
not ask that question. `nros_census_step` adds the step for every cross
workspace image before the configure's check runs, so the step met a census
that was never taken and refused.

This breaks the Tutorial's C and C++ tabs once they move to
`mps2-an385-freertos` (issue 2376) and the `freertos-c` / `freertos-cpp`
starter tracks.

Second sighting: the tier-2 matrix, run 42721642028 (job 129516803849, at
24a7f9b94d). Nine sites across the `threadx_linux`, `freertos` and `nuttx`
stages failed on the same refusal, each a contract-less cross workspace image.

## Resolution (2026-11-10)

`take_model_in_build` asks the same first question as `take --image` and
`check`, and asks it first: a model that folds in no contract has nothing for
a census to reconcile, fresh, stale or never taken. That return still writes
the census digest, write-if-changed, as the constant `no contract`, because
`nros_census_step` names the digest as a configure input and a byproduct (an
input that never exists is dirty on every build). It never fails a
`--fail-if-changed` step: no knob of a contract-less image is read from a
census, so the digest appearing on a first build says nothing about the pool
sizes that build read.

Test: `a_model_with_no_contract_has_nothing_to_take_and_a_stable_digest`.
Measured on a fresh `nros new --board mps2-an385-freertos --lang c`, under
Unix Makefiles and under `CMAKE_GENERATOR=Ninja`: the first `nros build`
passes, and two more `cmake --build` / `ninja` runs configure nothing and
compile nothing. (`ninja -n` lists "Re-running CMake" on every tree with a
`CONFIGURE_DEPENDS` glob, because its glob check is always dirty; `-d
explain` names only `VerifyGlobs.cmake_force`, never the census.)
Sweep: `git grep -n 'discover_contract(' packages/cli/nros-cli-core/src/cmd/entity_census.rs`
(`take_census` and `take_model_in_build` both ask; the standalone-leaf road
asks `entity_source_of` instead).
