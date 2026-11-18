---
rfc: 0134
title: "Router-less peering on a point-to-point link"
status: Draft
area: rmw-wire
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-575]
amends: [0109, 0110]
supersedes: []
superseded-by: null
---

# RFC-0134 — Router-less peering on a point-to-point link

**Status:** Draft (2026-11-18). Decided by the user on 2026-11-18: opt-in
unicast peer mode, per image, only for images that carry a point-to-point
link, implemented as a patch to our zenoh-pico fork, and measured before the
link is built. Tracked by phase-575 W4.

## Summary

Two nano-ros images on two real-time cores of one SoC, joined by an
inter-core channel (RPMsg, or a Tegra IVC channel), must exchange ROS 2 traffic
with no zenoh router between them. Neither core runs Linux, so no `rmw_zenohd`
exists on that path.

zenoh-pico can do this only in unicast peer mode, which nano-ros compiles out
today. Its peer mode as shipped in 1.10.1 is also built on sockets, so it
cannot use a point-to-point link even when compiled in. This RFC decides:

- **D1.** Unicast peer mode is an opt-in, per image, and only for an image
  whose link set includes a point-to-point link. The default build stays
  peer-free, and a gate holds it there.
- **D2.** Our zenoh-pico fork gains peering on a single-peer symmetric link:
  "listen" opens the link and completes one session handshake as the
  acceptor, and reads and leases go through the link rather than a socket
  wait.
- **D3.** Peer mode adds no thread. Every task it adds is a future on
  zenoh-pico's executor, driven in the mode RFC-0110 D1 chose for the
  platform. The tasks are named below.
- **D4.** One end is configured to listen and the other to connect. The
  choice is per image, written in the image's network configuration.

## Motivation

Facts read in zenoh-pico 1.10.1 (the pinned fork):

1. **nano-ros compiles peer mode out.** `nros-zpico-build` writes
   `#define Z_FEATURE_UNICAST_PEER 0` for every image. Multicast transport is
   off by a size decision (issue 0682), and RFC-0109's W3a amendment refuses
   `mode: "peer"` for that reason.
2. **Without unicast peer, a session either listens or connects, never
   both** (`src/net/session.c`, the two `Z_FEATURE_UNICAST_PEER == 0` checks).
   Of two router-less ends, one must accept the other's session, which needs
   the compiled-out mode.
3. **The accept path is TCP or TLS only.** On a listen locator the transport
   manager spawns `_zp_unicast_accept_task_fn`, which calls `_z_tcp_accept`,
   and that spawn exists only under `Z_FEATURE_LINK_TCP || Z_FEATURE_LINK_TLS`
   (`src/transport/manager.c`, `src/transport/unicast/accept.c`). Any other
   link fails the open with `_Z_ERR_TRANSPORT_OPEN_FAILED`.
4. **The peer read loop waits on sockets.** It iterates the peer list and
   waits through `_z_socket_wait_readable` on each peer's `_socket`
   (`src/transport/unicast/read.c`), and adding a peer takes
   `*_z_link_get_socket(zl)`. `_z_link_get_socket` has no case for IVC, and an
   RPMsg link modelled on IVC would have none either.
5. **The link layer is already symmetric.** The IVC link's listen is its open
   (`src/link/unicast/ivc.c`, mirroring serial). So the gap is in the
   transport's peer handling, not in the link.

## Design

### D1 — Opt-in, per image, gated

- The image's link set decides it. An image that declares a point-to-point
  link (RPMsg or IVC) in a router-less role builds with
  `Z_FEATURE_UNICAST_PEER 1`. Every other image builds with `0`, as today.
- The value is written in one place, the generated zenoh config in
  `nros-zpico-build`, and shared by the shim and the library (the issue 0135
  rule: every TU sees one config).
- A gate checks that an image without a point-to-point link compiles
  `Z_FEATURE_UNICAST_PEER 0`.
- RFC-0109's refusal of `mode: "peer"` stays for every other link, with its
  reason (multicast is off). It is lifted only for the point-to-point case.

