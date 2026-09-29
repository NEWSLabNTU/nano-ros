---
id: 1574
title: "The Zephyr lane still compiled zenoh-pico's 10 s lease, so a Zephyr TCP image dropped off a stock rmw_zenohd ~30 s after it opened"
status: resolved
type: bug
area: zephyr, rmw-zenoh, interop
related: [issue-0906, issue-1056, issue-0924]
resolved_in: "zephyr/Kconfig NROS_ZENOH_LEASE_MS default 60000 + check-zenoh-lane-ownership (13)"
---

## What happened

Issue 0906 found that zenoh-pico measures router silence against
`min(router lease, Z_TRANSPORT_LEASE)` (`src/transport/unicast/transport.c`,
the OpenAck handling, unchanged from upstream 1.8.0), while the router ROS
ships (`rmw_zenohd`, `DEFAULT_RMW_ZENOH_ROUTER_CONFIG.json5`) announces
`lease: 60000` with `keep_alive: 2` -- one keepalive every 30 s on an idle
link. At a 10 s lease the client decides the router is gone after two silent
lease periods and closes with `Close(reason 5, EXPIRED)`. 0906 fixed it by
moving the constant the CARGO lane compiles in (`nros-zpico-build`,
`Z_TRANSPORT_LEASE_MS = 60_000`).

The Zephyr image does not get zenoh-pico from the cargo lane. The west lane
(`zephyr/cmake/nros_rmw_zenoh.cmake`) turns `CONFIG_NROS_ZENOH_LEASE_MS` into
`Z_TRANSPORT_LEASE`, and that Kconfig default stayed at zenoh-pico's upstream
10000. Every Zephyr TCP image that did not state the knob kept the 0906 bug.

## Measured (simple-autoware-safety-island phase8-W15)

QEMU mps2/an385, Zephyr 4.4, zenoh-pico 52f60b79 (1.8.0 + the nano-ros
line), client mode, over SLIRP guestfwd through a TCP tap to a stock
`rmw_zenohd` (ros-humble-rmw-zenoh-cpp 0.1.9, zenoh 1.8.0). No host peer:

    19.463 c1 isl->rtr OPEN len=53 hex=420a00...      (island: lease 10 s)
    19.463 c1 rtr->isl OPEN len=6 hex=623c85c7e330    (router: lease 60 s)
    49.652 c1 rtr->isl KEEPALIVE len=1 hex=04
    49.676 c1 isl->rtr CLOSE len=2 reason=5 hex=2305
    49.678 c1 isl->rtr INIT len=24 syn

The CLOSE is not on the wire at 2 x 10 s. It leaves 24-50 ms after the NEXT
byte the router sends -- here its 30 s keepalive; with a host `ros2 node
list` joining at OPEN+24 s, the peer's liveliness declarations forwarded to
the island (`rtr->isl FRAME` at 28.401) and CLOSE at 28.451. That is what
made the drop look like "a host peer joining kills the island".
`_zp_unicast_failed` joins the read task before it sends the Close, and the
read task returns only when data arrives; the expiry itself was decided
earlier.

One change at a time, same 10 s image:

| change | result |
| --- | --- |
| stock router, no peer | CLOSE reason 5 at OPEN+30.2 s |
| stock router, peer at OPEN+24 s | CLOSE reason 5 50 ms after the peer's frames |
| router `keep_alive: 6` (10 s cadence) | held 88 s, 3 host peers, nodes listed each time |
| router gossip `enabled: false` | CLOSE reason 5 at OPEN+30.2 s |
| router rmw_zenoh_cpp 0.1.10 (same zenoh 1.8.0) | CLOSE reason 5 |
| graph discovery off, router 0.1.10 | CLOSE reason 5 at OPEN+30.1 s, 1 ms after the keepalive |
| `CONFIG_NROS_ZENOH_LEASE_MS=60000`, stock router (0.1.9, and again 0.1.10) | held 77 s and 97 s, keepalives at +30/+60/+90 s, 4 of 4 host node lists each |

Gossip and router version change nothing: the island is a CLIENT
(`whatami: client` in the router log), and the zenoh config itself says
"instances in client mode do not participate in gossip". The lease is the
variable.

## Fix

- `zephyr/Kconfig`: `NROS_ZENOH_LEASE_MS` defaults to 60000, the cargo
  lane's value, with the reason in the help text.
- `scripts/check-zenoh-lane-ownership.py` (13): the Kconfig defaults of
  `NROS_ZENOH_LEASE_MS` / `NROS_ZENOH_LEASE_FACTOR` must equal the literal
  `Z_TRANSPORT_LEASE` / `Z_TRANSPORT_LEASE_EXPIRE_FACTOR` that
  `config_header()` emits. Red on main's Kconfig, green with this change.

A board that states another value in its own conf is untouched; this moves
the default only.
