---
id: 1818
title: "`just zenohd locator=tcp/…` handed the router the literal string `locator=tcp/…`, which it reported as `Unicast not supported for locator=tcp protocol`"
status: resolved
type: bug
severity: low
area: [tooling, rmw-zenoh]
related: [0654, 0774, 1305]
found: 2026-10-11
---

## Measured

While measuring issue 1305's board switch, the router would not start:

```
$ just zenohd locator=tcp/0.0.0.0:10500
ERROR zenohc::session: Error opening session: Unicast not supported for locator=tcp protocol at …/zenoh-link/src/lib.rs:183.
```

The report read this as an environment fault, "likely the loader/libzenohc
class in issue 0774", and left the runtime check unverified. That was wrong. The
paired router starts on the same host:

```
$ just zenohd tcp/127.0.0.1:17447
Started Zenoh router with id …
```

## Why

`just` recipe parameters are positional. Only top-level variables accept
`name=value`. So `just zenohd locator=tcp/…` does not set `locator`: it passes
the string `locator=tcp/…` as its value. `nros_router_exec` put that string into
`listen/endpoints`, the router split it at the first `/`, and it reported the
protocol as `locator=tcp`. The message names a protocol and the word
"unsupported", so it reads like a router built without TCP, or like 0774's
unpaired `libzenohc`, rather than a typo on the command line.

## Fix

`nros_router_exec` (`scripts/dev/zenohd.sh`) checks the locator before it
resolves the router: `<protocol>/<address>`, with a lower-case protocol name
(`tcp`, `udp`, `serial`, `unixsock-stream`). Anything else is refused with exit 2.
When the protocol part contains `=`, the refusal names the positional spelling:

```
error: 'locator=tcp/127.0.0.1:17449' is not a zenoh locator (<protocol>/<address>, e.g. tcp/127.0.0.1:7447)
       `just` parameters are positional: write `just zenohd tcp/127.0.0.1:17449`,
       not `just zenohd locator=tcp/127.0.0.1:17449`.
```

The check sits in `nros_router_exec` rather than in the `just zenohd` recipe,
so it covers all eight per-platform `just <plat> zenohd` recipes and every
`ZENOH_LOCATOR`-driven script too. Checked by hand:

- accepted: `serial/dev/ttyUSB0#baudrate=115200`, `udp/1.2.3.4:5`,
  `unixsock-stream//tmp/x`;
- refused: `127.0.0.1:7447`, `TCP/x`.
