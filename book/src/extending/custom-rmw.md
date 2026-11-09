# Write a custom RMW backend

<!-- type: how-to · level: 4 -->

**Needs first:** [How a build becomes an image](../background/how-a-build-becomes-an-image.md)

nano-ros's own backends are listed in the
[RMW API reference](../reference/platform-api.md#backends-in-the-tree).
When your middleware is none of them (MQTT, a proprietary bus, a vendor's
DDS), you write a backend: the code that turns publish, take, request and
reply into your protocol. The backend is a package in your own workspace,
found by its `package.xml` announcement the way nano-ros's own backends are,
so nothing in nano-ros changes. By the end of this page an image selects
your backend by name and publishes through it.

If the protocol is zenoh or XRCE-DDS and only the wire is new,
[add a transport](add-a-transport.md) instead: four functions, not a backend.

## Before you start

- You know how nano-ros's RMW layer differs from ROS 2's: the caller owns
  the I/O buffers, one call drives I/O instead of wait sets, and the backend
  binds when the image links. [Internals → RMW API design](../internals/rmw/rmw-api-design.html)
  argues these against ROS 2's `rmw.h`.
- You have chosen the backend's language. A Rust backend implements the
  traits of `nros-rmw`. A C or C++ backend fills the C table
  `nros_rmw_vtable_t` from `<nros/rmw_vtable.h>`. Either way the runtime
  calls the backend through that one C table.
- For the worked example below: a Linux host with nano-ros
  [installed](../start/install.md), and ROS 2 Humble (on Jazzy, replace
  `humble` in each command).

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
   [RMW API](../reference/platform-api.md#the-rmw-api) reference links its rendered form. A
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

4. Register the backend under its name, in one function the image calls
   before the first session opens:

   - A Rust backend wraps its `Rmw` type in the adapter of `nros-rmw-cffi`:
     `RustBackendAdapter::<MyRmw>::register_named(c"myproto".as_ptr())`.
   - A C or C++ backend calls
     `nros_rmw_cffi_register_named("myproto", &MY_VTABLE)`.

5. Announce it. Put the backend in a package of its own in your workspace,
   whose `package.xml` exports `<nano_ros_provides kind="rmw" name="myproto"/>`,
   with an `nros-rmw.toml` beside it. The descriptor states what the name
   cannot: `[rmw] cpp_define`, the define C++ code tests for the backend, and
   how the backend reaches the link. A Rust backend names its crate and its
   register function under `[rmw.provides.cargo]`; a CMake one names its
   `dir` and `target` under `[rmw.provides.cmake]`. Then `[rmw.link]`. Start
   from the descriptor of the backend nearest yours.

6. Select it. An image names the backend in its bringup's `system.toml`
   (`rmw = "myproto"`). The build looks the name up among the announcements
   in your workspace and in nano-ros, so a misspelt name fails with the names
   it found. A Rust image links the backend's crate and calls its register
   function itself; a C or C++ image's CMake configure resolves the name
   through the same lookup. Run a talker and a listener, then check the pair
   against a stock ROS 2 node, as [What ROS 2 tools expect](#what-ros-2-tools-expect)
   describes.

## Worked example: a backend in your workspace

The backend here, `acme`, is the smallest that runs: zenoh's table under
its own name, with two slots of its own. One reports its name; the other
wraps `publish` and logs a line on the first message, so you can see the
image chose it. A real backend fills every slot with its own protocol.

Install the zenoh router ROS 2 ships and provision the host:

```bash probe=610 track=installed
sudo apt-get install -y ros-humble-rmw-zenoh-cpp
nros setup linux --rmw zenoh
```

Scaffold a Rust talker workspace for the Linux host. Source ROS 2 first:
`nros new` takes the ROS 2 edition from it.

```bash probe=620 track=installed
source /opt/ros/humble/setup.bash
nros new acme_ws --board linux --template talker --lang rust --rmw zenoh
cd acme_ws
mkdir -p src/acme_rmw/src
```

The backend's package. The `nano_ros_provides` line is what the build finds:

```bash probe=630 track=installed
cat > src/acme_rmw/package.xml <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>acme_rmw</name>
  <version>0.1.0</version>
  <description>The acme RMW backend.</description>
  <maintainer email="dev@example.com">Developer</maintainer>
  <license>Apache-2.0</license>
  <export>
    <build_type>nros_cargo</build_type>
    <nano_ros_provides kind="rmw" name="acme"/>
  </export>
</package>
EOF
```

Its descriptor. The image links the crate `acme-rmw` and calls its
`register()`; the linux board's `platform-posix` feature and the ROS edition
pass through to it:

```bash probe=640 track=installed
cat > src/acme_rmw/nros-rmw.toml <<'EOF'
[rmw]
cpp_define = "NROS_RMW_ACME"

[rmw.provides.cargo]
crate = "acme-rmw"
register = "register"

[rmw.provides.cargo.platform_features]
platform-posix = "platform-posix"

[rmw.link]
rlib_dep = "acme-rmw"
edition_feature = "ros-{edition}"
needs_cxx_linker = false
EOF
```

Its crate. It names nano-ros's crates by version, as any third-party crate
would; the build points each one at the nano-ros it builds against:

```bash probe=650 track=installed
cat > src/acme_rmw/Cargo.toml <<'EOF'
[package]
name = "acme-rmw"
version = "0.1.0"
edition = "2024"

[lib]
name = "acme_rmw"

[dependencies]
nros-rmw-cffi = { version = "*", default-features = false, features = ["alloc"] }
nros-rmw-zenoh = { version = "*", default-features = false }
nros-log = { version = "*", default-features = false }

[features]
platform-posix = ["nros-rmw-zenoh/platform-posix"]
ros-humble = ["nros-rmw-zenoh/ros-humble"]
ros-iron = ["nros-rmw-zenoh/ros-iron"]
ros-jazzy = ["nros-rmw-zenoh/ros-jazzy"]
EOF
cat > src/acme_rmw/src/lib.rs <<'EOF'
#![no_std]

use core::{
    ffi::c_char,
    sync::atomic::{AtomicBool, Ordering},
};

use nros_rmw_cffi::{
    NROS_RMW_RET_OK, NrosRmwRet, NrosRmwVtable, RustBackendAdapter, nros_rmw_cffi_register_named,
    rmw_byte_span_t, rmw_publisher_t, rmw_ret_t,
};
use nros_rmw_zenoh::ZenohRmw;

const ZENOH: NrosRmwVtable = RustBackendAdapter::<ZenohRmw>::VTABLE;

static ACME_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_implementation_identifier: Some(acme_identifier),
    publish: Some(acme_publish),
    ..ZENOH
};

static ANNOUNCED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn acme_identifier() -> *const c_char {
    c"acme".as_ptr()
}

unsafe extern "C" fn acme_publish(
    publisher: *const rmw_publisher_t,
    payload: rmw_byte_span_t,
) -> rmw_ret_t {
    if !ANNOUNCED.swap(true, Ordering::AcqRel) {
        nros_log::log_info!(nros_log::get_logger("acme_rmw"), "publishing through backend acme");
    }
    match ZENOH.publish {
        Some(inner) => unsafe { inner(publisher, payload) },
        None => nros_rmw_cffi::NROS_RMW_RET_ERROR,
    }
}

pub struct RegisterError(pub NrosRmwRet);

pub fn register() -> Result<(), RegisterError> {
    let rc = unsafe { nros_rmw_cffi_register_named(c"acme".as_ptr(), &ACME_VTABLE) };
    if rc == NROS_RMW_RET_OK { Ok(()) } else { Err(RegisterError(rc)) }
}
EOF
```

Select it in the bringup:

```bash probe=660 track=installed
sed -i 's/^rmw = "zenoh"/rmw = "acme"/' src/acme_node_bringup/system.toml
grep -n '^rmw' src/acme_node_bringup/system.toml
```

```text
7:rmw = "acme"
```

The image still speaks zenoh on the wire, so it needs a router. Start the one
ROS 2 ships, and leave it running:

```bash probe=670 probe-run=60 probe-keep track=installed
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=670
Started Zenoh router with id
```

In a second terminal, in `acme_ws`, build and run the talker. `nros run`
starts a router only for nano-ros's own zenoh backend, so name the one
already running with `--router`; the talker dials it:

```bash probe=680 probe-run=1200 track=installed
source /opt/ros/humble/setup.bash
nros run acme_node --router tcp/127.0.0.1:7447
```

```text probe-expect=680
publishing through backend acme
Publishing: 'Hello World:
```

The first line is your backend's: every message goes through `acme_publish`.

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
  ones you cannot. Report a mismatch (a best-effort publisher with a
  reliable subscriber) rather than lose messages.

Actions need nothing more: the runtime composes each action from three
services and two topics.

## Example: the traits

A backend that loops each published message back to its subscribers. Only
publish and take are shown; services and clients follow the same pattern:

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

Complete examples:
[`nros-rmw-metadata`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/metadata),
a small Rust backend registered through `RustBackendAdapter`, and
[`nros-rmw-uorb`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/uorb/nros-rmw-uorb),
a C++ backend with its `package.xml` and `nros-rmw.toml`.

## If it doesn't work

- **`no package provides rmw `myproto``**, from `nros build`. No
  `package.xml` announces that name. Check the spelling on both sides against
  the names the message lists, and that the package sits under `src/`.
- **The build refuses a backend with no `[rmw.provides.cargo]`.** A Rust
  image links the backend through its crate, so a backend with only a CMake
  table cannot serve one.
- **The executor opens no session.** The backend registered under a
  different name from the one the image selects.
- **Services or actions time out while topics work.** `take_request` or
  `take_response_raw` blocks instead of answering "no data", so the
  executor never reaches `drive_io`.
- **A second session in one process fails.** Session state lives in a
  `static` instead of in the `Session` that `open` returns.
- **`ros2 topic list` does not show your endpoints.** The backend emits no
  discovery traffic; see "What ROS 2 tools expect".

## See also

- [RMW API](../reference/platform-api.md#the-rmw-api): the C table and the backends in the tree.
- [Internals → RMW API vs upstream](../internals/rmw/rmw-vs-upstream.html):
  every difference from `rmw.h`, for a porter coming from a ROS 2 backend.
- [Internals → RMW Zenoh protocol](../internals/rmw/zenoh-protocol.html): the
  key expressions and attachments a zenoh-compatible backend must produce.

## Design background

- [RFC-0054: C headers as the ABI single source of truth](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0054-c-header-abi-ssot.md): why every backend is reached through the C table
- [RFC-0071: RMW backend descriptor](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0071-rmw-backend-descriptor.md): `nros-rmw.toml`, and how the build finds and links a backend in your workspace
