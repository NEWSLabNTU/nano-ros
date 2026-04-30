# nros rmw-cffi {#mainpage}

C vtable for plugging a third-party RMW backend into nros. Use this
surface when nano-ros's pre-built RMW crates (`nros-rmw-zenoh`,
`nros-rmw-xrce`) do not cover your transport.

## When to use this

| Path | Read | Use case |
|------|------|----------|
| **Pre-built Rust RMW** | nros::reference | Zenoh-pico (`rmw-zenoh`) or XRCE-DDS (`rmw-xrce`). |
| **Custom Rust RMW** | book — porting/custom-rmw | New transport (uORB, FastDDS, custom UDP, …) — preferred path. |
| **Custom C RMW via this vtable** | this site + porting/custom-rmw | New transport, must stay in C. |

## Quick start

1. Build nano-ros with the `rmw-cffi` feature:

   ```bash
   cargo build -p nros --features rmw-cffi,platform-posix,std
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

The vtable groups by entity (see @ref nros_rmw_vtable_t):

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
  `call_raw`. `call_raw` is currently synchronous; the caller blocks
  on the executor.

## Return-value conventions

```
open                     non-NULL = success, NULL = error
close/drive_io/
  publish_raw/send_reply 0 = success, negative = error
try_recv_raw             positive = bytes received, 0 = no data, negative = error
try_recv_request         positive = bytes received (seq_out written), 0 = none, negative = error
has_data/has_request     1 = yes, 0 = no
call_raw                 positive = reply bytes, negative = error
destroy_*                void (best-effort cleanup)
```

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
  — full Rust + C walkthrough, factory pattern, lifecycle.
- The [`nros-rmw-cffi` source tree](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-rmw-cffi)
  — header + crate sources for this vtable.
