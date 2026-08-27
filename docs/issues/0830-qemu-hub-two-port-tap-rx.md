---
id: 830
title: "A QEMU net hub with only a NIC and a tap never delivers host->guest
  frames; a third hub port fixes it"
status: open
type: limitation
area: boards
related: [phase-385]
---

## Symptom

An `mps3-an536-freertos` guest wired to a host `tap` device transmits fine and
receives nothing. Guest-to-host works end to end — the host's CycloneDDS
discovers the guest participant, and a raw multicast listener on the host
captures the guest's RTPS SPDP — while every frame the host sends is dropped
before the guest's driver ever sees it.

The host is not at fault and neither is the driver. Attaching to `tap1`
directly (`TUNSETIFF`, in place of QEMU) shows the host emitting exactly what
it should:

```
frame 2: 42B 7a:cb:5b:32:4d:46 -> ff:ff:ff:ff:ff:ff ARP who-has 192.0.3.10
frame 9: 486B 7a:cb:5b:32:4d:46 -> 01:00:5e:7f:00:01 IPv4   (RTPS SPDP)
```

Instrumenting `lan9118_lwip_poll()` to print every received frame shows zero
frames on tap, and non-zero frames for the same binary on a `-net socket,mcast`
backend. So the frames leave the host, and never reach the NIC model.

## Trigger

QEMU 11.0.0, `-machine mps3-an536`, with the hub holding exactly two ports:

```
-net nic -net tap,ifname=tap1,script=no,downscript=no
```

`-nic tap,...` (a direct NIC/netdev peering, no hub) fails the same way.

## Workaround

Add a third, unpeered hub port:

```
-net nic -net tap,ifname=tap1,script=no,downscript=no \
-netdev hubport,id=h0,hubid=0
```

Measured back to back, twice each, same binary and same tap:

| wiring | driver RX frames | `ping` guest | neighbour |
| --- | --- | --- | --- |
| two ports | 0 | 100% loss | `INCOMPLETE` |
| three ports | 19–20 | 0% loss | `REACHABLE` |

With the third port, bidirectional ROS 2 interop works: a host
`ros2 topic echo /chatter` receives 39 of the guest's samples, and the guest
logs the host's published value 20 times.

## Cause (likely, not confirmed)

QEMU only polls a backend's fd while the destination reports it can receive.
A hub port with no peer always can, so the third port keeps the hub's
answer true and the tap gets read; with two ports the answer depends solely on
the LAN9118 model, which appears to report "cannot receive" and never
self-clears — the driver cannot drain a FIFO it is never given anything to
drain. Confirming this needs QEMU's `lan9118_can_receive` source, which the SDK
ships only as a binary.

## Notes

`MAC_CR.BCAST` is inverted (SET disables broadcast, which would kill ARP while
leaving multicast working — the exact shape of this symptom). The driver
inherited whatever the reset value held; it now clears the bit explicitly. That
is hardening, NOT the fix for this issue: clearing it changed nothing, and
neither did enabling full promiscuous mode. Both were ruled out before the hub
port was found.

## Cost of not knowing this

Two false leads were pursued and recorded before the wiring was suspected: a
`rt/` topic-name-mangling hypothesis (disproved — the same host `ros2 topic
echo /chatter` now matches the guest's writer with no naming change), and
"host egress is dead", which came from reading a tap's `tx_packets`. That
counter did not move while the interface was demonstrably transmitting, so on
a tun/tap device it is not trustworthy evidence of what left the wire — read
the frames instead.

Also: a leftover QEMU holding the tap makes a new one exit with
`could not configure /dev/net/tun (tap1): Device or resource busy` while the
surrounding test keeps running and reports a clean failure. Two measurements
here were void for that reason. Any tap test should assert the attach
succeeded before believing its own result.
