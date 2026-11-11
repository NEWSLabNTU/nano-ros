# `custom-transport-loopback` — Phase 115.F

Pure-C demonstration of the runtime-pluggable transport vtable
(`nros_transport_ops_t`). The example installs a transport via
`rclc_set_custom_transport`, opens an nros session over `custom/loopback`,
and exercises the four callbacks (`open`, `close`, `write`, `read`)
end-to-end. Exits non-zero if any of the four never fires.

The callbacks carry zenoh-pico's wire bytes over a TCP connection the
program opens itself, to a zenoh router (`NROS_CUSTOM_TCP_TARGET`,
`host:port`, default `127.0.0.1:7447`). zenoh-pico frames its own wire
protocol, so the callbacks pass bytes through untouched; replace the socket
with a UART, USB CDC or a shared-memory ring and nothing else changes. The far
end must answer the session's handshake: a buffer that only echoes the
image's own bytes back cannot, and the session refuses.

## Build

A one-package workspace: `src/native_c_custom_transport_loopback` holds the
program, and `src/native_c_custom_transport_loopback_bringup` holds the
`system.toml` that says what to build. The program keeps its own `main`
(registering the transport is the thing a user writes), so every image names
it as its owned entry and `nros build` builds it rather than generating one.

The image's `locator = "custom/loopback"` in the bringup's `system.toml`
is what compiles zenoh-pico's custom link into it; without it the `custom`
scheme is unknown and the session refuses before `open` runs.

```sh
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_c_custom_transport_loopback_bringup:linux
ros2 run rmw_zenoh_cpp rmw_zenohd &      # the far end of the link
nros run linux
```

Expected output:

```
custom link: spinning for ~3 seconds (Ctrl-C to stop sooner)
Published: 0
...
custom link callback counts:
  open:  1
  write: ≥1
  read:  ≥1
  close: 1
```

Process exit code is `0` when every callback fired at least once.

## What this proves

* `rclc_set_custom_transport(&ops)` with the V1 `abi_version`
  returns `RCL_RET_OK`.
* Session bring-up calls `open` exactly once.
* Every publish drives `write`.
* The executor's spin tick polls `read` (returns 0 when nothing
  arrived, the byte count when bytes did).
* Teardown (`rclc_support_fini`) drives `close` once.

## Waiting in `read`

zenoh-pico's receive loop calls `read` with a timeout of 0 and leaves the
bound to the transport, so `read` waits at least 1 ms (`poll(2)`) rather
than return at once and spin a core; while the session opens it is asked
with a real timeout and waits up to that. On bare metal, wait on the
semaphore or interrupt your medium signals instead.

## See also

* `book/src/extending/add-a-transport.md` — full porting guide.
* `examples/linux/rust/custom-transport-{talker,listener}/`
  — Rust-side equivalents: a talker and a listener, each bridging its
  session to a zenoh router over a TCP connection it opens itself.
* `packages/core/nros-rmw-abi/include/nros/rmw_transport.h` —
  canonical C ABI header for `nros_transport_ops_t`.