**Amendment to D1 (2026-11-18, for the user's ruling; not yet approved).**
As built, the opt-in is decided by the **link set alone**, not by the image's
role. An image that compiles a point-to-point link compiles peer mode,
whether or not it opens a peer session:

- **As approved:** peer mode only for images carrying the RPMsg (or IVC) link
  in a router-less role; default builds peer-free.
- **As built:** `Z_FEATURE_UNICAST_PEER` = the build compiles a point-to-point
  link (IVC today, RPMsg when it lands). Serial is excluded: it is on by
  default for hosted and bare-metal builds, so it is not the image stating a
  point-to-point link.
- **Why:** the session's mode is runtime configuration (the bringup's
  `rmw/zenoh.json5`) that the zenoh-pico build cannot see, and compiling peer
  mode in costs 5,188 bytes of code, no static RAM and no heap (step 0
  Results). Default builds stay peer-free: no in-tree image compiles IVC.
- **The cost:** every IVC (later RPMsg) image carries the 5 KB, used or not.
- **The alternative, if the approved rule is preferred:** a per-image knob the
  CLI sets only when the image's zenoh config states `mode: "peer"` with a
  point-to-point endpoint, read by the same single function.

The session guard is the other half: `mode: "peer"` without the multicast
transport is refused unless every `listen`/`connect` endpoint is on a
point-to-point scheme (`ivc/`) and the build compiled peer mode for it.

### D2 — Peering on a single-peer symmetric link (fork patch)

A link capability, "single peer", is set by links that join exactly two
endpoints (IVC, serial, RPMsg). For such a link, the unicast transport:

- **On listen:** opens the link, then runs the acceptor half of the session
  handshake once, blocking until the remote end connects or the open
  deadline passes. No accept task is spawned, because there is never a second
  peer to accept.
- **On connect:** runs the initiator half, unchanged from client mode.
- **Reads:** with exactly one peer, the peer read loop reads from the link
  instead of waiting on a socket set. The link's own read (and its frame
  reassembly, as IVC already does) is the wait.
- **Leases:** each end's lease future tracks the one peer, using the existing
  peer-mode lease code. On expiry the peer is dropped, and the listening end
  re-enters the acceptor handshake so the remote can reconnect after a reset.

The patch touches the transport manager, the unicast read loop and the peer
add path. It does not touch the IVC or RPMsg link code beyond setting the
capability.

### D3 — What peer mode spawns, and how it is driven

Peer mode creates no OS threads in 1.10.1. Its work is futures on zenoh-pico's
executor, so RFC-0110's driving rule holds unchanged: in THREAD mode they run
on the executor thread, and in CALLER mode inside `zpico_spin_once`.

| Future | Spawned when | On a point-to-point link (D2) |
| --- | --- | --- |
| `_zp_unicast_accept_task_fn` | session open with a listen locator (TCP/TLS) | not spawned; the acceptor handshake runs once at open |
| `_z_add_peers` retry | session open, when a connect locator fails and the open does not exit on failure | spawned only if the connect end opens before the listen end; retries until the peer appears |
| the unicast read future | session open | unchanged; reads the single peer's link |
| the unicast lease future | session open | unchanged; uses the peer-mode lease and keep-alive arm |

Both spawns happen at session open, from the thread that opens the session,
so RFC-0110's "app-thread spawn latency" row still holds: nothing spawns from
an app thread in steady state. The measurement below confirms the task count
against `Z_RUNTIME_MAX_TASKS`.

### D4 — Which end listens

Each image's network configuration names its role, `listen` or `connect`, on
the point-to-point locator. Exactly one end listens. A configuration where
both ends connect, or both listen, is refused at build time where both images
are in one workspace, and otherwise fails at open with a message naming the
role.

## What must be measured before this leaves Draft

Measured on FreeRTOS MPS2-AN385 under QEMU, unicast peer over TCP (the only
transport peer mode supports before D2 lands), against the same image built
peer-free:

| Question | Method |
| --- | --- |
| Static RAM added | `just mem-report` on both builds, with `--baseline` |
| Heap added at open and per peer | the boot-time heap print, before and after open |
| Executor tasks live at open, steady state and reconnect | count against `Z_RUNTIME_MAX_TASKS` |
| Executor stack high-water mark | the boot-time peak print (issue 1146 method) |
| Threads added | expected 0; confirmed from the FreeRTOS task list |

After D2 lands, the same table is taken again over IVC or RPMsg on dual-core
QEMU.

### Results (2026-11-18, step 0)

Image: `examples/mps2-an385-freertos/rust/talker`, built with `nros build
mps2-an385-freertos` from main at 5370be0bb9, run under QEMU with the router on
the host. Three builds, each booted for 30 to 40 s:

