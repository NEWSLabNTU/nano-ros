---
id: 2042
title: "zenoh runtime and build diagnostics tell the user to set a retired ZPICO_* env spelling"
status: resolved
type: bug
area: [rmw-zenoh, ux]
severity: medium
found: 2026-10-25
resolved: 2026-10-25
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

## Resolution

Fixed by #2624 (`b3c36d27da`).

- **The messages.** Every prose occurrence of a retired spelling in a Rust
  string literal now names the registry's canonical env spelling. That is 52
  Rust sites, from the zenoh shim's qos, publisher, service, session and
  zpico messages to the build-script warnings of `nros-rmw-zenoh` and
  `nros-zpico-build`, the message-bound comments and the test failure text.
  The 2 cmake twins in `cmake/NanoRosMessageBounds.cmake` were changed too.
  The C macro and Rust constant names (`ZPICO_MAX_QUERYABLES` as a `const`,
  a `-D` or a `#define`) are unchanged, because RFC-0103 keeps them as
  backend-internal names.
- **The gate.** `check-retired-knob-spellings` gained a message rule. A small
  Rust string lexer finds the literals, which run across `\`-continued lines
  that no per-line rule can read. A retired name in a literal that contains
  whitespace is refused, unless it is code (`const X`, `#define X`, `-DX`,
  `X: usize`). The rule has 13 self-test cases.

Two parts of "Fix the class" were not taken:

- **The names are literals, not read from the registry.** The gate makes a
  literal safe: a retired spelling cannot come back unrefused. Reading
  `knobs.toml` from runtime log text would add a build dependency to every
  message site.
- **Messages do not also name the Kconfig symbol.** This was not done in
  #2624. A Zephyr user reads the canonical env name and finds its symbol in
  `knobs.toml`. If that lookup turns out to cost Zephyr users, it is a new
  issue.
