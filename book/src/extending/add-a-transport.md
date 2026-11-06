# Add a transport

<!-- type: how-to · level: 4 -->

**Needs first:** [How a build becomes an image](../background/how-a-build-becomes-an-image.md)

Your link is not one nano-ros carries itself: a USB CDC device, BLE GATT,
RS-485 with your own framing, a semihosting or RTT bridge, a ring buffer for a
test. You can plug it in from application code, without a new board or
platform. You give nano-ros four C functions (open, close, write and read) and
point the image's locator at the `custom` scheme. By the end of this page the
RMW session runs over your link.

This is nano-ros's counterpart of micro-ROS's
`rmw_uros_set_custom_transport`. When a link nano-ros builds in (Ethernet,
serial, WiFi) fits, use that instead: a built-in link is smaller.

## Before you start

- You have an image that builds and runs on its board, and the RMW is
  zenoh-pico or XRCE-DDS. Cyclone DDS binds the platform's socket stack and
  does not use this hook.
- You have a driver for your link with an open, a close, a blocking write and
  a read that takes a timeout.

## Steps

### 1. Write the four callbacks

Each callback takes the `user_data` pointer you register, so per-link state
travels through it:

| Callback | Returns |
| --- | --- |
| `open(user_data, params)` | `0` on success, a negative `rmw_ret_t` on failure |
| `close(user_data)` | nothing; teardown is best effort |
| `write(user_data, buf, len)` | `0` on success, a negative `rmw_ret_t` on failure |
| `read(user_data, buf, len, timeout_ms)` | the byte count, which may be less than `len` or `0`; a negative `rmw_ret_t` on error |

The callbacks never run from an interrupt: wrap an interrupt-driven device in
a queue that `read` drains. `write` should not block beyond a short hardware
retry. `user_data` must live until `close` returns.

### 2. Register them, for zenoh-pico

Register before the executor or support object opens the session. The
backend reads the registration once, at open; a later call is not refused,
but the session may never see it.

<!-- tabs -->

#### Rust

```rust
use nros_rmw::{NrosTransportOps, NROS_TRANSPORT_OPS_ABI_VERSION_V1, set_custom_transport};

unsafe {
    set_custom_transport(Some(NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: core::ptr::addr_of_mut!(MY_LINK).cast(),
        open: my_open,
        close: my_close,
        write: my_write,
        read: my_read,
    }))
    .expect("abi_version matches this nano-ros");
}
```

A version other than `NROS_TRANSPORT_OPS_ABI_VERSION_V1` returns
`TransportError::IncompatibleAbi`, and the previous registration stays.

#### C

```c
#include <rclc/rclc.h>

const nros_transport_ops_t ops = {
    .abi_version = NROS_TRANSPORT_OPS_ABI_VERSION_V1,
    ._reserved = 0,
    .user_data = &my_link,
    .open = my_open,
    .close = my_close,
    .write = my_write,
    .read = my_read,
};
if (rclc_set_custom_transport(&ops) != RCL_RET_OK) {
    /* a version other than NROS_TRANSPORT_OPS_ABI_VERSION_V1 */
}
```

`rclc_set_custom_transport(NULL)` clears the registration.

#### C++

```cpp
#include <rclcpp/transport.hpp>

rclcpp::TransportOps ops;
ops.user_data = &my_link;
ops.open  = [](void* ud, const void*) -> nros_cpp_transport_ret_t { return my_open(ud); };
ops.close = [](void* ud) { my_close(ud); };
ops.write = [](void* ud, const uint8_t* buf, size_t len) -> nros_cpp_transport_ret_t {
    return my_write(ud, buf, len);
};
ops.read  = [](void* ud, uint8_t* buf, size_t len, uint32_t to) -> int32_t {
    return my_read(ud, buf, len, to);
};
rclcpp::set_custom_transport(ops);
```

The fields are plain function pointers, so only lambdas with no captures
convert to them. `set_custom_transport` fills in the ABI version and refuses
a null field.

<!-- /tabs -->

