# nros rmw-cffi {#mainpage}

C function-pointer table for plugging a third-party RMW backend into
nano-ros. Use this surface when nano-ros's pre-built RMW backends
(zenoh-pico, XRCE-DDS, dust-DDS, uORB) do not cover your transport and
your backend stays in C.

## Quick start

1. Build nano-ros with the `rmw-cffi` option enabled:

   ```bash
   cmake -DNROS_RMW=cffi -DNROS_PLATFORM=posix -B build
   cmake --build build
   ```

2. Implement the vtable in C:

   ```c
   #include <nros/rmw_vtable.h>

   static nros_rmw_handle_t my_open(const char* locator, uint8_t mode,
                                    uint32_t domain_id, const char* node_name) {
       return /* my_session_t */;
   }
   /* ... fill in every field ... */

   static const nros_rmw_vtable_t VTABLE = {
       .open                   = my_open,
       .close                  = my_close,
       .drive_io               = my_drive_io,
       /* ... */
   };
   ```

3. Register before any nros call:

   ```c
   int main(void) {
       nros_rmw_cffi_register(&VTABLE);
       /* now you can call nros_init(), nros_node_init(), ... */
   }
   ```

## Vtable structure

The vtable is a struct of function pointers grouped by entity (see
@ref nros_rmw_vtable_t):

- **Session** — `open`, `close`, `drive_io`. `drive_io(timeout_ms)` is
  the executor's I/O drive call; it must dispatch any pending
  receive/send work and return within the given timeout.
- **Publisher** — `create_publisher`, `destroy_publisher`,
  `publish_raw`. Raw payloads are CDR-encoded by the upper layer.
- **Subscriber** — `create_subscriber`, `destroy_subscriber`,
  `try_recv_raw`, `has_data`. `try_recv_raw` is non-blocking; return
  `0` if no data is ready.
- **Service Server** — `create_service_server`, `destroy_service_server`,
  `try_recv_request`, `has_request`, `send_reply`. The `seq_out`
  parameter on `try_recv_request` carries the request sequence number
  forwarded back to `send_reply`.
- **Service Client** — `create_service_client`, `destroy_service_client`,
  `call_raw`. `call_raw` is synchronous; the caller blocks on the
  executor.

## Return-value conventions

Status is reported as `nros_rmw_ret_t` — a signed 32-bit integer.
Zero is success; every error code is a named negative constant in
@ref rmw_ret.h. Pointer-returning calls signal failure with `NULL`.

```
open                     non-NULL = success, NULL = error
close/drive_io/
  publish_raw/send_reply NROS_RMW_RET_OK = success, negative = named error code
try_recv_raw             >= 0 = bytes received (0 = no data), negative = named error code
try_recv_request         >= 0 = bytes received (seq_out written), negative = named error code
has_data/has_request     1 = yes, 0 = no
call_raw                 >= 0 = reply bytes, negative = named error code
destroy_*                void (best-effort cleanup)
```

The full set of named codes (`NROS_RMW_RET_TIMEOUT`,
`NROS_RMW_RET_INVALID_ARGUMENT`, `NROS_RMW_RET_UNSUPPORTED`,
`NROS_RMW_RET_INCOMPATIBLE_QOS`, `NROS_RMW_RET_TOPIC_NAME_INVALID`,
`NROS_RMW_RET_NODE_NAME_NON_EXISTENT`,
`NROS_RMW_RET_LOAN_NOT_SUPPORTED`, `NROS_RMW_RET_NO_DATA`,
`NROS_RMW_RET_WOULD_BLOCK`, `NROS_RMW_RET_BUFFER_TOO_SMALL`,
`NROS_RMW_RET_MESSAGE_TOO_LARGE`, plus the catch-all
`NROS_RMW_RET_ERROR`) is documented at @ref rmw_ret.h.

There is no thread-local error string — the `rmw_set_error_string` /
`rmw_get_error_string` pattern needs heap allocation per thread which
embedded code paths cannot afford. Backends log diagnostic strings at
the failure site through the platform's `printk` equivalent.

## Threading

- The vtable itself is registered once and read concurrently. Function
  pointers must be safe to invoke from any executor thread.
- `drive_io` may block up to `timeout_ms`; it must not hold
  application locks across the wait.
- `publish_raw`, `try_recv_raw`, and `send_reply` may run concurrently
  from different threads — the backend is responsible for any
  required serialisation.
- `call_raw` blocks until the reply arrives or an error occurs.

## See also

- The [Custom RMW Backend porting guide](https://github.com/NEWSLabNTU/nano-ros/blob/main/book/src/porting/custom-rmw.md)
  — step-by-step walkthrough, factory pattern, lifecycle.
- The [`nros-rmw-cffi` source tree](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-rmw-cffi)
  — header + library sources for this vtable.
