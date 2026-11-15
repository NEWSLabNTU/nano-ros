# A bare-metal app that owns its main loop

<!-- type: how-to · level: 3 -->

**Needs first:** [Bare-metal Cortex-M3](bare-metal.md), [Use XRCE-DDS](../rmw/use-xrce-dds.md), [Use a serial link](../rmw/use-a-serial-link.md)

Use this page when your firmware already decides what runs when, and nano-ros
has to fit inside it. An RTIC application that sets its own interrupt
priorities is the common case, and so is a hand-written superloop. Such an
application cannot use `nros::main!()`: the macro owns `main`, and on RTIC it
starts its tasks at one fixed priority from the board's table. Instead you call
the library layer yourself: install a transport, register the backend, open an
executor over memory you own, and spin it from your own loop.

The page follows a serial-only board talking XRCE-DDS to a micro-ROS Agent over
a UART. The same steps hold for USB-CDC and for other byte streams.

## Before you start

You need:

- a board crate or application that already boots, with a panic handler;
- a `nros_platform_*` provider for the board, supplying the clock, sleep and
  the allocator ([Add a board](../../extending/add-a-board.md) lists them);
- a UART driver you can call from C-ABI callbacks;
- a micro-ROS Agent on the host, at the other end of the serial line.

Add `nros-node` with its `alloc` and `rmw-cffi` features, `nros-rmw-xrce-cffi`,
and the message crates you publish, such as `nros-std-msgs`.

## Build without an IP stack

A serial-only board has no IP stack, so build with the IP link turned off:

```console
NROS_LINK_IP=0 cargo build --release
```

With `NROS_LINK_IP=0` the XRCE backend builds no UDP transport, and the image
links no `nros_platform_udp_*` symbol. Your platform provider does not need to
stub them. If such an image is given a `udp://` locator, it refuses with a log
line saying it was built with `NROS_LINK_IP=0`.

## Bring nano-ros up from your code

The order matters. Install the transport before any session opens, and
register the backend before you open the executor:

```rust file=src/talker.rs
#![no_std]
use core::{ffi::c_void, mem::MaybeUninit, time::Duration};
use nros_node::{Executor, ExecutorConfig, ExecutorSizing};
use nros_rmw_xrce_cffi::{NrosRmwXrceTransportOps, set_custom_transport_ops};
use nros_std_msgs::msg::Int32;

// The board's UART driver, behind four C-ABI callbacks.
unsafe extern "C" fn uart_open(_: *mut c_void, _: *const c_void) -> i32 { 0 }
unsafe extern "C" fn uart_close(_: *mut c_void) {}
unsafe extern "C" fn uart_write(_: *mut c_void, _buf: *const u8, len: usize) -> i32 { len as i32 }
unsafe extern "C" fn uart_read(_: *mut c_void, _buf: *mut u8, _len: usize, _timeout_ms: u32) -> i32 { 0 }

// The executor's storage, owned by the application (in .bss, priced by mem-report).
const WORDS: usize = ExecutorSizing::DEFAULT.u64_len();
static mut BACKING: [MaybeUninit<u64>; WORDS] = [MaybeUninit::uninit(); WORDS];

pub fn run() -> ! {
    // 1. Install the transport (UART: framing on), before any session opens.
    let ops = NrosRmwXrceTransportOps {
        user_data: core::ptr::null_mut(),
        open: Some(uart_open),
        close: Some(uart_close),
        write: Some(uart_write),
        read: Some(uart_read),
    };
    unsafe { set_custom_transport_ops(&ops, true) }.expect("transport");
    // 2. Register the backend (needed where no `.init_array` constructor runs).
    nros_rmw_xrce_cffi::register().expect("xrce register");
    // 3. Open the executor over the application's own storage.
    let config = ExecutorConfig::new("custom://").node_name("talker");
    let backing: &'static mut [MaybeUninit<u64>] = unsafe { &mut *(&raw mut BACKING) };
    let mut executor = unsafe { Executor::open_in(&config, backing, ExecutorSizing::DEFAULT) }
        .expect("session open");
    // 4. Register nodes and their entities.
    let mut node = executor.create_node("talker").expect("node");
    let publisher = node.create_publisher::<Int32>("/count").expect("publisher");
    // 5. Spin from the application's own loop (RTIC idle, superloop, ...).
    let mut n = 0;
    loop {
        let _ = publisher.publish(&Int32 { data: n });
        n += 1;
        let _ = executor.spin_once(Duration::from_millis(10));
    }
}
```

