---
rfc: 0083
title: "CAN unicast over ISO-TP: full ROS semantics on a CAN bus"
status: Draft
since: 2026-08
last-reviewed: 2026-08-27
implements-tracked-by: [phase-393, phase-394]
supersedes: []
superseded-by: null
---

# RFC-0083 — CAN unicast over ISO-TP

> [RFC-0080](0080-can-link-for-zenoh-pico.md) and
> [RFC-0081](0081-can-link-for-zenoh-rs.md) built a **multicast** CAN link and
> proved it: ROS 2 topics cross a CAN bus, and a zenoh-pico peer interoperates.
> They also found the ceiling — services, actions, parameters and graph
> introspection do not work. This RFC is the way past that ceiling, and the
> ceiling turns out not to be CAN's.

## 1. The limitation is multicast's, not CAN's

RFC-0081 §3 recorded that queries and liveliness do not route to a multicast
face: `mcast_groups` appears only in `hat/*/pubsub.rs`, never in `queries.rs`,
`token.rs` or `interests.rs`. rmw_zenoh resolves services through queries and
builds the ROS graph from liveliness tokens, so both die.

**Measured on stock ROS 2 with no CAN involved at all**, to establish that this
is a property of zenoh's transport kind rather than of our link:

| stock rmw_zenoh over | topic | service |
| --- | --- | --- |
| UDP **multicast** (`udp/224.0.0.224:7447`) | 13/13 | **0 delivered — "service not available"** |
| UDP **unicast** (`udp/127.0.0.1:7448`) | — | **`Result of add_two_ints: 5`** |

The same library, the same node binaries, the same day. The only variable is
whether the locator names a multicast address, because that is precisely what
`LocatorInspector::is_multicast()` tests.

So: a unicast CAN link restores every ROS feature the multicast one cannot
carry. The two are complementary, not competing — multicast is a **bus** for
pub/sub across many peers; unicast is a **link** with full semantics to one.

## 2. CAN has no addressing, so ISO-TP manufactures it

A CAN frame carries an identifier and at most 8 bytes. The identifier names the
*message*, not a destination: there is no source field, no destination field,
and every node hears every frame. Unicast must therefore be built by
convention — which is what ISO-TP, J1939 and CANopen all do.

ISO 15765-2 builds it from **an identifier pair plus flow control**:

* **SF** — single frame, payload up to 7 bytes
* **FF** — first frame, carrying a 12-bit total length
* **CF** — consecutive frame, with a 4-bit sequence number
* **FC** — flow control: CTS/WAIT/OVFLW, block size, minimum separation time

The receiver **must** answer a first frame and then paces the sender. On a
broadcast medium two responders would collide, so exactly one peer may own the
other end of an identifier pair. **That pairing is the address**, and the flow
control is what makes it unicast rather than convention alone.

Two consequences follow, and they are the whole reason this RFC exists:

* **The MTU becomes 4095 bytes instead of 7.** Segmentation moves below zenoh.
  zenoh's ~16 bytes of per-fragment overhead fall from 25% of a frame to under
  0.5% of a PDU — which is what makes **classic CAN usable**, and classic CAN is
  what most hardware in the field actually is.
* **It stops being a bus.** One link is one peer pair. N peers means N
  identifier pairs and N links, with a publisher's traffic duplicated per peer.

RFC-0080 §2 rejected ISO-TP, correctly, for a *multicast datagram* link: there,
zenoh's own fragmentation to a 63-byte MTU did the job and ISO-TP would have
been redundant work. For unicast the calculus inverts — ISO-TP is what supplies
both the addressing and the MTU.

## 3. ISO-TP is a platform capability, not something we implement

This is the organising decision, and it is already how zenoh-pico is built:
each platform supplies its own `src/system/<platform>/network.c`.

| platform | ISO-TP from | licence |
| --- | --- | --- |
| Linux | `socket(PF_CAN, SOCK_DGRAM, CAN_ISOTP)`, kernel 15765-2:2016 | in-kernel |
| Zephyr | `isotp_bind` / `isotp_send` / `isotp_recv`, `subsys/canbus/isotp` | Apache-2.0 |
| FreeRTOS, NuttX, ThreadX, ESP-IDF, bare metal | vendored `SimonCahill/isotp-c` | **MIT** |

Both ends of the safety island are therefore covered by conformant code we
neither write nor own.

### Why the vendored library, specifically

Surveyed by compiling the candidates rather than reading their claims.
`SimonCahill/isotp-c` is the only one that is simultaneously permissive, pure
C11, freestanding, allocation-free, actively maintained and seriously tested
(87 test cases with a CI coverage gate). Measured with
`arm-none-eabi-gcc -Os -mcpu=cortex-m4`: **2421 bytes of text, 96 bytes of RAM
per link**. Its only undefined symbols are `isotp_user_send_can`,
`isotp_user_get_us`, `isotp_user_debug`, plus `memcpy`/`snprintf`/`assert` — no
threads, timers, sockets, allocator or OS headers. That is exactly zenoh-pico's
platform-abstraction shape. It also brings CAN FD and the 15765-2:2016 32-bit
`FF_DL` escape, both of which we would otherwise write.

