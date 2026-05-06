# Custom Transport (Phase 115)

`nano-ros` lets you plug a custom transport (USB-CDC, BLE GATT, RS-485
with framing, ring-buffer loopback, semihosting bridge, …) at runtime,
without changing the board crate, Cargo features, or rebuilding. This
is the runtime equivalent of micro-ROS's
`rmw_uros_set_custom_transport(framing, params, open, close, write, read)`.

## When to use

- You have a serial-over-USB device that doesn't fit `serial`,
  `ethernet`, or `wifi`.
- You're bridging through a host-side proxy (semihosting, RTT, OpenOCD).
- You're prototyping a transport on a target where the static path
  isn't built yet.
- You want a single firmware image that picks transport at boot from
  a config block.

If your transport fits one of the prebuilt static variants
(`platform-posix`, `platform-freertos`, `platform-nuttx`,
`platform-threadx`, `platform-zephyr`, `platform-bare-metal`), prefer
that — the runtime hook trades binary-size optimisation for
flexibility.

## API

### Rust

```rust
use core::ffi::c_void;
use nros_rmw::{NrosTransportOps, set_custom_transport};

unsafe extern "C" fn my_open(_ud: *mut c_void, _params: *const c_void) -> i32 { 0 }
unsafe extern "C" fn my_close(_ud: *mut c_void) {}
unsafe extern "C" fn my_write(_ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    // hand `buf[..len]` to the underlying medium, return 0 on success
    0
}
unsafe extern "C" fn my_read(_ud: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32 {
    // read up to `len` bytes within `timeout_ms`, return non-negative count
    0
}

unsafe {
    set_custom_transport(Some(NrosTransportOps {
        user_data: my_uart_handle as *mut c_void,
        open: my_open,
        close: my_close,
        write: my_write,
        read: my_read,
    }));
}
```

### C

```c
#include <nros/transport.h>

static nros_ret_t my_open(void *ud, const void *params) {
    (void)params;
    return my_uart_open((my_uart_t *)ud);
}
static void my_close(void *ud) { my_uart_close((my_uart_t *)ud); }
static nros_ret_t my_write(void *ud, const uint8_t *buf, size_t len) {
    return my_uart_write((my_uart_t *)ud, buf, len);
}
static int32_t my_read(void *ud, uint8_t *buf, size_t len, uint32_t timeout_ms) {
    return my_uart_read((my_uart_t *)ud, buf, len, timeout_ms);
}

int main(void) {
    nros_transport_ops_t ops = {
        .user_data = &g_uart,
        .open = my_open,
        .close = my_close,
        .write = my_write,
        .read = my_read,
    };
    nros_set_custom_transport(&ops);

    // ... continue with nros_support_init, nros_node_init, etc.
}
```

### C++

```cpp
#include <nros/transport.hpp>

nros::TransportOps ops;
ops.user_data = &g_uart;
ops.open  = [](void *ud, const void*) -> int { return my_uart_open((MyUart*)ud); };
ops.close = [](void *ud)               { my_uart_close((MyUart*)ud); };
ops.write = [](void *ud, const uint8_t *buf, std::size_t len) -> int {
    return my_uart_write((MyUart*)ud, buf, len);
};
ops.read  = [](void *ud, uint8_t *buf, std::size_t len, std::uint32_t to) -> std::int32_t {
    return my_uart_read((MyUart*)ud, buf, len, to);
};
auto r = nros::set_custom_transport(ops);
NROS_TRY(r);
```

> **Captureless lambdas only.** All four fields are raw C function
> pointers, not `std::function`. Pass per-instance state through
> `user_data`.

## Threading contract

| Constraint | Rationale |
|-----------|-----------|
| `read` and `write` are NEVER invoked concurrently from different threads. | The active backend serialises them through the `drive_io` / spin-once path. Custom transports written against this contract can use a single-buffer state machine without internal locking. |
| Callbacks must NOT be invoked from interrupt context. | The runtime path may take internal locks; ISR context could deadlock. Wrap ISR-driven hardware in a queue + `read` poller. |
| `user_data` must outlive the transport's active period. | The runtime never copies it. Lifetime is from the first callback invocation through `close` returning. |
| `set_custom_transport` must be called BEFORE `nros_support_init`. | The active backend reads the slot during `Rmw::open`. Calling after init is implementation-defined — backends may reject with `NROS_RET_ALREADY_INIT`. |

## Return-code conventions

- `open` / `write` return `0` (`NROS_RMW_RET_OK`) on success, a
  negative `nros_ret_t` (e.g. `NROS_RMW_RET_TIMEOUT`,
  `NROS_RMW_RET_ERROR`) on failure.
- `read` returns the non-negative byte count on success (may be less
  than `len`); a negative `nros_ret_t` on error / timeout.
- `close` returns nothing — failures during teardown are best-effort.

## Framing

Some transports need wire-level framing (HDLC for serial, length-prefix
for stream sockets). The **active backend** decides whether framing is
applied; the user vtable just sees raw bytes.

- **XRCE-DDS**: pass `framing=true` to the backend's
  `init_transport_from_custom_ops(framing)` for byte-stream transports
  (UART, USB-CDC). Pass `framing=false` for packet-oriented transports
  (UDP, BLE GATT).
- **zenoh-pico**: framing is built into the wire protocol — always
  `framing=false` regardless of the underlying medium.

## Backend coverage

| Backend | Status |
|---------|--------|
| **XRCE-DDS** | ✅ Wired (Phase 115.E). `nros_rmw_xrce::init_transport_from_custom_ops(framing)` pulls the registered vtable into `uxr_set_custom_transport_callbacks` via four C trampolines. |
| **zenoh-pico** | 🟡 Deferred (Phase 115.X-zenoh). zenoh-pico's custom-link API needs a per-platform `_z_link_t` shim; tracked separately. Zenoh users with custom transports today fork a `zpico-platform-*` crate. |
| **dust-DDS** | 🟡 Deferred (Phase 115.X-dds). dust-dds requires a custom transport plug-in implementing `RtpsUdpTransportParticipantFactory`-equivalent; design doc tracked separately. |

## Loopback test

`packages/xrce/nros-rmw-xrce/tests/custom_transport.rs` exercises the
slot lifecycle + the XRCE bridge round-trip with stub callbacks (no
real session). Run via:

```bash
cargo test -p nros-rmw-xrce --features platform-posix \
    --test custom_transport
```

## See also

- [`<nros/transport.h>`](../../../packages/core/nros-c/include/nros/nros_generated.h) — C header
- [`<nros/transport.hpp>`](../../../packages/core/nros-cpp/include/nros/transport.hpp) — C++ header
- [`nros_rmw::custom_transport`](../../../packages/core/nros-rmw/src/custom_transport.rs) — Rust source
- [`docs/roadmap/phase-115-runtime-transport-vtable.md`](../../../docs/roadmap/phase-115-runtime-transport-vtable.md) — phase doc
- [Custom platform](custom-platform.md) — when you need more than just transport
