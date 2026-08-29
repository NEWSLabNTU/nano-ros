---
id: 888
title: "FreeRTOS is the only platform arm with no AllowMulticast — it inherits a
  default nobody wrote down"
status: resolved
type: bug
area: rmw
related: [issue-0836, phase-385]
---

## What

`kEmbeddedCycloneConfig` in `nros-rmw-cyclonedds/src/session.cpp` states a
multicast policy for two of its three platform arms and not the third:

| arm | AllowMulticast | why |
| --- | --- | --- |
| ThreadX | `spdp` | NetX enables IGMPv2, virtio-net accepts all multicast; discovery multicast, data unicast |
| native_sim | `false` | multicast breaks Cyclone's select-based waitset there |
| **FreeRTOS** | **unset** | — falls through to the Cyclone default |

The Cyclone default is multicast for data as well as discovery. So a FreeRTOS
image advertises multicast data locators, and peers may address data to them —
observed on the ASI an536 lane, where a bridge-side trace shows
`writer_hbcontrol: wr … multicasting`.

That may even be the behaviour someone wants. The defect is that it was never
chosen: two arms argue their case and the third inherits silently, so the
difference is invisible until a trace shows it.

## Not a platform limitation

Worth stating, because the obvious guess is that FreeRTOS simply cannot do
multicast and the omission is a tacit "no". It can:

* `LWIP_IGMP = 1` in the FreeRTOS lwIP options
* the netif is registered with `NETIF_FLAG_IGMP`
* the LAN9118 driver enables `MAC_CR_MCPAS` (pass-all-multicast) specifically
  so IGMP-joined groups reach lwIP
* and empirically SPDP discovery over `239.255.0.1` works — it is how an
  an536 island and a host ROS 2 peer find each other at all

## Fix

State the policy: `spdp` — discovery over multicast, data unicast, matching the
ThreadX arm. This is also what a ROS 2 peer talking to an embedded island
typically configures on its own side, so the two ends agree by default rather
than by accident.

## Not the cause of 0836

Checked, because the timing invited the conclusion. With the peer's own domain
policy already `spdp`, data was going unicast regardless: of the destinations in
a bridge-side trace, 97,976 sends went to `udp/192.0.3.10:58376` and 69 to
`udp/239.255.0.1` (all discovery). Setting this on the island changed nothing
about the missing trajectory. It is a correctness and legibility fix, not a
delivery fix.
