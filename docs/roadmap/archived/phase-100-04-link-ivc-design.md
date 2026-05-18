# Phase 100.4 — `Z_FEATURE_LINK_IVC` design

Status: **draft / pre-implementation**. Companion to the work item in
`docs/roadmap/phase-100-orin-spe-infra.md` § 100.4 (lines 210–247). This
document is the contract the implementation in 100.4 must satisfy and
the framing spec the autoware_sentinel `ivc-bridge` daemon (out of
scope here) must mirror.

## 1. Scope

**In scope.** Make NVIDIA Tegra IVC a first-class zenoh-pico link
transport, peer to TCP / UDP / Serial / RawEth, gated behind a new
`Z_FEATURE_LINK_IVC` build flag (default off). The link is implemented
as a new C source under `src/link/unicast/ivc.c` inside the vendored
zenoh-pico submodule, plumbed through five `extern "C"` shim
forwarders in `zpico-platform-shim` that dispatch into
`<P as PlatformIvc>`. Build wiring in `zpico-sys/build.rs` enables the
link only when the SPE board crate (or the POSIX-mock dev path)
selects the new `link-ivc` cargo feature. URI scheme: `ivc/<channel
id>` (e.g. `ivc/2` for `aon_echo`).

**Out of scope.** The CCPLEX-side bridge daemon, the FSP `tegra_ivc_*`
linkage, the board crate (100.6), and any zenoh-routing decisions
above the link layer. The wire framing in §5 is the only contract the
bridge daemon shares with this design — its implementation belongs in
`autoware_sentinel/src/ivc-bridge/`.

## 2. Reference transports

zenoh-pico ships two single-peer link families that share IVC's
"talk to one specific other endpoint, no IP/ports" shape: **serial**
and **raweth**. We model 100.4 on serial, not raweth, because:

- **Serial** (`src/link/unicast/serial.c:70-166`) is a thin pass-through
  layer: `_z_f_link_write_serial` simply calls `_z_send_serial(sock,
  ptr, len)` and `_z_f_link_read_serial` calls `_z_read_serial(sock,
  ptr, len)`. The link layer doesn't own any framing — that lives in
  `src/protocol/codec/serial.c:28-118`, a COBS-framed
  `header(1) | len(2) | payload | crc32(4)` packetizer that runs
  *above* the link layer (called from
  `src/transport/unicast/{rx,tx}.c`). The link's mental model is "an
  ordered byte stream", advertised as `Z_LINK_CAP_FLOW_DATAGRAM` with
  `_is_reliable = false` (`serial.c:147-149`) and an MTU of
  `_Z_SERIAL_MTU_SIZE = 1500` (`include/zenoh-pico/system/link/serial.h:30`).
  IVC has the same single-peer shape and the same need for
  link-internal framing — the serial COBS codec is exactly the kind
  of layer we need, with different physical-layer assumptions (fixed
  64-byte datagrams instead of an open byte stream).

- **Raweth** (`src/link/unicast/raweth.c`, `src/transport/raweth/{tx,rx}.c`)
  is *not* a comparable model. Its "framing" is L2 Ethernet headers
  (`tx.c:100-146`), which fit one zenoh batch in one frame at the
  default 1500-byte MTU. There is no fragmentation/reassembly logic —
  `raweth/tx.c` writes one `_z_wbuf_t` per Ethernet frame and assumes
  the whole batch fits. With a 64-byte IVC frame and a 2048-byte zenoh
  batch we need real fragmentation, which raweth does not have.
  Furthermore, raweth lives in its own transport family
  (`Z_LINK_CAP_TRANSPORT_RAWETH`, `link.h:69`) rather than the
  unicast one — copying its shape would also require a new transport
  family and a parallel `src/transport/ivc/{rx,tx}.c` tree. Excessive.

**Decision.** IVC reuses the **serial transport family**: link-layer
methods remain pass-through into `_z_send_ivc` / `_z_read_ivc`, and
the per-frame fragmentation lives *inside* those two functions in
`zpico-platform-shim` (or, equivalently, inside `src/link/unicast/ivc.c`
just below the shim call boundary — see §5 for which side owns the
state). The link advertises `Z_LINK_CAP_FLOW_DATAGRAM`,
`_is_reliable = false`, `_cap._transport = Z_LINK_CAP_TRANSPORT_UNICAST`,
and an MTU equal to `Z_BATCH_UNICAST_SIZE` (2048 bytes by default —
see `include/zenoh-pico/config.h:24` and `zpico-sys/build.rs:166-169`).

