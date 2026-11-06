# Write a custom RMW backend

<!-- type: how-to · level: 4 -->

**Needs first:** [How a build becomes an image](../background/how-a-build-becomes-an-image.md)

nano-ros ships backends for zenoh-pico, XRCE-DDS, Cyclone DDS and PX4's uORB.
When your middleware is none of these (MQTT, a proprietary bus, a
vendor's DDS), you write a backend: the code that turns publish, take, request
and reply into your protocol. By the end of this page your backend registers
with nano-ros, and an image publishes and subscribes through it.

Most link problems are not new middleware. If the protocol is zenoh or
XRCE-DDS and only the wire is new, [add a transport](add-a-transport.md)
instead: it is four functions, not a backend.

## Before you start

- You know how nano-ros's RMW layer differs from ROS 2's: it leaves the I/O
  buffers to the caller, replaces wait sets with one call that drives I/O,
  and binds the backend when the image links. [How a build becomes an image](../background/how-a-build-becomes-an-image.md)
  explains the three choices; [Internals → RMW API design](../internals/rmw/rmw-api-design.html)
  argues them against ROS 2's `rmw.h`.
- You have chosen the backend's language. A Rust backend implements the
  traits of `nros-rmw`. A C or C++ backend fills the C table
  `nros_rmw_vtable_t` from `<nros/rmw_vtable.h>`. Either way the runtime
  calls the backend through that one C table.
- Plan for a change to nano-ros. An image selects its backend by name in
  `system.toml` (`rmw = "..."`), and today the names the build accepts are the
  backends in nano-ros's own tree. A backend outside it registers and runs
  from your own code, but `nros build` refuses its name until it is
  contributed. Step 5 says what that touches.

## Steps

1. Implement the session and its four entity kinds. In Rust, these are the
   traits and the methods each must provide; the rest have defaults:

   | Trait | Required methods |
   | --- | --- |
   | `Rmw` | `open(self, &RmwConfig)`, which returns the session |
   | `Session` | `create_publisher`, `create_subscription`, `create_service`, `create_client`, `close`, `drive_io` |
   | `Publisher` | `publish_raw`, `buffer_error`, `serialization_error` |
   | `Subscription` | `take_serialized`, `deserialization_error` |
   | `ServiceTrait` | `take_request`, `send_response` |
   | `ClientTrait` | `send_request_raw`, `take_response_raw` |

   A C backend fills the matching slots of `nros_rmw_vtable_t`
   (`create_session`, `drive_io`, `create_publisher`, `publish`,
   `create_subscription`, `take`, and the service and client slots). The
   header states each slot's signature, ownership and blocking rules; the
   [RMW API](../reference/rmw-api.md) reference links its rendered form. A
   slot the backend leaves `NULL` is reported as unsupported.

2. Keep to the I/O model. The buffers are the caller's: `publish_raw` gets a
   byte slice and `take_serialized` a slice to fill, both already CDR. Every
   take is non-blocking and answers "no data" when nothing is ready. All
   waiting happens in `drive_io(timeout_ms)`, which the executor calls from
   its own thread to move your protocol's I/O; it may block up to the
   timeout. A client sends with `send_request_raw` and polls with
   `take_response_raw`, matching a reply to its request by the sequence id.

3. Return specific errors. The runtime maps each `TransportError` variant to a
   named `rmw_ret_t` at the C boundary, so a caller can act on it: `Timeout`
   when a bounded wait elapses, `NoData` for an empty queue, `WouldBlock` when
   a resource is busy, `BufferTooSmall` when the caller's slice is short,
   `MessageTooLarge` when a message exceeds the backend's capacity,
   `Unsupported` for an operation your middleware cannot do, and
   `IncompatibleQos` when two endpoints cannot be reconciled. Keep `Backend`
   for what fits none of them.

4. Register the backend before the first session opens, under its name:

   - A Rust backend wraps its `Rmw` type in the adapter of `nros-rmw-cffi`:
     `RustBackendAdapter::<MyRmw>::register_named(c"myproto".as_ptr())`.
   - A C or C++ backend calls
     `nros_rmw_cffi_register_named("myproto", &MY_VTABLE)`.

   An image may register several backends; it opens the one it names.

5. Make it a backend nano-ros can select. This is the contribution. The
   backend is a package whose `package.xml` announces
   `<nano_ros_provides kind="rmw" name="myproto"/>`, with an `nros-rmw.toml`
   beside it. The descriptor is required, because two facts cannot be derived
   from the name: `[rmw] cpp_define`, the define C++ code tests for the
   backend, and how the backend reaches the link (`[rmw.provides.cargo]`
   with its `crate` and `register` function for a Rust backend,
   `[rmw.provides.cmake]` with its `dir` and `target` for a CMake one, and
   `[rmw.link]`). Start from the descriptor of the in-tree backend nearest
   yours.

6. Build an image with `rmw = "myproto"` and run a talker and a listener.
   Then check the pair against a stock ROS 2 node, as the next section
   describes.

## What ROS 2 tools expect

A backend that implements the traits passes messages between nano-ros nodes.
To appear in `ros2 topic list` and to talk to `rmw_zenoh_cpp` or a DDS
node, it must also carry what the traits do not state:

- **Discovery.** ROS 2 tools find endpoints through discovery traffic. A
  zenoh-flavoured backend publishes a liveliness token per endpoint, under
  `@ros2_lv/<domain>/<zid>/…`; a DDS-flavoured one sends the ROS 2 user data
  in its participant discovery. A new protocol needs some discovery channel,
  emitted as a side effect of each `create_*`.
- **Per-message metadata.** Each message carries a sequence number, a source
  timestamp and a publisher id that subscribers read as `MessageInfo`. On
  zenoh it travels as an attachment; on DDS it is the sample info. A backend
  that drops it leaves every `MessageInfo` at its default.
- **Type hashes.** `TopicInfo::type_hash` arrives filled in for the selected
  ROS edition: the literal `TypeHashNotSupported` on Humble, an `RIHS01_…`
  hash from Iron on. Your backend puts it where your wire carries it; a peer
  that disagrees about the hash never matches.
- **QoS.** Honour reliability, durability, history and depth, or refuse the
  ones you cannot. A best-effort publisher matched with a reliable
  subscriber is a QoS mismatch in ROS 2, and the backend should report it
  rather than lose messages.

Actions need nothing more: the runtime composes each action from three
services and two topics.

## Example

A backend that loops each published message back to its subscribers, with no
network. Only publish and take are shown; the service and client types follow
the same pattern:

```rust
#![no_std]
use nros_rmw::*;

static mut ECHO_BUF: [u8; 1024] = [0; 1024];
static mut ECHO_LEN: usize = 0;

pub struct EchoPub;
impl Publisher for EchoPub {
    type Error = TransportError;
    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        unsafe {
            let len = data.len().min(ECHO_BUF.len());
            ECHO_BUF[..len].copy_from_slice(&data[..len]);
            ECHO_LEN = len;
        }
        Ok(())
    }
    fn buffer_error(&self) -> TransportError { TransportError::BufferTooSmall }
    fn serialization_error(&self) -> TransportError { TransportError::SerializationError }
}

pub struct EchoSub;
impl Subscription for EchoSub {
    type Error = TransportError;
    fn take_serialized(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        unsafe {
            if ECHO_LEN == 0 {
                return Ok(None);
            }
            let len = ECHO_LEN;
            buf[..len].copy_from_slice(&ECHO_BUF[..len]);
            ECHO_LEN = 0;
            Ok(Some(len))
        }
    }
    fn deserialization_error(&self) -> TransportError { TransportError::DeserializationError }
}
```

`EchoSession::create_publisher` returns `Ok(EchoPub)`, `create_subscription`
returns `Ok(EchoSub)`, `drive_io` returns `Ok(())`, and `EchoRmw::open`
returns the session.

Two in-tree backends are complete examples to read beside it.
[`nros-rmw-metadata`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/metadata)
is a small Rust backend that records what a node declares instead of sending
it, registered through `RustBackendAdapter`.
[`nros-rmw-uorb`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/uorb/nros-rmw-uorb)
is a C++ backend over PX4's uORB, with its `package.xml` and `nros-rmw.toml`.

## If it doesn't work

- **`unknown rmw `myproto``**, from `nros build`. The build knows only the
  backends in nano-ros's tree; see step 5.
- **The executor opens no session.** The backend registered after the first
  session opened, or under a different name from the one the image selects.
- **Services or actions time out while topics work.** `take_request` or
  `take_response_raw` blocks instead of answering "no data", so the
  executor never reaches `drive_io`.
- **A second session in one process fails.** Session state lives in a
  `static` instead of in the `Session` that `open` returns.
- **`ros2 topic list` does not show your endpoints.** The backend emits no
  discovery traffic; see "What ROS 2 tools expect".

## See also

- [RMW API](../reference/rmw-api.md): the C table and the in-tree backends.
- [Internals → RMW API vs upstream](../internals/rmw/rmw-vs-upstream.html):
  every difference from `rmw.h`, for a porter coming from a ROS 2 backend.
- [Internals → RMW Zenoh protocol](../internals/rmw/zenoh-protocol.html): the
  key expressions and attachments a zenoh-compatible backend must produce.

## Design background

- [RFC-0054: C headers as the ABI single source of truth](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0054-c-header-abi-ssot.md): why every backend is reached through the C table
- [RFC-0071: RMW backend descriptor](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0071-rmw-backend-descriptor.md): `nros-rmw.toml` and what the toolchain lowers from it
