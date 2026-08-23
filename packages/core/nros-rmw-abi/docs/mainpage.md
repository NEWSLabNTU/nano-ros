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
   #include <nros/rmw_ret.h>
   #include <nros/rmw_entity.h>

   /* Backend writes its own session pointer into out->backend_data.
    * The runtime has already filled out->node_name + out->namespace_. */
   static rmw_ret_t my_open(const char* locator, uint8_t mode,
                                 uint32_t domain_id, const char* node_name,
                                 rmw_session_t* out) {
       out->backend_data = /* my_session_t pointer */;
       return NROS_RMW_RET_OK;
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

- **Session** — `create_session`, `destroy_session`, `drive_io`.
  `drive_io(timeout_ms)` is
  the executor's I/O drive call; it must dispatch any pending
  receive/send work and return within the given timeout.
- **Publisher** — `create_publisher`, `destroy_publisher`,
  `publish_raw`. Raw payloads are CDR-encoded by the upper layer.
- **Subscription** — `create_subscription`, `destroy_subscription`,
  `take`, `has_data`. `take` is non-blocking; report `taken = false`
  with `NROS_RMW_RET_OK` when no data is ready (phase 376 W3.d
  retired the `NROS_RMW_RET_NO_DATA` sentinel here — see below).
- **Service** — `create_service`, `destroy_service`,
  `take_request`, `has_request`, `send_reply`. The `seq_out`
  parameter on `try_recv_request` carries the request sequence number
  forwarded back to `send_reply`.
- **Client** — `create_client`, `destroy_client`,
  `send_request_raw`, `take_response` (non-blocking pair; the
  executor drives I/O between them — there is no blocking call slot).

## Return-value conventions

Status is reported as `rmw_ret_t` — a signed 32-bit integer whose VALUES
are upstream rmw's (phase 376 W3.d step B): `OK 0`, `ERROR 1`, `TIMEOUT 2`,
`UNSUPPORTED 3`, `BAD_ALLOC 10`, `INVALID_ARGUMENT 11`,
`NODE_NAME_NON_EXISTENT 203`. Codes upstream does not define live in the
extension range at `NROS_RMW_RET_EXTENSION_BASE` (1000) and above, so a future
upstream addition can never collide with one of ours.

Zero is success; **nothing returns a negative value any more**. Do not test a
status by its sign — compare against a named constant. Pointer-returning calls
still signal failure with `NULL`.

```
create_session           non-NULL = success, NULL = error
destroy_session/drive_io/
  publish_raw/send_reply NROS_RMW_RET_OK = success, else a named error code
take                     NROS_RMW_RET_OK + *taken / *out_len; else a named error
take_request             NROS_RMW_RET_OK + *taken / *out_len / *seq_out; else a named error
has_data/has_request     NROS_RMW_RET_OK + *out_has_{data,request}; else a named error
send_request_raw         NROS_RMW_RET_OK = queued, else a named error code
take_response            NROS_RMW_RET_OK + *taken / *out_len; else a named error
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
- `publish_raw`, `take`, and `send_reply` may run concurrently
  from different threads — the backend is responsible for any
  required serialisation.
- `send_request_raw` / `take_response` are non-blocking; the
  executor drives I/O between them.

## See also

- The [Custom RMW Backend porting guide](https://github.com/NEWSLabNTU/nano-ros/blob/main/book/src/porting/custom-rmw.md)
  — step-by-step walkthrough, factory pattern, lifecycle.
- The [`nros-rmw-cffi` source tree](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/cffi)
  — header + library sources for this vtable.