zenoh-pico frames its own wire protocol, so the bytes your callbacks carry
need no framing. A Rust image also turns on the backend's `link-custom`
option, beside its other dependencies:

```toml
nros-rmw-zenoh = { version = "*", default-features = false, features = ["link-custom"] }
```

### 3. Or register them, for XRCE-DDS

XRCE-DDS takes its own registration, with the same four callbacks and a
framing choice: framing on for a byte stream (a UART, USB CDC), off for a
packet link (UDP, BLE GATT). Call
`nros_rmw_xrce_cffi::set_custom_transport_ops(&ops, framing)` from Rust, or
`nros_rmw_xrce_set_custom_transport_ops(&ops, framing)` from C
(`nros_rmw_xrce.h`), before the backend registers. XRCE-DDS never calls
`read` and `write` at the same time. A board usually makes this call in its
entry, as the bare-metal MPS2-AN385 board does for its UART.

### 4. Point the image at the custom link

The locator's scheme is the link. In the bringup's `system.toml`:

```toml
[image.my_image]
board = "my-board"
locator = "custom://uart"
```

On a byte-stream link like this one, the build leaves the IP transports out
of the image. What follows the scheme is a label for your own logs and must
not be empty.

### 5. Run the image

The session opens over your link: your `open` runs once, every publish
reaches `write`, and the executor's spin calls `read`.

## Example

A host image whose zenoh session runs over a TCP connection the application
opens itself, with no zenoh TCP link. The factory in `nros-transport-callbacks`
returns the four callbacks for a TCP bridge, and the session's locator is the
custom scheme:

```rust
use nros::prelude::*;

fn main() {
    let ops = nros_transport_callbacks::tcp_transport_ops("127.0.0.1:7447")
        .expect("connect to the router");
    unsafe { nros_rmw::set_custom_transport(Some(ops)).expect("abi_version v1") };

    let config = ExecutorConfig::new("custom/loopback").node_name("talker");
    let mut executor: Executor = Executor::open(&config).expect("open the session");
    let mut node = executor.create_node("talker").expect("create the node");
    let publisher = node
        .create_publisher::<std_msgs::msg::String>("/chatter")
        .expect("create the publisher");
    // publish and spin as in any talker
}
```

The full image is
[`examples/linux/rust/custom-transport-talker`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/custom-transport-talker),
which also registers its backend first and logs each message. A C image that
loops the bytes through a ring buffer, and exits non-zero unless all four
callbacks ran, is
[`examples/linux/c/custom-transport-loopback`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/c/custom-transport-loopback).
On a board,
[`examples/mps2-an385-baremetal/rust/talker-xrce`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal/rust/talker-xrce)
runs XRCE-DDS over the board's UART through `locator = "custom://uart"`.

## If it doesn't work

- **The session never opens, and `open` never ran.** The transport was
  registered after the session opened, or the zenoh image lacks `link-custom`
  and treats `custom` as an unknown scheme.
- **Registration fails.** `abi_version` is not
  `NROS_TRANSPORT_OPS_ABI_VERSION_V1`, or the struct was built without
  setting it.
- **An XRCE-DDS session ignores the transport.** It was registered through
  `set_custom_transport`, which only zenoh-pico reads. Use the XRCE call of
  step 3.
- **The session declares, then deadlocks.** On a threaded platform,
  zenoh-pico reads in its own task while your application writes. A transport
  that holds one lock across a blocking `read` starves `write`. Keep the two
  directions independent, as a full-duplex UART or a TCP stream is.

## See also

- [`nros_rmw::custom_transport`](../api/rust/nros_rmw/index.html), the
  callbacks' contract, in the Rust API reference.
- [Add a board](add-a-board.md), when the link needs a board of its own.
- [Write a custom RMW backend](custom-rmw.md), when the protocol itself is new.

## Design background

- [RFC-0006: portable RMW and platform interface](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0006-portable-rmw-platform-interface.md): the C ABI the transport hook is part of
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md): the link as the locator's scheme (D6)
