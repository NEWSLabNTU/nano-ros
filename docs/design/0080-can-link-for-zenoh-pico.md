---
rfc: 0080
title: "A CAN link for zenoh-pico: a datagram link type, self-contained in the vendored tree"
status: Draft
since: 2026-08
last-reviewed: 2026-08-23
implements-tracked-by: [phase-377]
supersedes: []
superseded-by: null
---

# RFC-0080 — A CAN link for zenoh-pico

> Opened from the Autoware safety island bring-up on MR-CANHUBK344 (S32K344 +
> Zephyr). The board's Ethernet is one 100BASE-T1 port and the MCU's GMAC has no
> RGMII, so Ethernet is capped at 100 Mbit and needs a media converter to reach
> any normal host. The board also carries **six CAN FD ports** — the thing it is
> actually built for. This RFC asks what it takes to run the RMW over one of them.
> **[OPEN]** marks unresolved points.

## 1. Position

**The CAN link belongs in zenoh-pico, not in nano-ros.**

zenoh-pico already defines a general link abstraction with several
implementations (TCP, UDP, serial, BT, WS, TLS, raweth) across twelve platform
layers. A CAN link is another instance of that abstraction, not a nano-ros
concept. nano-ros becomes a *consumer*: one Kconfig entry that sets
`Z_FEATURE_LINK_CAN=1`, exactly as `NROS_ZENOH_LINK_SERIAL` does today.

Consequences we want:

* the link is upstreamable to `eclipse-zenoh/zenoh-pico` on its own merits;
* every zenoh-pico consumer gets it, not just nano-ros;
* nano-ros carries no transport-specific code, so portability across RMWs and
  boards is unaffected;
* the host side can be a plain zenoh-pico peer for testing, before any
  zenoh-rs work exists.

An earlier sketch routed this through `_Z_LINK_TYPE_CUSTOM` and a
`nros_zpico_custom_take()` hook. That is rejected — see §3.

## 2. CAN is a datagram link, and that decides most of the design

zenoh-pico links declare themselves:

```c
zl->_type             = _Z_LINK_TYPE_CAN;
zl->_cap._transport   = Z_LINK_CAP_TRANSPORT_MULTICAST;  /* see the RESOLVED section */
zl->_cap._flow        = Z_LINK_CAP_FLOW_DATAGRAM;
zl->_cap._is_reliable = false;
zl->_mtu              = <see §4.1>;
```

`FLOW_DATAGRAM` is the load-bearing choice. CAN is frame-based, bounded and
self-delimiting, which is what that capability describes — and zenoh's transport
already fragments to the link MTU:

```c
/* src/transport/common/tx.c */
uint16_t mtu = (zl->_mtu < Z_BATCH_UNICAST_SIZE) ? zl->_mtu : Z_BATCH_UNICAST_SIZE;
```

with `Z_FEATURE_FRAGMENTATION` splitting anything larger. A 718-byte
`nav_msgs/Odometry` therefore becomes ~12 CAN FD frames **handled by zenoh's own
transport**, not by the link.

Two things fall out:

* **No ISO-TP.** Segmentation and reassembly are not the link's problem. This
  was the single largest piece of imagined work and it does not exist.
* **No codec file.** `src/protocol/codec/serial.c` exists because a byte stream
  must be framed — length, escaping, CRC32. CAN frames arrive delimited with a
  hardware CRC, so a datagram CAN link needs no codec at all.

The precedent is in the tree: `_Z_LINK_TYPE_IVC` (NVIDIA Tegra IVC) is a non-IP,
point-to-point, datagram link whose declaration is the five lines above. It is
the template.

### `_is_reliable` = false

CAN is arguably reliable at frame level — CRC, ACK slot, automatic
retransmission on error. It is not reliable end-to-end: frames are lost to
controller buffer overrun, and a bus-off condition drops everything. IVC makes
the same call. Start `false` and let zenoh's own reliability do its job;
revisiting this is a measurement, not a decision to take up front.

## 3. Why not the CUSTOM link

`_Z_LINK_TYPE_CUSTOM` exists and is runtime-pluggable, which makes it the
obvious shortcut. It declares:

```c
/* src/link/unicast/custom.c */
zl->_cap._flow = Z_LINK_CAP_FLOW_STREAM;
```

A **stream**. Routing CAN through it means presenting an ordered, gap-free byte
stream over frames — i.e. building ISO-TP-style segmentation, reassembly and
flow control inside the link, then having zenoh fragment *again* on top. Worse
latency, a reassembly buffer per peer, and a private wire format that no one
else can interoperate with.

The shortcut is more work than the proper link. Rejected.

## 4. [OPEN] Wire-format decisions

These are baked into the wire and must be settled before implementation.

### 4.1 DLC quantisation and MTU

CAN FD payloads are not arbitrary: DLC steps are 0–8, 12, 16, 20, 24, 32, 48,
64. A 40-byte datagram travels in a 48-byte frame, so the receiver cannot infer
length from the frame alone.

Options:

