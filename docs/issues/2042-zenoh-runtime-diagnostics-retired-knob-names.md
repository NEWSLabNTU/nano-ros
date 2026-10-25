---
id: 2042
title: "zenoh runtime and build diagnostics tell the user to set a retired ZPICO_* env spelling"
status: open
type: bug
area: [rmw-zenoh, ux]
severity: medium
found: 2026-10-25
related: [phase-484, phase-488]
---

## What

Phase-484 W8 retired the `ZPICO_*` env spellings. Setting one now stops the
build and names its `NROS_ZENOH_*` replacement (`knobs.toml`, the `legacy`
column), and `check-retired-knob-spellings` keeps env readers and writers off
them. The same words survive as the shim's C macro and Rust constant names,
and that is correct.

What the gate does not reach is the TEXT a user reads. Messages that name the
knob to raise still spell the retired env name, so following the advice turns
a runtime warning into a build refusal. Seen on the Autoware port (phase-488
W2) at start-up:

```
[WARN] nros: qos: service '/external_cmd_selector/service/select_external_command' asked for
KEEP_LAST(10); this image's receive ring holds 4. Granting 4 and advertising it to the graph.
Raise ZPICO_SUBSCRIBER_RING_DEPTH to keep more.
```

The env to raise is `NROS_ZENOH_SUBSCRIBER_RING_DEPTH`. Sites found on
2026-10-25 (`git grep -n 'ZPICO_' packages/rmw/zenoh`, reading each message):

- `nros-rmw-zenoh/src/shim/qos.rs`: `ZPICO_SUBSCRIBER_RING_DEPTH`, `ZPICO_TL_RETAIN_DEPTH`;
- `nros-rmw-zenoh/src/shim/publisher.rs`: `ZPICO_MAX_TL_PUBLISHERS`, `ZPICO_MAX_QUERYABLES`;
- `nros-rmw-zenoh/src/shim/service.rs`: `ZPICO_MAX_QUERYABLES` (four messages), `ZPICO_MAX_PENDING_REPLIES`;
- `nros-rmw-zenoh/src/shim/session.rs`: `ZPICO_MAX_LIVELINESS`, `ZPICO_GRAPH_DISCOVERY`;
- `nros-rmw-zenoh/src/zpico.rs`: `ZPICO_MAX_SESSIONS`;
- `nros-rmw-zenoh/build.rs`: `ZPICO_MAX_TL_PUBLISHERS`, `ZPICO_SUBSCRIBER_RING_DEPTH`;
- `nros-zpico-build/src/runner.rs`: `ZPICO_GRAPH_DISCOVERY`.

The "liveliness: declare failed (Full)" log from the same port named
`ZPICO_MAX_LIVELINESS` in the same way.

## Fix the class

Each message names the knob's canonical env spelling, preferably from the
registry (`nros_zephyr_build::knob_registry`, or a constant generated from
`knobs.toml`) rather than as a literal, and also the Kconfig symbol where the
message can be reached on Zephyr. The gate that refuses retired env readers
should also refuse a retired spelling inside a string literal that is not a C
macro or constant definition, with an allowlist of definition sites.

## Done when

No runtime or build message advises a retired spelling, and the gate above
refuses a new one.