## 3. Files to add / edit

### Vendored zenoh-pico submodule (`packages/zpico/zpico-sys/zenoh-pico/`)

> The submodule already tracks **our fork**:
> `https://github.com/jerry73204/zenoh-pico.git`, branch `nano-ros`,
> currently pinned at commit `13e53072`. Edits below land as a fork
> commit on top of that pin, then nano-ros bumps the submodule pointer
> in a follow-up commit. No patch-on-build, no upstream-acceptance
> caveat — adding `_Z_LINK_TYPE_IVC`, the `ivc/<channel>` URI scheme,
> and the framing protocol is a fork-internal decision.

| Path | Change |
| --- | --- |
| `include/zenoh-pico/link/endpoint.h` | Add `#define IVC_SCHEMA "ivc"` next to the existing `TCP_SCHEMA` / `SERIAL_SCHEMA` block at lines 30-45. |
| `include/zenoh-pico/link/link.h` | Add `_Z_LINK_TYPE_IVC` to the `enum _z_link_type_e` (currently lines 122-130); add `_z_ivc_socket_t _ivc;` to the `_z_link_t._socket` union (lines 135-157), gated `#if Z_FEATURE_LINK_IVC == 1`; include `"zenoh-pico/system/link/ivc.h"` in the matching gated block above (line 41 area). |
| `include/zenoh-pico/system/link/ivc.h` (new) | Defines `_z_ivc_socket_t` (one field: `void *_ch` — opaque handle returned by `_z_open_ivc`) plus the public API: `_z_open_ivc`, `_z_listen_ivc`, `_z_close_ivc`, `_z_read_ivc`, `_z_send_ivc`, `_z_ivc_notify`. Mirrors `system/link/serial.h:35-46`. |
| `include/zenoh-pico/link/config/ivc.h` (new) | `_z_endpoint_ivc_*` parsers and `IVC_CONFIG_*` keys; mirrors `link/config/serial.h:27-70`. Configurable knob: `frame_size` (advisory only — actual size comes from `nvidia_ivc_channel_frame_size`). Channel id is parsed from the locator address (`ivc/2` ⇒ `id = 2`), not from the config map. |
| `src/link/config/ivc.c` (new) | Companion to the header above: defines `_z_ivc_config_strlen`, `_z_ivc_config_to_str`, `_z_ivc_config_from_str(n)`. Mirrors `src/link/config/serial.c`. |
| `src/link/unicast/ivc.c` (new) | The link layer proper: `_z_endpoint_ivc_valid`, `_z_f_link_open_ivc`, `_z_f_link_listen_ivc`, `_z_f_link_close_ivc`, `_z_f_link_free_ivc`, `_z_f_link_write_ivc`, `_z_f_link_read_ivc`, `_z_f_link_read_exact_ivc`, `_z_f_link_read_socket_ivc`, `_z_get_link_mtu_ivc`, `_z_new_link_ivc`. Set `_type = _Z_LINK_TYPE_IVC`, `_cap._flow = Z_LINK_CAP_FLOW_DATAGRAM`, `_cap._is_reliable = false`, `_mtu = Z_BATCH_UNICAST_SIZE`. The write/read functions own the framing state machine in §5. Whole file gated `#if Z_FEATURE_LINK_IVC == 1`. |
| `include/zenoh-pico/link/manager.h` | Add `_z_endpoint_ivc_valid` and `_z_new_link_ivc` declarations next to the serial block at lines 44-45. |
| `src/link/link.c` | Add gated `else if (_z_endpoint_ivc_valid(&ep) == _Z_RES_OK) { ret = _z_new_link_ivc(zl, ep); }` clauses to both `_z_open_link` (after the `Z_FEATURE_LINK_SERIAL` block at lines 68-72) and `_z_listen_link` (after the bluetooth block at lines 129-133). Add `_Z_LINK_TYPE_IVC` case to the switch in `_z_link_get_socket` (lines 232-265) returning `&link->_socket._ivc._sock` (or null — IVC has no `_z_sys_net_socket_t`; see below). |
| `src/link/endpoint.c` | Add `IVC_SCHEMA` clauses in three sites: `_z_endpoint_config_from_string` (after line 404), `_z_endpoint_config_strlen` (after line 451), `_z_endpoint_config_to_string` (after line 498). Each gated `#if Z_FEATURE_LINK_IVC == 1`. |
| `CMakeLists.txt` | Add `set(Z_FEATURE_LINK_IVC 0 CACHE STRING "Toggle IVC links")` next to the existing serial line (260), append it to the status banner (~line 343), and propagate it as a `target_compile_definitions`. The existing `file(GLOB_RECURSE Sources "src/link/*.c" "src/transport/*.c" ...)` at lines 473-482 already picks up the new files automatically; no list edit needed. |

