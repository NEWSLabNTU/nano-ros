---
id: 2093
title: "`ZPICO_TL_RETAIN_BYTES not derived` is printed at configure for a CycloneDDS image: zenoh-only knob refusals are reported whatever the image's RMW"
status: resolved
type: bug
area: [build, cmake, rmw, ux]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [phase-517, 1498, 1572, 1125, 0973, 1649]
---

## Measured

The phase-517 first-project flow (`nros new my_robot --workspace`, which
scaffolds `rmw = cyclonedds`, then `nros sync && nros build`, the cmake road)
printed this at configure on branch `feat/517-w5-probe-sizing` (PR #2707):

```
-- nros: ZPICO_TL_RETAIN_BYTES not derived -- <reason>
```

The image links Cyclone, and nothing in it reads a zenoh-pico knob.

The defect predates that branch. The line was added in `91a9a1edc1`
(2026-09-25), and PR #2707 does not touch `cmake/NanoRosMessageBounds.cmake`.
On `main` the scaffold stays silent only because its entity counts are not
derived there, so `_nros_bounds_tl_retain` returns before it has a reason to
give. The branch derives them, which is what exposed the line. The emitter
reproduces on `main` (`2a01f7af3e`) with `cmake -P`, using a one-type bound
fragment and an entity fragment with one transient-local publisher plus an
action server:

| `-D` | line printed |
| --- | --- |
| `NROS_RMW=cyclonedds` | yes |
| `NROS_RMW=xrce` | yes |
| `NROS_RMW=zenoh` | yes |
| none | yes |

## Cause

`nros_derive_message_bound_knobs()` derives the transient-local retention slot
(`_nros_bounds_tl_retain`) for every image, which is fine: one inventory feeds
every backend, and an unread `NROS_DERIVED_TL_RETAIN_BYTES` costs nothing. The
STATUS line reporting its refusal was unconditional, though, and that line names
a zenoh-pico pool. It also used the retired `ZPICO_*` env spelling (phase-484
W8) rather than the registry's `NROS_ZENOH_TL_RETAIN_BYTES`.

The module could not ask which backend the image is. The one resolver,
`_nros_entry_resolve_rmw` (`NROS_DEFAULT_RMW` → `NROS_RMW` → `CONFIG_NROS_RMW_*`),
was private to `NanoRosEntry.cmake`.

**The class.** A sweep of configure-time and `nros build` messages that name a
backend-specific knob (`message(STATUS|WARNING|NOTICE)` /
`nros_message_detail` in `cmake/`, `zephyr/cmake/`, and the CLI's
`eprintln!`/`println!`) found one sibling. The cargo-leaf road
(`leaf_entity_env::leaf_env`) prints "payload classes not derived, so
`LARGE_PAYLOADS` keeps the crate default" for every leaf, and `LARGE_PAYLOADS`
is `nros-rmw-zenoh`'s pool. The Zephyr-side zenoh messages in
`zephyr/cmake/nros_cargo_build.cmake` (`NROS_ZENOH_GRAPH_DISCOVERY`,
`NROS_RMW_LOCAL_QUERYABLE`) and `zephyr/cmake/nros_rmw_zenoh.cmake` already sit
inside `if(CONFIG_NROS_RMW_ZENOH)` / the zenoh module, so they are not in the
class.

## Resolution

- `cmake/NanoRosImageRmw.cmake` holds the resolver as
  `nros_resolve_image_rmw()`, moved out of `NanoRosEntry.cmake` with its three
  callers renamed, so there is still one copy. It adds
  `nros_image_reports_backend_knob(<backend> <out>)`, which is TRUE when the
  image is that backend or no backend has been stated. An unknown backend keeps
  the report, because hiding it from the image it does apply to is the worse
  error.
- `NanoRosMessageBounds.cmake` reports the retention-slot refusal only when that
  helper says zenoh, and names it `NROS_ZENOH_TL_RETAIN_BYTES`. The derivation
  and the output file are unchanged.
- The cargo-leaf twin `leaf_reports_zenoh_knob()` resolves each `[image.*]`
  through `SystemToml::resolved_rmw`, the same ladder `nros build` uses, with
  `[image_defaults]` and the `[system] rmw` fallback. A leaf none of whose
  images is zenoh is told that `NROS_SUBSCRIBER_BUFFER_SIZE` keeps its default,
  not `LARGE_PAYLOADS`. The refusal is still reported, because the small class
  is also the executor's receive region on every backend.

Real flow: on the scaffolded Cyclone first project, I instrumented the
derivation for one reconfigure and then reverted the instrumentation. At the
call site the resolver answered `rmw=cyclonedds reports_zenoh=FALSE` on both
passes. `CMakeCache.txt` holds `NROS_RMW:STRING=cyclonedds`.

**Tests**

- `tests/cmake-message-bounds-tests.sh` case R (`just check message-bound-knobs`,
  fast line) covers `NROS_RMW=cyclonedds`, `xrce`, `NROS_DEFAULT_RMW=cyclonedds`
  and `CONFIG_NROS_RMW_CYCLONEDDS=y` (the west arm), which must hide the line,
  and `NROS_RMW=zenoh` and no backend, which must show it under the canonical
  name. Mutation-checked: forcing the report on fails the 4 hidden rows, and the
  old code also fails the canonical-name assertion.
- `leaf_entity_env::tests::a_zenoh_knob_is_reported_only_to_a_zenoh_leaf` covers
  `[system] rmw` alone, an image overriding it in both directions,
  `[image_defaults]`, and one zenoh image among several. Mutation-checked:
  forcing `true` fails it.