| | MTU | cost |
| --- | ---: | --- |
| **(a)** 1-byte length prefix in payload | 63 | one byte per frame; receiver trivial |
| **(b)** derive length from zenoh's own framing | 64 | link must parse zenoh headers — layering violation |
| **(c)** pad and let the upper layer ignore | 64 | zenoh datagram semantics say a read returns *the* datagram; padding breaks that |

**Proposed: (a).** One byte, no layering violation, receiver is a memcpy.
Classic CAN (8-byte) falls out of the same scheme at MTU 7.

### 4.2 Addressing

CAN is a shared bus; zenoh links are point-to-point. For two nodes a fixed
TX/RX identifier pair is enough, carried in the endpoint config:

```
can/can0#bitrate=500000;dbitrate=2000000;tx_id=0x100;rx_id=0x101
```

**[OPEN]** Multi-peer needs an identifier allocation scheme, and CAN identifier
value *is* bus priority — a design decision with real-time consequences, not a
free choice. Not built now, but **the endpoint syntax must not foreclose it**:
keep identifiers explicit rather than derived, so a future `peers=` or
`id_base=` extends the same grammar.

### 4.3 [OPEN] Bus sharing

If the island's CAN bus also carries ordinary vehicle signals, zenoh traffic
must not starve them. Identifier choice sets priority, and a large fragmented
message occupies the bus for many frames. Whether zenoh should hold a low
priority band, and whether an inter-frame gap is needed, is unresolved and
should be settled with a measurement on a loaded bus.

## 5. File layout

Everything below is inside the vendored zenoh-pico tree, following the existing
per-link pattern.

| file | contents |
| --- | --- |
| `include/zenoh-pico/system/link/can.h` | the platform contract (§6) |
| `src/link/config/can.c` | endpoint parsing, config keys |
| `src/link/unicast/can.c` | capability declaration, lifecycle |
| `src/system/unix/network.c` | Linux SocketCAN implementation |
| `src/system/zephyr/network.c` | Zephyr `<zephyr/drivers/can.h>` implementation |
| `CMakeLists.txt` | `Z_FEATURE_LINK_CAN` toggle, default 0 |

No `src/protocol/codec/can.c` — see §2.

In nano-ros, one Kconfig entry:

```kconfig
config NROS_ZENOH_LINK_CAN
    bool "CAN link for zenoh-pico"
    help
      Enable CAN/CAN FD transport for zenoh-pico.
```

selecting `Z_FEATURE_LINK_CAN=1`. That is the whole of the nano-ros change.

## 6. The platform contract

Mirroring `include/zenoh-pico/system/link/serial.h`:

```c
z_result_t _z_open_can(_z_sys_net_socket_t *sock, const char *dev,
                       uint32_t bitrate, uint32_t dbitrate,
                       uint32_t tx_id, uint32_t rx_id);
z_result_t _z_listen_can(_z_sys_net_socket_t *sock, const char *dev,
                         uint32_t bitrate, uint32_t dbitrate,
                         uint32_t tx_id, uint32_t rx_id);
void       _z_close_can(_z_sys_net_socket_t *sock);
size_t     _z_read_can(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len);
size_t     _z_send_can(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len);
```

Each implementation is guarded `#if Z_FEATURE_LINK_CAN == 1`, as serial is.

**Write the `unix` binding first.** It is the same code that talks to a virtual
`vcan0`, so it unlocks the entire test loop (§7) before any target hardware or
Zephyr work exists.

## 7. Testability

The property that makes this tractable: **the whole link can be exercised on one
laptop with no hardware.**

Zephyr ships `CAN_NATIVE_LINUX`, a SocketCAN driver for `ARCH_POSIX`, and
`native_sim.dts` already carries the node — including the setup hint:

```c
compatible = "zephyr,native-linux-can";
/* adjust zcan0 to desired host interface or create an alternative
 * name, e.g.: sudo ip link property add dev vcan0 altname zcan0 */
host-interface = "zcan0";
```

Both it and `can_loopback.c` support CAN FD via `CONFIG_CAN_FD_MODE`.

Every frame is observable with `candump`. That is a better debugging surface
than target hardware ever provides.

**QEMU is not the path.** Zephyr has no CTU CAN FD or QEMU CAN driver —
`drivers/can/` contains nothing matching. QEMU emulates CAN controllers that
Zephyr cannot drive. `native_sim` + `vcan` is both simpler and more capable.

## 8. What this does not solve

**The host side is a second implementation.** A CAN link in zenoh-pico covers
MCU-class peers. A Linux host running zenoh-rs needs a matching link in Rust,
sharing this wire format. That work is real and is not scoped here — but §7
means it can be deferred while everything else is proven, because a zenoh-pico
peer over SocketCAN is a legitimate other end.

**Bandwidth is not generous.** CAN FD at a 2 Mbit/s data phase yields roughly
1–1.5 Mbit/s of usable payload after arbitration and framing. The safety island
needs an estimated 0.5–1 Mbit/s. That fits, without much margin, and the
estimate should be replaced by a measurement on the tier-1 harness before anyone
commits hardware.