> Naming sub-decision: although there is no socket FD, naming the
> field `_z_link_t._socket._ivc._sock` (matching `_serial._sock`)
> keeps `_z_link_get_socket` uniform — it can return that pointer and
> peer.c's `_owns_socket = false` path treats it the same way as the
> serial-only build (see `zpico-platform-shim/src/shim.rs:594-626`,
> the existing fallback for serial-only zpico-sys builds). The
> "socket" is just the opaque IVC channel handle; nothing dereferences
> it as a real socket.

### nano-ros side (no submodule fork)

| Path | Change |
| --- | --- |
| `packages/zpico/zpico-sys/build.rs` | Extend `LinkFeatures` (lines 17-57) with an `ivc: bool` field read from `CARGO_FEATURE_LINK_IVC`. Emit `#define Z_FEATURE_LINK_IVC {ivc_flag}` in `generate_config_header` (line ~284 area). Emit the same `-D` define in all four `cc::Build` paths (`build_zpico_shim` ~line 1318, `build_zenoh_pico_embedded` ~line 1497, the FreeRTOS path ~line 1716, and the NuttX/ThreadX paths ~lines 1869/2099). When `ivc` is enabled and `bare-metal`/`freertos` is the platform backend, also emit `cargo:rustc-cfg=feature="link_ivc"` so the shim's `mod net_ivc` block is enabled. The `add_c_sources_recursive` call at line 1467 already picks up `src/link/unicast/ivc.c` — no change there. |
| `packages/zpico/zpico-sys/Cargo.toml` | Add `link-ivc = []` to `[features]`. |
| `packages/zpico/zpico-platform-shim/Cargo.toml` | Add `link-ivc = []` and (transitively, via the workspace umbrella) flow it through. The shim already gates its `network` block on a feature-flag; the new `link-ivc` feature follows the same pattern. |
| `packages/zpico/zpico-platform-shim/src/shim.rs` | Add a `mod ivc_helpers` block gated `#[cfg(feature = "link_ivc")]` (note: cargo-feature names are kebab-cased on the manifest side, snake_cased in `cfg`). The block contains the five forwarders defined in §4. The block lives below the existing `mod net_helpers_stub` (line 602+) and dispatches via `<P as PlatformIvc>::*`. The `use nros_platform::PlatformIvc;` import goes at the top of the file, gated on the same feature. |

> The nvidia-ivc driver crate (`packages/drivers/nvidia-ivc/`) and the
> `PlatformIvc` trait (`packages/core/nros-platform-api/src/lib.rs:522-541`)
> are unchanged. The orin-spe platform crate
> (`packages/platforms/nros-platform-orin-spe/src/ivc.rs`) is unchanged —
> its impl is already the dispatch target for the shim forwarders.

## 4. Shim symbol contract

Five `extern "C"` symbols, all `#[unsafe(no_mangle)]`, dispatching
through `<P as PlatformIvc>`. The signatures are direct translations
of the trait methods (`packages/core/nros-platform-api/src/lib.rs:522-541`)
into C ABI:

```rust
// All inside `mod ivc_helpers` in zpico-platform-shim/src/shim.rs,
// gated `#[cfg(feature = "link_ivc")]`.