**Zephyr's implementation cannot be lifted out**, despite being the most
conformant of all of them: 1391 lines containing 56 `net_buf`, 19 `k_timer`,
15 `k_work`, 7 `k_sem`, and `struct device` in the public signature. Porting it
means reimplementing Zephyr's buffer, work-queue and timer subsystems — a
rewrite, not a vendoring. It is used natively on Zephyr and nowhere else.

**Two licence traps worth naming**, because both are easy accidental picks:
`devcoons/iso15765-canbus` is the most popular-looking result at 195 stars and
is **AGPL-3.0** (changed in a December 2024 commit); `altelch/iso-tp` is
GPL-3.0. Neither is usable here. `openxc/isotp-c`, at 358 stars, has
multi-frame transmit as a stub that logs *"Only single frame messages are
supported"*.

### Addressing mode: settled by the common denominator

No portable candidate implements extended or mixed addressing; the kernel and
Zephyr both do. **Normal addressing is therefore the interoperable common
denominator** and the only mode this design uses — an evidence-based default
rather than a deferred maybe. Extended addressing is a documented non-goal.

Also honest: no portable library implements `N_As`/`N_Ar`; they assume the send
callback is synchronous. On a platform whose CAN driver sends asynchronously,
the hook must either block until transmit-confirm or grow a timer. That is a
per-platform decision, not a library defect.

## 4. The zenoh-rs link

A new crate `zenoh-link-isotp`, **unicast**, alongside the existing multicast
`zenoh-link-can`. They coexist and do different jobs.

| property | value | why |
| --- | --- | --- |
| MTU | 4095 | the classic first frame carries a 12-bit length; universally supported |
| `is_streamed` | `false` | ISO-TP preserves message boundaries |
| `is_reliable` | `false` | flow control, but a lost consecutive frame aborts the PDU |
| `supports_priorities` | `true` when configured | one ISO-TP socket per class, identifiers priority-major |

Endpoint grammar, ISO-TP normal addressing — a directed identifier pair, which
is how 15765-2 defines a point-to-point channel:

```
isotp/<device>#tx_id=0x7E0;rx_id=0x7E8;prio_classes=1
```

**Listening needs no `accept()`.** `zenoh-link-serial` already proves the shape:
`new_listener` constructs a single link and hands it to the manager on first
contact. There is no socket demux to do, because the kernel already filters by
identifier.

**This link is serial-shaped**, and that is the strongest argument for it
upstream: an unreliable, non-streamed, point-to-point unicast link is a kind
zenoh already ships — and `rmw_zenoh` enables it, passing
`--features=shared-memory zenoh/transport_serial` in `zenoh_cpp_vendor`. We are
adding a link of a shape ROS already runs, with a better MTU and real flow
control.

## 5. The zenoh-pico link, and why RFC-0080's blocker does not apply

RFC-0080 rejected unicast on evidence: `_zp_unicast_accept_task` opens by
requiring `_z_link_get_socket()` and then calling `_z_socket_accept()`, and a
bus has neither.

That evidence does not transfer, for two reasons found by reading the tree:

* the accept task is compiled only when `Z_FEATURE_UNICAST_PEER == 1`;
* pico's own serial link — its existing unicast link on a non-accept medium — is
  registered **only in `_z_open_link`**, the connect side.

So a unicast island **dials out and never listens**, and never reaches the
accept path at all. RFC-0080 hit that wall because multicast peers all listen;
it is a property of the multicast design, not of CAN or of pico.

The island connects, the Linux host listens. That is also the natural ROS
topology, and it is what gives the island services, actions, parameters and
graph introspection.

## 6. What it costs

* **It is a link, not a bus.** The four-peers-on-one-wire property of the
  multicast link does not exist here, and identifier pairs scale with peers.
* **No interoperability with the multicast CAN link.** Different transport,
  different framing. A deployment picks one per bus, or runs both on disjoint
  identifier bands.
* **`is_reliable = false` is real.** One lost consecutive frame aborts an entire
  PDU, so a 4 KiB message dies on a single frame loss. This argues for not
  raising the MTU to its maximum merely because ISO-TP permits it.
* **`N_Cr`/`N_Bs` timeouts surface as link errors** that fail the transport,
  which is correct for unicast and the opposite of the multicast link, where
  dropping a frame is routine.

## 7. Out of scope

Extended and mixed addressing (§3). Hardware validation, which remains
untouched across all of these phases. The upstream PRs, which are the campaign's
later phases. And any change to the existing multicast link, which keeps working
and keeps its own use case.