## [RESOLVED 2026-08-23] UNICAST cannot complete a two-peer handshake

Found by implementing it. The link works: `tests/z_can_link_test` passes every
boundary on `vcan0` (CAN FD negotiated, MTU 63, both directions, DLC steps,
over-MTU refusal). A *session* does not.

`_zp_unicast_accept_task` (`src/transport/unicast/accept.c:29`) opens with

```c
const _z_sys_net_socket_t *socket_ptr = _z_link_get_socket(ztu->_common._link);
if (socket_ptr == NULL) { return NULL; }
...
z_result_t ret = _z_socket_accept(&listen_socket, &con_socket);
```

Both halves are stream/TCP assumptions. A CAN link returns NULL from
`_z_link_get_socket` (its read must filter on the receive identifier, which a
bare socket read cannot do), so the accept task exits immediately, and there is
no `accept()` for a datagram medium anyway.

Observed on `vcan0` with `candump`: the connecting peer emits exactly one frame
and the listener never answers.

```
(000.000000)  vcan0  TX B -  100  [32]  18 C1 09 F2 ...
```

The listening side declares its subscriber locally and then waits forever.

**IVC has the same shape**, so this is not specific to CAN — it is what
`Z_LINK_CAP_TRANSPORT_UNICAST` costs a link with no socket and no accept.

### Direction: multicast, which is what CAN actually is

`Z_LINK_CAP_TRANSPORT_MULTICAST` is the honest model. CAN is a broadcast bus —
every node hears every frame and filters by identifier — and the multicast
transport needs no accept at all (`transport/multicast/transport.c:118` sets
`_accept_task_running = NULL`).

It asks one thing of the link that unicast does not: `read` must fill the
`addr` slice so the transport can tell peers apart
(`transport/multicast/rx.c:110` compares `_remote_addr`). The sender's CAN
identifier *is* that address, which fits without inventing anything.

Concretely this changes:

* `_z_read_can` gains an out-parameter for the sender identifier, and the
  platform contract in `system/link/can.h` changes with it;
* the receive filter widens from one identifier to a range or mask, since a
  peer must hear every other peer rather than one;
* the endpoint grammar moves from `tx_id`/`rx_id` to an own-identifier plus a
  peer mask, which is also what section 4.2 wanted for multi-peer.

Section 4.2 already said the grammar must not foreclose multi-peer. This is that
bill arriving earlier than expected.

### Resolved

Implemented and verified on `vcan0`. Each peer owns one identifier, transmits on
it, accepts every frame the mask admits, drops its own, and reports the sender's
identifier as a 4-byte address. The endpoint is now

```
can/<device>#bitrate=500000;dbitrate=2000000;id=0x100;match=0;mask=0
```

The link lives in `src/link/multicast/` and registers in `_z_listen_link` only,
beside UDP multicast — multicast peers all listen, so there is no connect side.

Two zenoh-pico peers exchange pub/sub across the bus with a **189-byte payload
over a 63-byte MTU**, so the transport's own fragmentation is driving the link,
and `candump` shows traffic on both identifiers. The `_flow = DATAGRAM` choice
was right all along; only `_transport` was wrong.

## Sharing a bus between sessions

One zenoh session owns one link owns one CAN socket
(`_z_transport_multicast_t` holds a single `_z_link_t *`). Several sessions may
share one controller, and the two platforms reach that differently.

**Linux is correct by construction.** Each link opens its own
`socket(PF_CAN, SOCK_RAW, CAN_RAW)` with its own filter; the kernel demuxes and
`close()` is per socket. The one requirement is that each session use a distinct
`id`, since the read drops frames whose sender is its own identifier — two
sessions sharing an id would each discard the other's traffic.

**Zephyr needs the sharing built explicitly**, because a controller has one
driver, a handful of filter slots, and a single start/stop. Each link therefore
takes a receive queue from a pool (`Z_CAN_MAX_LINKS`, default 2), keeps the
filter id so `close` can release the slot, and the device is refcounted —
started by the first link to claim it and stopped only by the last to leave. A
link that finds a controller already running inherits its configuration rather
than reconfiguring it under the peers already using it.

`CONFIG_CAN_FD_MODE` gates more than performance on Zephyr: without it
`z_impl_can_set_bitrate_data` is not compiled — the declaration is
unconditional, so that is a link error rather than a compile error — and
`struct can_frame.data` is 8 bytes rather than 64. FD-specific paths must be
behind that Kconfig, not behind a runtime flag.

## Exploration log

**2026-08-23 — the CUSTOM hook is stream-shaped.** Discovered while reading
`src/link/unicast/custom.c`: the runtime-pluggable hook declares
`Z_LINK_CAP_FLOW_STREAM`. The apparent shortcut would have required building
ISO-TP inside the link. Reading `ivc.c` next showed the datagram template that
makes the whole segmentation problem vanish. Recorded because the wrong route
looked strictly easier right up until the capability flag was read.