#[unsafe(no_mangle)]
pub extern "C" fn _z_open_ivc(channel_id: u32) -> *mut c_void;
//   Resolves `channel_id` to the opaque per-channel handle. Returns
//   null on failure. Forwards to `<P as PlatformIvc>::channel_get`.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_read_ivc(ch: *mut c_void, buf: *mut u8, len: usize) -> usize;
//   Read at most `len` bytes from the channel. Returns bytes read,
//   `0` if no frame is currently available, or `usize::MAX` on hard
//   error. NOTE: one call returns *one* IVC frame (≤ 64 bytes); the
//   reassembly loop in `_z_f_link_read_ivc` is responsible for
//   stitching frames into a zenoh batch. Forwards to
//   `<P as PlatformIvc>::read`.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_send_ivc(ch: *mut c_void, buf: *const u8, len: usize) -> usize;
//   Write `len` bytes (must be ≤ frame_size — caller guarantees
//   this; the link layer fragments). Returns bytes written, or
//   `usize::MAX` on error. Forwards to `<P as PlatformIvc>::write`.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_close_ivc(ch: *mut c_void);
//   No-op on hardware (FSP channel handles outlive the session) and
//   on the unix-mock (the `Registry` owns the fd). Provided to
//   match the link-layer's `_z_f_link_close_*` signature shape.
//   Implementation: empty body (or a logging stub).

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_ivc_notify(ch: *mut c_void);
//   Ring the doorbell to wake the peer. Called by `_z_f_link_write_ivc`
//   after each IVC frame is enqueued (or, optionally, only after the
//   final fragment of a zenoh batch — see §5 "open question on
//   notify cadence"). Forwards to `<P as PlatformIvc>::notify`.
```

The C side declares matching prototypes in
`include/zenoh-pico/system/link/ivc.h`:

```c
void  *_z_open_ivc(uint32_t channel_id);
size_t _z_read_ivc(void *ch, uint8_t *buf, size_t len);
size_t _z_send_ivc(void *ch, const uint8_t *buf, size_t len);
void   _z_close_ivc(void *ch);
void   _z_ivc_notify(void *ch);
```

These five names are the entire C↔Rust ABI for the new transport.
Everything else (frame-size query, channel-id parsing, reassembly
state machine) lives strictly on the C side of the boundary inside
`src/link/unicast/ivc.c`.

## 5. Wire protocol

### 5.1 Constraints

- **IVC frame size:** fixed 64 bytes per frame, negotiated at
  carveout setup (NVIDIA default). Driver returns it via
  `nvidia_ivc_channel_frame_size`. Each `_z_send_ivc` /
  `_z_read_ivc` call moves *exactly one* frame; both
  `SOCK_DGRAM` (unix-mock) and `tegra-ivc` (hardware) preserve frame
  boundaries.
- **Zenoh batch size:** up to `Z_BATCH_UNICAST_SIZE = 2048` bytes
  (`include/zenoh-pico/config.h:24`; the default `unix_mock` /
  embedded path in `zpico-sys/build.rs:154-181` confirms 2048 as the
  realistic upper bound, and serial bumps that to 1500 to fit the
  zenohd negotiation floor; we keep 2048).
- **Single peer per channel:** no multiplexing; one IVC channel ⇔
  one zenoh peer ⇔ one reassembly state.
- **Ordering:** in-order delivery is guaranteed end-to-end by both
  the unix-mock backend (single SOCK_DGRAM SPSC) and the Tegra IVC
  ring (the hardware ring is SPSC per direction, FIFO). No
  reordering possible. Frame *loss* is also impossible on a
  cooperatively-driven SPSC ring — the producer back-pressures
  rather than drops. This drives several simplifications in §5.4.

### 5.2 Header layout — recommendation

The phase doc proposes "u16 total_len + u16 sequence". After
inspecting the constraints in §5.1, we recommend a small change:
**replace `sequence` with `offset`**. The on-wire frame layout is

```
 byte  0   1   2   3   4   5   ...                                63
     +---+---+---+---+---+---+---+---+---+---+---+---+---+---+...+---+
     | total_len (u16, LE) | offset (u16, LE) |  payload (≤ 60 B)    |
     +---+---+---+---+---+---+---+---+---+---+---+---+---+---+...+---+
