---
id: 1856
title: "The carved parameter home reserves the six services' sets and buffers in an image that only holds a store, so a store-only image paid ~23 KiB of empty service state (the island's DTCM overflowed)"
status: resolved
resolved_in: "issue 1856 PR (nros_params::PARAM_SERVICES_SERVED sizes the home's services half)"
type: bug
area: memory, params, node
related: [1812, 1702, 1706, phase-382]
---

## Resolution

`nros-params`' build script now exports `PARAM_SERVICES_SERVED`: false only
when the build STATES `NROS_CAPABILITY_PARAM_SERVICES=0`, which the Zephyr
west road does for every image whose bringup does not declare
`param_services` (`CONFIG_NROS_CAPABILITY_PARAM_SERVICES`, forwarded as 0 or
1 by `nros_cargo_build.cmake`). `nros-node` sizes the services' half of the
carved home from it:

- `PARAM_SERVICE_SETS` (the inline `ParamState::services` capacity) is
  `MAX_SERVICE_SETS` when served and 0 otherwise;
- `HOME_BUFFER_BYTES` (the carved request/reply pair) is
  `param_service_buffer_bytes()` when served and 0 otherwise;
- `register_parameter_services` and `enable_parameter_events_on` return
  `Transport(Unsupported)` with a log line naming the knob in a build that
  states the services off, instead of failing later on a full table.

The store itself is untouched: its slots and the state that borrows them are
carved exactly as before. A build that states nothing keeps issue 1812's full
home, so an image WITH the services still asks the heap for nothing. The
served flag is part of `nros-params`' store-shape fingerprint (`.s1` / `.s0`),
so the sizes probe re-runs when it moves.

Tests: `executor::tests::the_carved_home_reserves_the_services_only_when_the_image_serves_them`
runs in both builds (`node-std-tests` adds the stated-off run), and
`scripts/check-param-services-heap-free.sh` gains a store-only, heap-free row.

### Measured (thumbv7em-none-eabihf, the safety island's image)

`-Zprint-type-sizes` on the island's own `nros-cpp` build (22 store slots,
four node slots, `NROS_CAPABILITY_PARAM_SERVICES=0`):

| | before | after |
| --- | --- | --- |
| `ParamHome` | 24,744 B | 1,472 B |
| `NROS_EXECUTOR_SIZE` | 76,176 B | 52,904 B |
| `NROS_CPP_EXECUTOR_STORAGE_SIZE` | 77,048 B | 53,776 B |

## Problem

Issue 1812 carved the parameter services' home (`ParamHome`: the state with
its service sets inline, the request/reply buffer pair and the staging slot)
from the executor backing whenever the sizing carves a STORE (`params > 0`).
A store is also implied by a contract that declares parameters, with no
`param_services` axis: the image holds parameters and serves none. Such an
image paid for four empty service sets and the buffer pair. 1812 recorded the
cost as "small beside the store it rides with", which held for the 285 KB
default store and not for a contract-derived one.

On the safety island (22 declared parameters, `param_services` not declared)
the C++ executor storage, which lives in DTCM, grew from 52,056 B to 77,048 B
on the pin that brought 1812 in, and the link failed with `region 'DTCM'
overflowed by 22608 bytes`. Attributed with `-Zprint-type-sizes` on the
image's own build: `ParamHome` was 24,744 B of the 24,992 B growth (4 x
3,596 B of service sets, 2 x 4,444 B of buffers, 596 B of event publisher and
the rest of the state).

The build already knew: the west road states the axis for every image, and
the entry calls `register_parameter_services` only when it is declared.
