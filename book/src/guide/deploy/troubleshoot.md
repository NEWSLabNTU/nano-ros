# Troubleshoot

<!-- type: how-to · level: 2 -->

**Needs first:** [Run a talker](../../tutorial/run-a-talker.md)

Use this page when a build or a running image fails in a way the page you were
following does not cover. Find the symptom, then apply its fix.

Some topics keep their own failures:

- First build and first run: the [troubleshooting index](../../reference/troubleshooting-index.md).
- ROS 2 sees nothing (domain ID, QoS, client mode, edition):
  [Talk to ROS 2](../../tutorial/talk-to-ros2.md#troubleshooting).
- Serial links: [Use a serial link](../rmw/use-a-serial-link.md#if-it-doesnt-work).
- Several backends in one binary:
  [Bridge between backends](../rmw/bridge-between-backends.md#if-it-doesnt-work).

## Before you start

Run the doctor in the workspace. It checks the SDK paths, toolchains and
environment, and names what is missing:

```bash
nros doctor
```

Outside a workspace it checks the install instead, and reports store entries
you can remove, with the command to remove them.

## Steps

1. Run `nros doctor` and fix what it names.
2. Run the failing step again with `--dry-run` where it has one
   (`nros build`, `nros run`, `nros flash`), to see the plan and the exact
   command.
3. Find the symptom in the sections below.

## zenoh-pico multiple client issues

**Symptom:** the first of several zenoh clients on one router starts, and the
next fails to open its session (`-102`, `_Z_ERR_TRANSPORT_OPEN_FAILED`) or to
declare a publisher or subscriber (`-78`, `_Z_ERR_SYSTEM_OUT_OF_MEMORY`, or
`-128`, `_Z_ERR_GENERIC`).

**Fix:** `-102` means the client could not reach the router: check its
`locator` and that the router listens there. `-78` is an allocation that did
not fit. Raise the image's heap, or the zenoh pool that filled, which the
image names at start-up; the knobs are in
[Environment variables](../../reference/environment-variables.md#buffer-tuning).

## zenoh-pico version compatibility

**Symptom:** publishing fails at once with `-100`
(`_Z_ERR_TRANSPORT_TX_FAILED`), then `-73` (`_Z_ERR_SESSION_CLOSED`).

**Fix:** compare the zenoh on both sides. nano-ros does not pin the router,
because the router is the one ROS ships:

```bash
grep 'define ZENOH_C ' \
    /opt/ros/$ROS_DISTRO/opt/zenoh_cpp_vendor/include/zenoh_configure.h
nros setup --list | grep zenoh-pico
```

Read the header, not `dpkg -l`: the package version is a wrapper version.
Under zenoh's 1.x wire guarantee the two need not match, so a mismatch is
evidence rather than a diagnosis.

## Multiple Zephyr native_sim instances

### Ephemeral port conflict

**Symptom:** two native_sim instances (a talker and a listener) both fail to
open their sessions, or one fails with `-100`.

**Cause:** native_sim's test entropy source gives every instance the same
random sequence, so both pick the same ephemeral port.

**Fix:** give each instance its own seed:

```bash
./build-listener/zephyr/zephyr.exe --seed=12345
./build-talker/zephyr/zephyr.exe --seed=67890
```

### Cyclone DDS: two native_sim nodes never discover each other

**Symptom:** with Cyclone DDS, the publisher prints
`Publishing: 'Hello World: N'` and the subscriber never prints `I heard:`, with
no error.

**Cause:** the same fixed entropy gives both participants the same DDSI GUID
prefix, and a node drops discovery from a peer that shares its own GUID.

**Fix:** distinct seeds, as above. On native_sim, Cyclone discovery is
unicast to `127.0.0.1`. Real hardware has real entropy and multicast, and
needs neither workaround.

## Network configuration issues

- **QEMU cannot reach the network.** An image on QEMU's built-in `qemu`
  network uses user-mode networking and needs nothing on the host. An image
  on a network of your own needs `[image.<id>.run] nic` naming a host bridge
  or TAP device; see
  [Configure an image](../build/configure-an-image.md#give-the-image-an-address).
- **Zephyr native_sim.** Sockets are forwarded to host system calls, so there
  is no bridge to configure. Point each image at a router on host loopback,
  `tcp/127.0.0.1:7447`.

## An XRCE node goes silent after its Agent restarts

**Symptom:** a node on XRCE-DDS keeps running, but ROS 2 no longer sees its
topics after the micro-ROS Agent was restarted. The node logs one line,
`XRCE session lost: the Agent is reachable but no longer holds this session (it restarted)`.

**Cause:** an Agent that restarts forgets every session it held. About a
second after it is back, the node asks it whether it still holds the session,
and on "no" marks the session lost. From then on every publish, spin and ping
returns `SessionLost` (C: `RCLC_RET_NOT_FOUND`; C++:
`NROS_CPP_RET_TRANSPORT_ERROR`), and
`Executor::session_lost()` is true. An Agent that is only down marks nothing:
the node carries on once the same Agent answers again.

**Fix:** drop the executor and open it again, then create the nodes and
entities again. The loop is in
[When the Agent restarts](../platforms/bare-metal-own-main-loop.md#when-the-agent-restarts).
How soon the node asks is `XRCE_SESSION_PROBE_MS` (override it with `-D`), in the
[compile-time XRCE table](../../reference/environment-variables.md#compile-time-only-not-environment-variables).

## Message too large or truncated

**Symptom:** `TransportError::MessageTooLarge`, payloads that arrive
corrupted, or large messages (images, point clouds) that disappear.

**Cause:** a message passes several static buffers on its way, and must fit
each: zenoh's defragmentation and batch sizes, XRCE's transport MTU, and the
entity's own receive buffer.

**Fix:** raise the transport limit for the image, for example
`NROS_ZENOH_FRAG_MAX_BYTES` or `XRCE_TRANSPORT_MTU`, from the
[buffer tuning tables](../../reference/environment-variables.md#buffer-tuning),
and size the entity's buffer in code:

```rust
let sub = node.create_subscription_sized::<MyMsg, 4096>(/* … */)?;
```

## Build issues

- **The zenoh-pico source is not found.** Let provisioning fetch it:
  `nros setup <board> --rmw zenoh`, or `nros setup --source zenoh-pico`.
- **A changed knob has no effect.** An old CMake cache holds the previous
  value. Remove the image's directory under `build/` and build again.

### The link fails: a region overflowed

**Symptom:** the build stops at the link with the linker's line, for
example on a nucleo-f767zi-zephyr image whose heap was raised:

```text
region `RAM' overflowed by 122432 bytes
```

`nros build` ends with the command that failed and, after it, what the
image reserves in that region:

```text
nros build: image `talker_node_bringup:zephyr` does not fit its memory: the link overflowed `RAM` by 122,432 B.
  What the image reserves:
    main stack  33,368 B, CONFIG_MAIN_STACK_SIZE
      measured under the stack-floor row `talker/rust/zenoh@nucleo-f767zi-zephyr`: static walk 69,648 B (an upper bound through an indirect call, not a size), measured peak 28,888 B, margin 10 %
    heap        393,216 B, `[image.zephyr.knobs.memory] heap_bytes`
  To make room, lower what the image adds (a pool or cache knob it raised), the heap, or the main stack: `CONFIG_MAIN_STACK_SIZE` in a conf file `[image.zephyr] os_config` names.
```

**Fix:** give back at least the bytes it names. Lower a knob you raised (a
pool, a cache, the heap) before the main stack: the stack-floor row is the
stack the image was measured to need. The whole tool output is in the
image's build log; [What `nros build` produces](../../reference/build-outputs.md#the-builds-log)
says where.

## Rust and C FFI issues

### Subscriber callback crashes

**Symptom:** a callback registered from Rust through C crashes, or reads
garbage fields, such as `sub->callback=0x60`.

**Cause:** the C side kept a pointer to a struct the Rust code then moved,
for example by returning it from a function.

**Fix:** give the struct a stable address before you hand it to C: a
`static` (with `MaybeUninit`), a `Box`, or `Pin`. Or have the C side keep an
index instead of a pointer.

### Function pointer ABI mismatch

**Symptom:** a callback passed between Rust and C receives garbage
arguments.

**Fix:** declare it `extern "C" fn`, not a plain `fn`.

## Example

The usual first pass on a failing image:

```console
nros doctor
nros build <image> --dry-run
nros run <bringup>:<image> --dry-run --why
```

## If it doesn't work

- Look the error up in the
  [troubleshooting index](../../reference/troubleshooting-index.md), which
  also lists the zenoh-pico error codes.
- File an issue with the doctor's output and the dry-run plan.

## See also

- [Log and observe a board](log-and-observe.md): what to read when a board
  has no console.
- [Deploy to production](deploy-to-production.md): the run plan and the
  verify step.