What each step does:

1. **The transport.** `set_custom_transport_ops` takes four callbacks over your
   UART driver. `read` returns the number of bytes read, 0 on a timeout;
   `write` returns the number written. The second argument turns on framing:
   pass `true` for a byte stream (UART, USB-CDC) and `false` for a transport
   that already delivers whole packets. The full contract is in
   `nros_rmw_xrce.h`, beside the backend.
2. **The backend.** `register()` adds XRCE to the backends an executor can
   open. A hosted build does this from a constructor before `main`; bare metal
   runs none, so call it yourself. Calling it twice is harmless.
3. **The executor.** `Executor::open_in` opens the session over a buffer you
   own, here a `static` of `ExecutorSizing::DEFAULT.u64_len()` words. It sits
   in `.bss`, so a memory report counts it. The `custom://` locator selects
   the transport from step 1. If the Agent is not up yet the open fails and
   hands the buffer back, so you can retry it in a loop.
4. **Nodes and entities.** Create them from the executor as on any other
   board.
5. **The loop.** `spin_once` does one round of work and returns within the
   time you pass. In RTIC, call it from `idle`; in a superloop, once per pass.

## Use your own message packages

Code generation finds message packages in this order: the directories in
`NROS_INTERFACE_SEARCH_PATH`, then `AMENT_PREFIX_PATH`, then the copies
nano-ros bundles. Point the variable at your own packages, as a list separated
by `:` or `;`. Each entry is a colcon-style `src/` directory, whose
subdirectories each hold one package with its `package.xml`:

```console
export NROS_INTERFACE_SEARCH_PATH="$HOME/fw/msgs/src"
```

Code generation reads it whether a nano-ros build runs it or CMake's
`nros_generate_interfaces` does.
[Message generation](../../reference/message-generation.md) covers the bundled
copies and the error a missing package gives.

## When the Agent restarts

An Agent that restarts forgets the node's session. The node notices about a
second after the Agent is back: it asks whether the Agent still holds the
session, and on "no" marks it lost and logs one line saying so. From then on
every publish and spin returns `SessionLost`, and `executor.session_lost()` is
true; it stays true. To reconnect, drop everything the old session owns and open again over
the same storage:

```rust
loop {
    // The previous executor was dropped, so its borrow of BACKING has ended.
    let backing: &'static mut [MaybeUninit<u64>] = unsafe { &mut *(&raw mut BACKING) };
    let Ok(mut executor) = (unsafe { Executor::open_in(&config, backing, ExecutorSizing::DEFAULT) })
    else {
        continue; // Agent not up yet: retry
    };
    let mut node = executor.create_node("talker").expect("node");
    let publisher = node.create_publisher::<Int32>("/count").expect("publisher");
    drop(node);
    let mut n = 0;
    while !executor.session_lost() {
        let _ = publisher.publish(&Int32 { data: n });
        n += 1;
        let _ = executor.spin_once(Duration::from_millis(10));
    }
    // The Agent restarted: drop the entities first, then the executor.
    drop(publisher);
    drop(executor);
}
```

Re-opening takes no new memory: the dropped executor hands the buffer back,
and the next open uses it again. Drop the entity handles before the executor,
since they belong to the old session.

An Agent that is merely down marks nothing lost. To ask on your own schedule,
`executor.ping(timeout_ms)` returns `Ok(())`, a `Timeout` (no answer yet,
which proves nothing), or `SessionLost`. A loop that calls `spin_once` does
not need it. A publish that the Agent stopped acknowledging returns
`WouldBlock`.

## If it doesn't work

- **The link fails on `nros_platform_udp_*`.** The build did not see
  `NROS_LINK_IP=0`. Set it in the environment of the `cargo build` that links
  the image, and rebuild from clean.
- **The executor never opens.** Check that `register()` ran first and the
  locator is `custom://`. Then check the serial line: the Agent must listen on
  the same port at the same baud rate, with framing on.
- **The first allocation fails or the heap looks empty.** If the board places
  its heap in a `NOLOAD` section, such as CCM, zero that section before the
  first allocation. `cortex-m-rt` zeroes only `.bss`, and the heap reads
  all-zero memory as not yet set up.