- **A, peer-free:** today's image, `Z_FEATURE_UNICAST_PEER 0`, client mode.
- **B, compiled in:** `Z_FEATURE_UNICAST_PEER 1`, still client mode.
- **C, peer mode:** as B, plus `mode: 'peer'`, a listen locator
  (`tcp/0.0.0.0:7447`) and a connect locator to the host, in the bringup's
  `rmw/zenoh.json5`. Two measurement-only patches are needed for C:
  - the shim's peer-mode guard (issue 0682) is bypassed, because it refuses
    any peer session unless the multicast transport is compiled in;
  - the image's `locator` moves into the file, because both homes are
    refused together.

  C was measured twice: with its outbound link only, and with a second host
  router dialing its listen port, so it also held an inbound peer link.
  The router logged `whatami: peer` for that link, which stayed up for 24 s.

| | A | B | C, outbound only | C, plus one inbound peer |
| --- | --- | --- | --- | --- |
| `.text` | 505,252 | 510,440 (+5,188) | 510,592 (+5,340) | same as C |
| `.data` + `.bss` (static RAM) | 915,948 | 915,948 (+0) | 915,956 (+8) | same as C |
| heap peak, of 720,896 | 159,544 | 159,544 (+0) | 159,768 (+224) | 161,240 (+1,696) |
| zenoh executor stack peak, of 5,120 | 880 | 912 (+32) | 968 (+88) | 1,312 (+432) |
| app task stack peak | 26,688 | 26,688 | 26,696 (+8) | 26,696 (+8) |
| messages published | yes | yes | yes | yes |

What this answers:

- **Compiling peer mode in is free in RAM.** It costs 5 KiB of code, no static
  RAM and no heap. The 32-byte stack change is within run-to-run noise.
- **A peer session costs about 0.2 KiB of heap, and each further peer about
  1.5 KiB.** That is the per-peer transport state and its buffers. The
  executor stack peak rises 432 bytes with an inbound peer, still under a
  third of its 5 KiB.
- **No thread is added.** `_zp_unicast_accept_task_fn` is a future, started
  from `_zp_unicast_start_accept_task` (`src/transport/manager.c`) on
  zenoh-pico's own executor, so it runs on the existing executor task. D3
  holds.

Not yet measured: executor futures counted at runtime against
`Z_RUNTIME_MAX_TASKS` (64). From the code, peer mode adds the accept future
and, while a connect locator is unreachable, the `_z_add_peers` retry. That is
at most four futures with one peer, against a cap of 64. A runtime count, and
the FreeRTOS task list confirming no new task, need a debug print that this
step did not add.

## Upstream contribution

D2 is a general zenoh-pico capability (peering over any single-peer link, for
example serial), and it is a plausible contribution to eclipse-zenoh/zenoh-pico.
It is a separate line of work from our patch branch:

- Our fork carries the patch on its patch-line branch, on the pinned 1.10.1,
  and that is what ships.
- An upstream pull request targets upstream `main`, which has diverged from
  1.10.1, so its shape may differ. It is not a condition for anything here,
  and our branch does not wait for it.

## Alternatives considered

- **Multicast transport.** Peer mode without unicast peer needs multicast,
  which a point-to-point channel does not provide, and multicast is off by a
  size decision (issue 0682). Rejected.
- **A router on a third core.** In the motivating system (two Cortex-R5F
  cores of a TI J784S4) the application cores have no role, so no core is
  free to host one. Rejected.
- **A pollable descriptor for every link,** so the socket-based peer code
  accepts it. An RTOS RPMsg endpoint has no such descriptor. Rejected.
- **A non-zenoh RMW for this hop.** XRCE needs an agent, and Cyclone is not a
  candidate for a 512 B inter-core channel. Rejected.

## Phased plan

Tracked by phase-575 W4:

0. This RFC and the RFC-0109/0110 amendments, plus the measurement above over
   TCP.
1. The D2 fork patch, committed on the fork's patch-line branch. The
   maintainer pushes the fork, then the superproject pin moves.
2. The RPMsg link, on the IVC link's pattern, with the vendor binding left to
   the board package.
3. Acceptance: two nano-ros peers over RPMsg on dual-core QEMU, with no
   router. The same over IVC if that comes cheap.

## Changelog

- 2026-11-18: drafted.
- 2026-11-18: step 0 measured on FreeRTOS MPS2 QEMU (Results).
- 2026-11-18: step 1 landed with a marked amendment to D1 (opt-in by link
  set), and a host test of two peers over the IVC mock.