```

| Field | Bits | Meaning |
| --- | --- | --- |
| `total_len` | 16 | Length of the whole zenoh batch this frame contributes to, in bytes. Same value in every fragment of the batch. Range `[0, 2048]` ⇒ `u16` is sufficient (covers up to 65 535). |
| `offset` | 16 | Byte offset of this fragment's payload within the batch, `[0, total_len)`. The final fragment is the one with `offset + payload_len == total_len`. |
| `payload` | up to (`frame_size` - 4) × 8 | Raw zenoh batch bytes. With the 64-byte default that's 60 bytes per frame ⇒ ⌈2048 / 60⌉ = 35 frames per batch worst case. |

**Why offset, not sequence.** `(offset, total_len)` makes the
reassembler stateless across "what's the next sequence I expect?" —
it knows from `total_len` exactly how many bytes are still pending,
and it places each fragment's payload directly at `buffer[offset..]`.
A `sequence` counter only carries the same information by way of
"next = current + 1" plus an off-band frame-size query, and it
breaks if the producer ever changes `frame_size` mid-batch. With
in-order SPSC delivery (§5.1) the receiver does not actually need a
sequence to detect loss — there can't be loss — so the `offset`
formulation is strictly more useful information at the same bit
cost.

If the upstream phase doc wording (`u16 total_len + u16 sequence`)
must be preserved verbatim, treat `sequence` as a synonym for
"fragment index" (`offset / (frame_size - 4)`); the reassembler can
recover `offset` from `sequence × (frame_size - 4)`. The bridge
daemon spec must be updated to match whichever form ships.

**`total_len = 0` reserved.** A frame with `total_len == 0` and
`offset == 0` is a keep-alive / handshake ping with no payload. The
link layer drops it silently; included so the bridge daemon can
probe the SPE without injecting a zero-byte zenoh batch.

### 5.3 Send path (`_z_f_link_write_ivc`)

```text
inputs:  ch (channel handle), buf (zenoh batch ptr), len (batch size)
constants: F = frame_size — 4   // payload per fragment, computed once at open
const buf MAX_FRAME = frame_size  // 64 by default; queried via PlatformIvc::frame_size

assert len <= Z_BATCH_UNICAST_SIZE          // i.e. fits in u16
let mut off = 0
while off < len:
    let chunk = min(F, len - off)
    let mut frame[MAX_FRAME] = {0}
    write_le_u16(&frame[0], len)          // total_len
    write_le_u16(&frame[2], off)          // offset
    memcpy(&frame[4], buf + off, chunk)
    let wrote = _z_send_ivc(ch, frame, 4 + chunk)
    if wrote == SIZE_MAX or wrote != 4 + chunk:
        return SIZE_MAX
    off += chunk
_z_ivc_notify(ch)                           // doorbell once per batch
return len
```

### 5.4 Receive path (`_z_f_link_read_ivc`)

The receive function is called with a destination buffer
(`_z_zbuf_t::wptr`, sized at least `Z_BATCH_UNICAST_SIZE`) and
returns the number of bytes of one *complete* zenoh batch on
success. Reassembly state lives in the `_z_ivc_socket_t` (one
reassembly buffer + a "bytes received so far" counter + a `total_len`
witness); it survives across `_z_f_link_read_ivc` calls because the
upper layer can call read with a fresh `_z_zbuf_t` for each batch.

```text
state in _z_ivc_socket_t:
    uint8_t  rx_buf[Z_BATCH_UNICAST_SIZE]
    uint16_t expected_total          // 0 == "no batch in progress"
    uint16_t bytes_received

inputs:  ch, dst_buf, dst_len
loop:
    let mut frame[frame_size] = {0}
    let n = _z_read_ivc(ch, frame, frame_size)
    if n == 0:                                // no frame yet
        return 0                               // matches PlatformIvc semantics; upper layer will retry
    if n == SIZE_MAX:                          // hard error
        reset_state(); return SIZE_MAX
    if n < 4:                                  // runt
        reset_state(); return SIZE_MAX
    let total = read_le_u16(&frame[0])
    let off   = read_le_u16(&frame[2])
    let payload_len = n - 4
    if total == 0 and off == 0:                // keep-alive — drop, keep looping
        continue
    if total > Z_BATCH_UNICAST_SIZE:           // oversized
        reset_state(); return SIZE_MAX
    if expected_total == 0:                    // first fragment of a new batch
        expected_total = total
        bytes_received = 0
    else if total != expected_total:           // mid-batch total_len changed
        reset_state(); return SIZE_MAX
    if off + payload_len > expected_total:     // overrun
        reset_state(); return SIZE_MAX
    memcpy(rx_buf + off, &frame[4], payload_len)
    bytes_received += payload_len
    if bytes_received == expected_total:       // last fragment
        if dst_len < expected_total:
            reset_state(); return SIZE_MAX     // caller buffer too small
        memcpy(dst_buf, rx_buf, expected_total)
        let n_done = expected_total
        reset_state()
        return n_done
    // else: still assembling; loop and read the next frame
