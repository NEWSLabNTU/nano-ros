---
id: 2724
title: "On an aarch64 host the Zephyr `native_sim/native/64` image of `zephyr-byo/app` fails to link: three undefined network symbols"
status: open
type: bug
severity: low
area: [zephyr, build, templates]
found: 2026-11-19
related: [2590]
---

## Symptom

`just check template-copy-out` on an aarch64 Linux host (`uname -m` =
`aarch64`) fails on one template, `zephyr-byo/app`, the only copy built for
`native_sim/native/64` (with the `nros-nsos` snippet). Every other template
passes on the same host, including the four Zephyr `mps2/an385` copies.
`west build` compiles and fails at the native-simulator link:

```
zephyr/lib/posix/options/net.c:222: undefined reference to `zsock_getnameinfo'
zephyr/subsys/net/l2/ethernet/ethernet.c:934: undefined reference to `net_mgmt_NET_REQUEST_ETHERNET_SET_TXINJECTION_MODE'
zenoh-pico/src/link/transport/udp/address.c:38: undefined reference to `_z_udp_unicast_endpoint_init'
collect2: error: ld returned 1 exit status
```

(The `.eh_frame` "no .eh_frame_hdr table will be created" lines above
them are noise.)

## Not caused by a change: main reproduces it

On 2026-11-19 it first showed on PR #4288 (issue 2590). The same template,
copied from main at the PR's merge base and built with that tree's own `nros`
(`nros configure && nros build` in the copy), fails with the same three
symbols. 2590 touches nothing in Zephyr networking or zenoh-pico.

## Where to look

- The three symbols come from three layers: Zephyr's POSIX `getnameinfo`
  shim, the ethernet L2, and zenoh-pico's UDP unicast transport. That points
  at a Kconfig set that differs on this host (sockets, offload, L2 or
  zenoh-pico feature flags), not at one missing object. Read the build's
  merged `.config` beside an x86_64 build of the same copy.
- `native_sim/native/64` on an arm64 Linux host is less exercised than on
  x86_64. Confirm the board and the `nros-nsos` snippet are supported there
  before treating this as our defect. If they are not, the template check
  should refuse `native_sim` on this host as an unmet precondition rather
  than report a broken template.

## Effect

`template-copy-out` is red on every aarch64 host whatever the change, so a
local run there cannot vouch for `zephyr-byo/app`. CI runs on x86_64 and is
unaffected.
