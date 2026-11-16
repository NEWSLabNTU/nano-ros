---
rfc: 0132
title: "A board-owned CAN frame driver: one C ABI, the backend chosen by the board, the device named by a fact"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-560]
supersedes: []
superseded-by: null
---

# RFC-0132 — A board-owned CAN frame driver

**Status:** Draft (2026-11-16). Review-gated: nothing here is built until the
maintainer accepts the shape. Opened for issue
[2633](../issues/2633-board-owned-can-frame-driver.md) (GitHub #4012); the
portable layer under issue
[2573](../issues/2573-can-fd-e2e-profile-and-flexcan-helper.md)'s FlexCAN
helper (GitHub #3761). Work items: phase-560 W7.

## Summary

An application that sends its own CAN or CAN FD frames (an actuator command, an
island-alive heartbeat) has no nano-ros interface to send them through. Each
consumer writes one backend per platform and picks one with `#if`. This RFC
adds a small CAN frame interface to the platform layer: `open`, `send`,
`send_fd`, `fd_active`, an optional `recv`, and `close`, as one C ABI
(`<nros/platform_can.h>`) with a Rust trait beside it. The board selects the
backend, and a board or image fact names the device. The application keeps
its encoder, its frame layout and any end-to-end protection; nano-ros moves
bytes in frames and nothing else.

## Motivation

ASI, the reference consumer, sends control commands on CAN and CAN FD. Its
backend is chosen in `actuation_module/src/asi_common/include/platform/platform_can.h`:

| Backend | Selected by | Device named by |
| --- | --- | --- |
| `zephyr_can.h` | `PLATFORM_ZEPHYR` | `DT_CHOSEN(zephyr_canbus)` |
| `zephyr_can_udp_tunnel.h` | a Kconfig symbol (the FVP has no CAN) | Kconfig peer address and port |
| `freertos_can_socketcan.h` | `PLATFORM_FREERTOS_POSIX` | `getenv("SAFETY_ISLAND_CAN_IFACE")`, `SAFETY_ISLAND_CAN_FORMAT` |
| `freertos_can_mock.h` | `PLATFORM_FREERTOS_CAN_MOCK` | nothing (records frames) |
| `freertos_can_stub.h` | `PLATFORM_FREERTOS` | nothing (fails) |

That chain costs three things:

1. **The census cannot build a host image.** The chain needs a `PLATFORM_*`
   define, which only the board build supplies, so ASI keeps
   `[census] on_missing = "warn"` and its contract is not checked against the
   code.
2. **Run-time facts travel in the environment.** The SocketCAN interface and the
   frame format come from `getenv`, which RFC-0109 and the knob registry both
   refuse as a carrier: nothing records which value an image ran with.
3. **Every consumer re-solves it.** The L4 island needs the same thing on the
   S32K344 (issue 2573: CAN FD to CANHUB), and so will the next one.

The platform ABI has no CAN surface. `packages/drivers/` holds `ipc/`, `net/`
and `serial/`. The zenoh-pico CAN and ISO-TP links (RFC-0080, RFC-0083) carry
the RMW over CAN; they are not an interface an application can send its own
frames through.

## Design

### D1 — Layers

The split RFC-0109 makes for networks holds here:

- **Layer 1 (devices)** belongs to whoever owns the device: Zephyr devicetree
  (`can0`, `bus-speed`, `zephyr,canbus`), Linux `ip link set can0 type can
  bitrate ... dbitrate ... fd on`, and on a `"board"`-stack board the board
  package, from `[can0] bitrate` in its D4 file. Bring-up stays where RFC-0109
  D10 put it (`nros-platform-zephyr/src/can_start.c` starts the chosen
  controller).
- **The frame driver** (this RFC) opens a device that layer 1 brought up and
  sends frames on it. It sets no bitrate and starts no controller.
- **The application** owns frame ids, payload layout and E2E protection (issue
  2573's profile is a library over this driver, not a part of it).

### D2 — The ABI

A new optional header beside `platform_net.h`, carved out for the same reason:
a board with no CAN omits it.

```c
/* <nros/platform_can.h> */
#define NROS_CAN_FLAG_EXT  0x01u  /* 29-bit identifier */
#define NROS_CAN_FLAG_FD   0x02u  /* CAN FD frame */
#define NROS_CAN_FLAG_BRS  0x04u  /* FD bit-rate switch */

typedef struct {
    uint32_t id;        /* 11- or 29-bit, per NROS_CAN_FLAG_EXT */
    uint8_t  flags;
    uint8_t  len;       /* payload BYTES: 0..8 classic; 0..8,12,16,20,24,32,48,64 FD */
    uint8_t  data[64];
} nros_can_frame_t;

typedef struct nros_can nros_can_t;   /* storage sized by the backend */

/* `device` NULL = the board's default device (D4). */
int8_t nros_platform_can_open(nros_can_t *can, const char *device, uint8_t want_fd);
int8_t nros_platform_can_send(nros_can_t *can, const nros_can_frame_t *f, uint32_t timeout_us);
uint8_t nros_platform_can_fd_active(const nros_can_t *can);
int8_t nros_platform_can_recv(nros_can_t *can, nros_can_frame_t *f, uint32_t timeout_us); /* optional */
void   nros_platform_can_close(nros_can_t *can);
```

- **One send.** `send_fd` in the issue is `send` with `NROS_CAN_FLAG_FD`; a
  separate entry point would be a second copy of the validation. A C++ and a
  Rust wrapper keep both names for the reader.
- **`len` is bytes, not DLC.** The backend maps bytes to the wire DLC. A length
  that is not a valid FD size is refused, never padded: padding is payload
  layout, which is the application's.
- **Errors are values.** `0` sent, and negative codes for: invalid frame (id
  out of range, bad length, FD flags on a classic open), not open, FD not
  active, queue full or timeout, bus-off, and not supported (`recv` on a
  backend without one). `timeout_us = 0` never blocks.
- **What "sent" means.** `0` means the controller accepted the frame within
  `timeout_us`. Whether a backend also waits for transmit completion is open
  question 1. ASI's Zephyr backend waits for the TX callback; its SocketCAN
  backend returns when `write` queues the frame.
- **`fd_active`** is true only when the device was opened with `want_fd` and
  the device carries FD (Linux MTU `CANFD_MTU`; Zephyr `CAN_MODE_FD` in the
  controller's capabilities). Opening with `want_fd` on a classic-only device
  fails, so an image never finds out at its first FD send.
- **No allocation and no thread.** The handle is caller storage, like the
  socket storage in `platform_net.h`. `recv` polls or blocks the caller; a
  receive callback into the executor is open question 2.

The Rust side is a trait `PlatformCan` in `nros-platform-api` with the same five
methods. The C ABI is generated from it by cbindgen, the way the rest of
`nros-platform-cffi` is, so the two cannot drift.

### D3 — Backends

| Backend | Platform | Device is | Lives in |
| --- | --- | --- | --- |
| `zephyr` | Zephyr | a devicetree node label; default `zephyr,canbus` | `nros-platform-zephyr/src/can.c` |
| `socketcan` | Linux (posix platform, FreeRTOS-POSIX, Zephyr `native_sim` with host CAN) | an interface name (`can0`, `vcan0`) | `nros-platform-posix/src/can_socketcan.c`, Linux only |
| `board` | FreeRTOS / ThreadX / bare metal with a vendor driver | a controller index the board defines | the board package (strong symbols over weak fail-loud defaults, as `nros_board_register_netif` is today) |
| `mock` | any, for host tests | ignored | `packages/drivers/can/nros-can-mock` |

- **`zephyr`** is Zephyr's `can_send()` / `can_add_rx_filter_msgq()` over the
  device. On the S32K344 and S32Z270 that device is the FlexCAN or CANXL
  controller, so issue 2573's "raw FlexCAN helper on Zephyr" is this backend
  on those boards, not a separate driver.
- **`socketcan`** is `PF_CAN` / `SOCK_RAW` / `CAN_RAW`, with `CAN_RAW_FD_FRAMES`
  when FD is wanted and the MTU allows it. It is Linux-only and says so
  (CLAUDE.md naming: `linux`, not `posix`), so it compiles only where
  `linux/can.h` exists.
- **`board`** is the hook for a vendor stack nano-ros cannot ship (the NXP RTD
  FlexCAN driver on `s32z270-freertos`, issue 2632). The board package supplies
  the five symbols; the weak defaults return "not supported", so an image whose
  board supplies none fails at `open`, naming the board.
- **`mock`** records sent frames and replays queued receive frames, with a
  configurable FD capability, so an application's encoder can be tested on the
  host with no interface at all. It replaces each consumer's own mock.

ASI's UDP tunnel (CAN frames over UDP, for an FVP with no CAN controller) is a
consumer protocol, not a CAN backend: its wire format is ASI's. The consumer
keeps it, and supplies it through the `board` hook of its own board package or
as a custom backend (open question 3).

### D4 — Selection and the device fact

- **The board states its backend and default device** in its descriptor:

  ```toml
  [board.can]
  driver = "zephyr"        # zephyr | socketcan | board | mock
  default = "zephyr,canbus"  # zephyr: a chosen property or node label
  fd = true                # the default device carries CAN FD
  ```

  A board with no `[board.can]` has no CAN, and `open` on it is refused at build
  time when the image names a CAN device (D5).
- **An image may name another device or the mock**, never another backend
  family on hardware:

  ```toml
  [image.posix-vcan]
  can = { device = "vcan0", fd = true }

  [image.host-test]
  can = { driver = "mock" }
  ```

- **The fact reaches the code through the existing carriers**, never `getenv`:
  the resolved `[board.can]` plus the image row go into the image's generated
  configuration (on Zephyr a Kconfig string and a devicetree reference; on the
  CMake road the board projection, as `NROS_BOARD_*` are today). `open(NULL)`
  reads that default. ASI's `SAFETY_ISLAND_CAN_IFACE` / `_FORMAT` become one
  image row.

### D5 — What the build checks

- An image that names a CAN device on a board with no `[board.can]` is
  refused by `nros build`, naming the board.
- `fd = true` in an image on a board whose `[board.can] fd` is false is refused.
- The census builds a host image with the `mock` backend, so a consumer whose
  only `PLATFORM_*` dependency was its CAN chain can set `[census] on_missing`
  back to `error`.

## Alternatives considered

- **Keep CAN in the application (status quo).** Every consumer writes the same
  five backends, and the census cannot build them. Rejected for the reasons in
  the Motivation.
- **Ride zenoh-pico's CAN link.** RFC-0080 keeps that link self-contained in the
  vendored tree, and it carries zenoh's own framing; an application frame with
  a fixed id and layout (a CANHUB command) cannot go through it. Sharing code
  between the link and this driver is open question 4.
- **A header-only C++ helper** (what ASI has, moved into nano-ros). It keeps the
  `#if` chain and still needs a platform define, so the census problem stays.
- **Zephyr only, for issue 2573.** It solves the K344 and leaves the SocketCAN,
  mock and FreeRTOS halves to each consumer again.

## Open questions

1. **Send completion.** Should `send` wait for transmit completion (Zephyr TX
   callback, SocketCAN `CAN_RAW_RECV_OWN_MSGS` loopback) where the backend can
   report it, or only for acceptance? Proposed: acceptance by default, and a
   `NROS_CAN_SEND_WAIT_TX` flag where the backend supports it.
2. **Receive into the executor.** Is a polled `recv` enough for the first
   consumers (both only send), or does the driver need a readiness callback into
   the executor (RFC-0084) from the start?
3. **Consumer backends.** May an image supply its own backend (ASI's UDP
   tunnel) by name, or only through its own board package's `board` hook?
4. **Sharing with the zenoh-pico CAN link.** Should the link's Zephyr and Linux
   code be rebased onto this driver later, so one image has one CAN owner per
   device?
5. **Multiple devices.** The first consumers use one controller. Should
   `[board.can]` name several now (`[board.can.can1]`), or wait for a consumer?

## Changelog

- 2026-11-16 — created (Draft) for issue 2633 / GitHub #4012.