```

`_z_f_link_read_exact_ivc` is the same loop with the loop's `return
0` replaced by a wait-and-retry (mirrors `_z_read_exact_serial` at
`src/system/common/serial.c:66-82`).

### 5.5 Error conditions

| Condition | Behaviour |
| --- | --- |
| Lost frame | Cannot occur on SPSC IVC ring or unix-mock SOCK_DGRAM; the producer back-pressures (`_z_send_ivc` returns 0 / `SIZE_MAX` rather than dropping). The reassembler does not implement a watchdog; if upper layers ever need one it can be added later as a tick-driven `expected_total = 0` reset. |
| Out-of-order frame | Cannot occur (SPSC FIFO). Detected as `off > bytes_received` only if the bridge daemon misbehaves; treated as "reset and abandon batch" (`return SIZE_MAX`). |
| Oversized batch (`total_len > Z_BATCH_UNICAST_SIZE`) | Reset state, return `SIZE_MAX`. Upper layer treats as transport error and tears the link down. |
| Mid-batch `total_len` change | Same. Indicates a bug or a bridge-daemon restart mid-batch. |
| `frame_size` runtime change | Not supported. The link captures `frame_size` once at open and never re-queries; if hardware ever resizes the ring the channel must be re-opened. |
| Sequence wrap | N/A — `offset` indexes within one batch and resets at every batch boundary, so it never reaches 65 535 (max batch is 2048). |
| Receive buffer too small | `dst_len < expected_total` ⇒ `SIZE_MAX`. The upper layer's `_z_zbuf_t` is always sized to at least `Z_BATCH_UNICAST_SIZE` (`src/transport/unicast/rx.c` allocates against that constant), so this is a safety check rather than an expected path. |

**Maximum supported zenoh batch:** `Z_BATCH_UNICAST_SIZE = 2048`
bytes by default. Configurable at zpico-sys build time via
`ZPICO_BATCH_UNICAST_SIZE` env (`zpico-sys/build.rs:173`); upper
bound is the protocol-level `u16` ⇒ 65 535. The on-wire `total_len`
field caps at the same `u16`, so the design tolerates any
`Z_BATCH_UNICAST_SIZE` ≤ 65 535 without header changes.

## 6. Build wiring

The flow that takes "board crate (100.6) wants IVC" all the way to
"`#define Z_FEATURE_LINK_IVC 1` in the C compile":

1. **Board crate (100.6).** `nros-board-orin-spe/Cargo.toml` enables
   `zpico-sys = { features = ["link-ivc", "freertos", ...] }`. POSIX
   dev / mock builds set the same feature on
   `zpico-sys = { features = ["link-ivc", "posix"] }`.

2. **`zpico-sys` Cargo feature.** `link-ivc = []` in `Cargo.toml`'s
   `[features]`. Cargo exports `CARGO_FEATURE_LINK_IVC=1` to
   `build.rs`.

3. **`zpico-sys/build.rs`.** `LinkFeatures::from_env` (lines 28-37)
   reads the feature into `LinkFeatures.ivc`. `generate_config_header`
   writes `#define Z_FEATURE_LINK_IVC {flag}` next to the existing
   `Z_FEATURE_LINK_SERIAL` block (~line 277). Each `cc::Build`
   pipeline (`build_zpico_shim`, `build_zenoh_pico_embedded`, the
   FreeRTOS / NuttX / ThreadX variants — five sites total) emits
   `build.define("Z_FEATURE_LINK_IVC", if link.ivc { "1" } else { "0" })`
   alongside the existing serial define. Globbing at
   `add_c_sources_recursive(&mut build, &src_dir.join("link"))` (line
   1467) picks up the new `src/link/unicast/ivc.c` and
   `src/link/config/ivc.c` automatically — no source-list edit
   needed.

4. **Cfg propagation to the shim.** `build.rs` emits
   `println!("cargo:rustc-cfg=feature=\"link_ivc\"")` when `link.ivc`
   is set, so downstream crates (specifically `zpico-platform-shim`)
   can `#[cfg(feature = "link_ivc")]`-gate their forwarders without
   needing the feature on their own manifest. (This mirrors the
   pattern at `build.rs:504-514` where the platform backend gets
   `cargo:rustc-cfg=zpico_backend="..."`.) Strictly, the shim already
   has its own `link-ivc` feature in step 5 — using both keeps the
   two crates independently testable.

