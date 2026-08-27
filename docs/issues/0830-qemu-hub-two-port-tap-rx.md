---
id: 830
title: "A QEMU net hub with only a NIC and a tap never delivers host->guest
  frames — OUR lan9118 can_receive patch deadlocks before the guest enables RX"
status: open
type: bug
area: boards
related: [phase-385, issue-0836]
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

## Cause — CONFIRMED, and it is OUR patch, not upstream QEMU

The SDK ships a PATCHED qemu (`11.0.0-nros2`, `[tool.qemu.source]` =
`NEWSLabNTU/qemu` branch `nano-ros-v11.0.0-patches`), and the patch is
`lan9118-flow-control.patch` — written for a DIFFERENT symptom, the slirp RX
stall on `mps2-an385` (`docs/research/qemu-lan9118-slirp-rx-stall.md`). It adds
the `lan9118_can_receive` callback that stock QEMU does not have.

The installed binary really does carry it — the symbol exists, and its body is
the patch:

```
$ nm ~/.nros/sdk/qemu/11.0.0-nros2/bin/qemu-system-arm | grep lan9118_can_receive
0000000000539da0 t lan9118_can_receive

$ objdump -d --start-address=0x539da0 …
  539db0:  testb  $0x4,0x24ac(%rax)     ; MAC_CR_RXEN
  539db7:  je     539ddd                ; -> false
  539dbf:  cmp    %ecx,0x390c(%rax)     ; rx_status_fifo_used == size
  539dc5:  je     539ddd                ; -> false
  539dd3:  cmp    $0x17f,%edx           ; free >= 384 words
  539dd9:  setg   %r8b
```

`.nros-provenance` sha256 `a1cb9df6…` matches `dist.linux-x86_64` in
`nros-sdk-index.toml`, so this is the released artifact and not a local build.

### The deadlock

`lan9118_can_receive` returns false while `MAC_CR_RXEN` is clear — which is the
state at guest boot, before the driver's `hw_init()` reaches step 9. From there:

1. `net_hub_port_can_receive` (`net/hub.c`) answers "can any OTHER port
   receive?". With exactly two ports, the tap's only peer is the NIC's port,
   which now answers **false**.
2. `qemu_net_queue_send` sees `!qemu_can_send_packet(sender)`, appends the frame
   and returns **0**.
3. `tap_send` treats 0 as back-pressure and calls `tap_read_poll(s, false)` —
   **the tap fd handler is removed**.
4. Re-arming needs `qemu_flush_queued_packets`, and the patch calls it from
   exactly one place: `rx_status_fifo_pop`, i.e. after the guest pops a received
   frame.

No frame can be received, so no frame is ever popped, so the flush never runs
and the fd handler is never restored. The guest enables `RXEN` a few
microseconds later and nothing notices. **Zero frames, permanently** — which is
what the measurement shows.

The third hub port works because `qemu_can_send_packet` short-circuits on a
peerless client (`if (!sender->peer) return 1;`), so the hub's answer is
unconditionally true and step 3 never fires. It is a wedge under a stuck
callback, not a fix.

This also explains why `-nic tap,...` (direct peering, no hub) fails the same
way: same gating, and no unpeered port to hold the door open.

## Fix

Flush when `RXEN` goes off->on, so enabling RX re-arms the backend — the idiom
every other NIC model already follows. In `do_mac_write`, `case MAC_CR`, which
today only handles the on->off edge:

```c
    case MAC_CR:
        if ((s->mac_cr & MAC_CR_RXEN) != 0 && (val & MAC_CR_RXEN) == 0) {
            s->int_sts |= RXSTOP_INT;
        }
+       if ((s->mac_cr & MAC_CR_RXEN) == 0 && (val & MAC_CR_RXEN) != 0) {
+           qemu_flush_queued_packets(qemu_get_queue(s->nic));
+       }
        s->mac_cr = val & ~MAC_CR_RESERVED;
```

This belongs on the fork branch `nano-ros-v11.0.0-patches` and needs a re-cut
SDK release to reach users, so the third hub port stays the documented
workaround until then. Note the pending `-nros3` re-cut already queued in
`nros-sdk-index.toml` (issue 0368 F3, the libslirp rpath bundle) — this fix
should ride along with it rather than earn its own release.

## What this cost, and the lesson

The original report reasoned to nearly the right mechanism ("the LAN9118 model
appears to report 'cannot receive' and never self-clears") but attributed it to
stock QEMU and closed with "confirming this needs QEMU's `lan9118_can_receive`
source, which the SDK ships only as a binary". Both halves were wrong in the
same direction: the callback is not upstream's, it is ours, and a stripped-looking
binary still answered the question in two commands (`nm`, then `objdump` at the
symbol). **When a hypothesis names a specific function, check whether that
function is one of ours before filing it against upstream** — and a local
`static` symbol usually survives in the symbol table, so "ships only as a
binary" is rarely the end of the road.

## Fix written and VERIFIED — awaiting a fork push and an SDK re-cut

`third-party/qemu/qemu` commit `729262e975` ("hw/net/lan9118: flush queued
packets when RX is enabled") on branch `nano-ros-v11.0.0-patches` implements the
hunk above. It is **committed locally and NOT pushed** — fork remotes need an
explicit allow-rule — so the superproject's submodule pin deliberately still
points at `dbd1049b06`. Push the branch first, then bump the pin, then re-cut
the SDK; until that release exists the third hub port remains the workaround,
and the board's `nros-board.toml` comment stays accurate.

Verified by rebuilding `qemu-system-arm` from that commit and answering ICMP
echo for the `mps3-an536` FreeRTOS/lwIP fixture over a `-net socket` backend
(the socket backend shares the exact gating path — `net/socket.c` disables
`read_poll` on a 0 return just as `tap_send` does — so this reproduces without
needing CAP_NET_ADMIN to make a tap). 30 s per run, same image throughout:

| binary | hub ports | frames sent | ICMP echo replies | verdict |
| --- | --- | --- | --- | --- |
| SDK `11.0.0-nros2` (unfixed) | 2 | 280 | **0** | dead |
| SDK `11.0.0-nros2` (unfixed) | 3 (workaround) | 281 | 259 | works |
| rebuilt with `729262e975` | **2** | 281 | **259** | works |

The middle row is the control: it shows the probe can detect RX at all, and the
fixed two-port run reproduces the workaround's numbers exactly. Repro script:
`tmp/rxprobe.py` (throwaway; it must start sending at t=0, because the deadlock
needs a frame to arrive BEFORE the guest sets `RXEN` — a probe that waits for
the guest to speak first will never trigger the bug and reports a false pass).

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
