---
id: 1485
title: "`NROS_PARAM_SERVICE_INBOX_BYTES` uses `0` as its derive sentinel while
  its readers treat `0` as a stated size, and `DECLARED_APP_QUERYABLES =
  usize::MAX` zeroes the builtin inbox on every Zephyr image"
status: open
type: bug
area: [zephyr, rmw, zenoh, params, sizing]
severity: high
found: 2026-09-24
related: [1352, phase-461, phase-467, rfc-0065]
---

## What happens

Two defects, one symptom: on the Autoware Safety Island (a Zephyr west image
whose contract declares `params:` on four nodes) the parameter family's inbox
is sized and then allocated zero times, and the 24 parameter-service
queryables fall through to 24-byte user-service rings that no
`set_parameters` request fits.

### 1. Four sites disagree about what `0` means

Verified against `origin/main` at 41dd9a93d.

1. `zephyr/Kconfig:1026-1043` -- `default 0`, and the help says "Leave it at 0
   to DERIVE it" (line 1034).
2. `zephyr/cmake/nros_cargo_build.cmake:1230-1236` guards the two sibling
   inbox knobs (`NROS_SERVICE_INBOX_BYTES`, `NROS_ACTION_INBOX_BYTES`) against
   the `-1` sentinel; `:1242` forwards THIS one unconditionally:

   ```cmake
   _nros_resolve_knob(NROS_PARAM_SERVICE_INBOX_BYTES "${CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES}")
   ```

   So every Zephyr image exports `NROS_PARAM_SERVICE_INBOX_BYTES=0`.
3. `packages/core/nros-node/build.rs:567-573` probes with `usize::MAX` for "not
   stated", so the forwarded 0 sets `PARAM_SERVICE_INBOX_STATED = true` with
   `PARAM_SERVICE_INBOX_BYTES = 0`. The generated doc on that const
   (`:1012-1016`) says "0 is an absence, not a size". The consumer
   (`packages/core/nros-node/src/param_sizing.rs:470`) takes the stated branch
   and returns 0, and the build-time gate `inbox_fits` (`:532-533`,
   `slot >= worst`) cannot catch it on an image with no declared shape,
   because `0 >= 0`.
4. `packages/rmw/zenoh/nros-rmw-zenoh/build.rs:148-151` reads the same knob
   with the derivation as the DEFAULT, so the forwarded literal 0 wins and
   becomes `BUILTIN_INBOX_BYTES`.

The phase-461 plan (`docs/roadmap/phase-461-service-inbox-per-family.md:201`)
specified "`-1` DERIVE sentinel like `NROS_PARAM_SERVICE_BUFFER_SIZE`"; the
Kconfig row landed with 0 instead.

The island did not see (1)-(4) as a 0-byte slot only because its board file
stated `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES=1016` by hand. That line is a
count the contract determines, and it went unused for the reason below.

### 2. `DECLARED_APP_QUERYABLES = usize::MAX` on the Zephyr road

The island's generated `buffer_config.rs` (MR-CANHUBK344, 2026-09-24):

```
pub const SERVICE_INBOX_BYTES: usize = 24;
pub const ACTION_INBOX_QUERYABLES: usize = 0;
pub const BUILTIN_INBOX_BYTES: usize = 1016;
pub const BUILTIN_INBOX_DEPTH: usize = 1;
pub const DECLARED_APP_QUERYABLES: usize = usize::MAX;
```

`declared_app_queryables` (`packages/rmw/zenoh/nros-rmw-zenoh/build.rs:468-478`)
reads `NROS_DECLARED_SERVICE_SERVERS` and returns the `usize::MAX` "nobody
said" sentinel when it is absent, and the sentinel is emitted as a constant
(`:296-299`, `:341`). The shim then computes

```rust
// packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs:367-370
const BUILTIN_INBOX_PER_SESSION: usize = const_min(
    ZPICO_MAX_QUERYABLES - const_min(DECLARED_APP_QUERYABLES, ZPICO_MAX_QUERYABLES),
    ZPICO_MAX_QUERYABLES - ACTION_INBOX_PER_SESSION,
);
```

which is 0. `NROS_DECLARED_SERVICE_SERVERS` is produced only by
`nros_entity_facts_env` (`cmake/NanoRosEntityFacts.cmake:850-853`), the CMake
road. A Zephyr west entry sizes its queryable table on the RESOLVER road
(`NROS_DERIVED_MAX_QUERYABLES` -> `NROS_RESOLVED_NROS_MAX_QUERYABLES`, 26 on
the island: 2 application servers + 24 parameter services) and never runs that
function, so the application's share of the table reaches nobody. The absence
of a DELIVERY reads as the absence of a DECLARATION: the image declares the
parameter family, and the builtin table is empty exactly as on an image that
declares nothing.

All 26 queryables then draw from the user-service table (`draw_shim_ring`
falls through to it), whose slot is the derived 24 B `OperateMrm` request. A
`set_parameters` request is dropped with the overflow flag.

## Fix

- `-1` is the sentinel (Kconfig default and help), and the cmake row carries
  the same guard as its two siblings, so a derived image forwards nothing.
- Both readers probe for "not stated" with a value no rung can produce, and a
  stated `0` is REFUSED with the same text in both build scripts (RFC-0065 D2):
  "NROS_PARAM_SERVICE_INBOX_BYTES=0: 0 is not a size; -1 derives."
- `nros-node` emits `PARAM_SERVICE_INBOX_BYTES: Option<usize>` in place of the
  `0` + `PARAM_SERVICE_INBOX_STATED` pair.
- `nros-rmw-zenoh` emits `DECLARED_APP_QUERYABLES: Option<usize>`; `None` is
  the undeclared case, and no sentinel is ever an emitted constant.
- The entity inventory publishes `NROS_ENTITY_APP_QUERYABLES`, the
  application's share of the same `NROS_DERIVED_MAX_QUERYABLES` derivation
  (servers, three per action server, and the transient-local cache
  queryables), and the Zephyr resolver forwards it beside the per-kind counts.
  `declared_app_queryables` reads it after the CMake road's carriers.

On the island's declared shapes (`5:55`, `4:53`, `12:272:2:37`, `4:57`, all
other fields 0) the worst request is the `12:272:2:37` node's `set_parameters`:
11 (header + sequence) + 368 (272 name bytes + 12 x 8) + 636 (12 x 53) = 1015 B,
rounded to 1016 for slot alignment. That number now comes out of the
derivation with no inbox line in the board file.