5. **`zpico-platform-shim`.** `Cargo.toml` has `link-ivc = []`. The
   `mod ivc_helpers` block is gated `#[cfg(feature = "link_ivc")]`.
   When the umbrella crate (the board crate's direct dep) enables
   the feature, the five forwarders link in.

6. **CMake-driven submodule build.** When zpico-sys uses CMake
   (`build_zenoh_pico_cmake` path, ~line 1077-1120) instead of `cc`
   directly, the `cmake_cfg.define("Z_FEATURE_LINK_IVC", ...)` line
   propagates. The submodule's own `CMakeLists.txt` (line 257-286
   block) gets the new `set(Z_FEATURE_LINK_IVC 0 CACHE STRING ...)`
   so external CMake users (zephyr port etc.) can flip it the same
   way they flip serial.

**Default-off byte-identical guarantee.** With
`link-ivc` disabled, `LinkFeatures.ivc == false`, the generated
config header gets `#define Z_FEATURE_LINK_IVC 0`, every gated `#if`
in zenoh-pico is dead, the CMake / cc Build never references
`ivc.c`, and `mod ivc_helpers` is `cfg`-disabled. No new symbols, no
new `.o` files, no behavioural change. This is item 1 of the phase
doc's acceptance criteria.

## 7. Test strategy

Two layers of test coverage, both runnable on the host without
NVIDIA hardware:

**Layer 1 — link unit test, in-process.** A new `tests/ivc_link.rs`
in `packages/zpico/zpico-sys/` (or as a `#[cfg(test)]` integration
inside the shim crate, whichever the existing test layout prefers):

- Use `nvidia-ivc`'s `unix-mock` `register_pair(id_a, id_b)` to wire
  two channel IDs to the two ends of one `UnixDatagram::pair()`.
- Open both as zenoh-pico links: `_z_open_link(&link_a, "ivc/{id_a}")`
  and `_z_listen_link(&link_b, "ivc/{id_b}")`.
- Build a 2048-byte payload; call `link_a._write_all_f(...)`; loop
  on `link_b._read_f(...)` until 2048 bytes are reassembled; assert
  the round-tripped buffer matches.
- Exercise edge sizes: 1 byte, 60 bytes (one frame minus header),
  61 bytes (forces two frames), 2047 bytes, 2048 bytes.
- Negative tests: forge a frame with `total_len = 3000` directly via
  the unix-mock fd ⇒ assert `_z_f_link_read_ivc` returns
  `SIZE_MAX` and resets state.

**Layer 2 — end-to-end via the `unix-mock` backend.** A second test
that boots a full zenoh-pico session on each end of the mock pair
(one peer, one client) and runs a publish/subscribe of a payload
≥ 64 bytes. This is the test the phase doc's acceptance criterion
("multi-frame zenoh message in/out") refers to. The test asserts
that a 1024-byte sample published on side A is delivered intact on
side B. With `Z_BATCH_UNICAST_SIZE = 2048` and a 1024-byte sample,
zenoh's own fragmentation does not kick in and the IVC link layer
sees a single batch that needs ⌈1024 / 60⌉ = 18 IVC frames.

Both test layers run under `cargo test -p zpico-sys --features
link-ivc` (or the equivalent on the umbrella). Neither requires the
FSP, hardware, or the autoware_sentinel bridge daemon.

## 8. Resolved design decisions

The five questions left open in the first draft are resolved as
follows. Implementation must match these.

### 8.1 Notify cadence — per-batch

`_z_ivc_notify` fires **once per zenoh batch**, after the final IVC
frame is enqueued (matches the send pseudocode in §5.3). One
doorbell per up-to-35 frames. The peer's read loop is woken once
per batch and drains the ring in a tight loop — no per-frame wakeup
overhead.

Trade-off accepted: the first frame of a batch sits in the ring
until the producer finishes writing the last frame. With a 2048-byte
worst-case batch and SPE-side cycle counts in the µs range, that
"first-frame latency" is in the same order as the doorbell
delivery itself; per-frame notify would not buy meaningful
latency. The bridge daemon must implement read as a
"loop-while-frames-available", not a "one-frame-per-doorbell"
state machine.

### 8.2 Frame size — cache at open

