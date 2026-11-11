# `custom-transport-loopback` — Phase 115.F

Pure-C demonstration of the runtime-pluggable transport vtable
(`nros_transport_ops_t`). The example installs a ring-buffer-backed
transport via `rclc_set_custom_transport`, opens an nros session
over `custom://loopback`, and exercises the four callbacks (`open`,
`close`, `write`, `read`) end-to-end. Exits non-zero if any of the
four never fires.

The transport callbacks are platform-neutral — every line below
the `/* loopback ring buffer */` block would compile unchanged on
Cortex-M / Zephyr / FreeRTOS. POSIX is used here only because
pthread synchronisation primitives are easiest to wire on a
hosted target.

## Build

A one-package workspace: `src/native_c_custom_transport_loopback` holds the
program, and `src/native_c_custom_transport_loopback_bringup` holds the
`system.toml` that says what to build. The program keeps its own `main`
(registering the transport is the thing a user writes), so every image names
it as its owned entry and `nros build` builds it rather than generating one.

```sh
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_c_custom_transport_loopback_bringup:linux
nros run linux
```

Expected output:

```
loopback: spinning for ~3 seconds (Ctrl-C to stop sooner)
loopback callback counts:
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
* The executor's spin tick polls `read` (returns 0 when the
  ring is empty, the byte count when there's a frame).
* Teardown drives `close` via `rclc_set_custom_transport(NULL)`.

## Threading contract

The transport's `read` and `write` are never invoked concurrently
from different threads — the executor serialises them through its
drive-io tick. The example uses pthread synchronisation primitives
only for the `read` block-with-timeout behaviour (waiting on
`pthread_cond_timedwait` for the matching `write` to fill the
ring). On bare-metal you would replace that with a semaphore /
WFE wake.

## See also

* `book/src/extending/add-a-transport.md` — full porting guide.
* `examples/linux/rust/custom-transport-{talker,listener}/`
  — Rust-side equivalents (two-process loopback over a Unix
  socket).
* `packages/core/nros-rmw-abi/include/nros/rmw_transport.h` —
  canonical C ABI header for `nros_transport_ops_t`.