`_z_ivc_socket_t` stores `frame_size` once during
`_z_f_link_open_ivc` (`frame_size = nvidia_ivc_channel_frame_size(ch)`)
and never re-queries. Saves a trait dispatch on the hot path and
matches §5.5's "frame_size runtime change is not supported"
contract — if the hardware ring is resized the channel must be
re-opened anyway.

### 8.3 `_z_open_ivc` ≡ `_z_listen_ivc` — IVC is symmetric like serial

The two shim symbols collapse to the same call:
`nvidia_ivc_channel_get(channel_id)`. There is no client / server
distinction at the IVC layer — both peers open the same channel
ID and the underlying ring is bidirectional SPSC.

This matches **serial's** semantics exactly. In zenoh-pico's POSIX
impl, `_z_listen_serial_from_dev` is literally:

```c
// src/system/unix/network.c:965-968
z_result_t _z_listen_serial_from_dev(_z_sys_net_socket_t *sock,
                                     char *dev, uint32_t baudrate) {
    // Serial is symmetric — listen is same as open
    return _z_open_serial_from_dev(sock, dev, baudrate);
}
```

UDP is the **asymmetric counter-example** worth contrasting
against: `_z_open_udp_unicast(sock, rep, tout)` in
`src/system/unix/network.c:416-440` creates a socket that actively
targets the **remote** endpoint `rep`, while
`_z_listen_udp_unicast(sock, lep, tout)` (`network.c:441-451`)
binds a socket on the **local** endpoint `lep` to accept incoming
datagrams. The signatures differ (`rep` vs `lep`), the bodies
differ (connect vs bind), and the POSIX impl of listen is in
fact a `@TODO: To be implemented` stub returning
`_Z_ERR_GENERIC` — UDP genuinely needs two distinct paths because
client and server are not interchangeable.

IVC has none of that asymmetry. There is no "address" beyond the
channel id, no `connect`/`bind` distinction, no notion of
"who initiated the conversation". Treating listen as a synonym
for open is the right shape and exactly what serial does.

The C side keeps two function pointers populated for uniformity
with the rest of zenoh-pico:

```c
// src/link/unicast/ivc.c (excerpt)
zl->_open_f   = _z_f_link_open_ivc;
zl->_listen_f = _z_f_link_listen_ivc;

z_result_t _z_f_link_listen_ivc(_z_link_t *self) {
    // IVC is symmetric — listen is same as open. Mirrors
    // _z_listen_serial_from_dev → _z_open_serial_from_dev
    // (system/unix/network.c:965-968).
    return _z_f_link_open_ivc(self);
}
```

If a future bring-up needs a distinct "wait until peer ready"
handshake, it belongs in the reserved `total_len = 0` keep-alive
ping path (§5.2), not in a third shim symbol.

### 8.4 Bridge-daemon spec sharing — text spec is the single source of truth

The autoware_sentinel `ivc-bridge` daemon implements the same
framing on the CCPLEX side. To keep the two specs in lock-step,
**this document's §5.2 is the spec** — the bridge daemon cites
it directly rather than duplicating the wording. No shared crate
yet: the framing logic is ~30 lines on each side and a shared
crate would create a build-time coupling between the nano-ros
repo and autoware_sentinel that neither wants right now.

If a third consumer appears (a Linux test harness, a different
SoC's bridge), revisit and factor a `tegra-ivc-framing` crate
that both repos can dep. Until then, the bridge daemon's PR
description must reference the §5.2 spec by file path + commit
hash.

## 9. Remaining open questions

1. **Channel-id namespace collision with serial.** Both schemes
   use small integers in their locator (`serial/2` vs `ivc/2`).
   The URI prefix disambiguates at parse time, but config-file
   copy-paste between a serial-using board and an SPE board can
   silently land on the wrong transport if the prefix is dropped.
   Doc-only warning; no code change.

2. **Does the unix-mock's SOCK_DGRAM truncate-on-short-read match
   tegra-ivc semantics?** SOCK_DGRAM drops bytes beyond the
   caller-supplied buffer. tegra-ivc's `tegra_ivc_read` requires
   the caller to pass a buffer at least `frame_size` bytes wide
   (else returns an error). The link layer always passes a
   `frame_size`-sized stack buffer, so the two match in practice.
   Plan to document the assumption in `ivc.c`'s file-level
   comment so a future edit doesn't accidentally pass a smaller
   buffer.
