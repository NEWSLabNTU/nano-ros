---
rfc: 0021
title: "Blocking API Rules"
status: Stable
area: runtime-api
since: 2026-04
last-reviewed: 2026-11-13
implements-tracked-by: [phase-82]
supersedes: []
superseded-by: null
---

# Blocking API Rules

> **Review 2026-11-13.** The rule holds in the code. The C names below are
> the ones the C API had before it took rclc's names (RFC-0089): the
> blocking call is now `rclc_client_call`, the registration
> `rclc_executor_add_client`, the executor `rclc_executor_t`, and the guard
> returns `RCLC_RET_REENTRANT` (`packages/api/nros-c/src/service.rs:2445`,
> `packages/api/nros-c/src/executor.rs:3447`, `executor.rs:415-423`). The
> Rust `Promise::wait` takes a `core::time::Duration`, not milliseconds
> (`packages/core/nros-node/src/executor/handles.rs:2788`). The C++ form is
> `fut.wait(executor.handle(), timeout_ms, out)` or
> `rclcpp::spin_until_future_complete`
> (`packages/api/nros-cpp/include/rclcpp_action/client.hpp:474`).

## The rule

> Every blocking helper exposed to user code must take an executor handle
> and drive that executor while it waits.

This applies to all three language bindings (Rust, C, C++).

## Rationale

1. **Single source of I/O.** Only `spin_*()` may call into the
   transport's read path. A blocking call that bypasses the executor
   either deadlocks on single-threaded transports (no read task to
   deliver the reply) or starves timers/subscriptions/parameter services
   on multi-threaded ones.

2. **Reentrancy safety.** When a blocking helper drives the executor,
   the user can see (via the executor argument) that calling it from
   inside another callback is reentrant. A blocking helper that takes no
   executor looks innocent and silently breaks.

3. **Timeout semantics.** Condvar timed waits are unreliable across our
   platform matrix (NuttX kernel `nxsem_clockwait` hang, FreeRTOS QEMU
   lease-task starvation, icount virtual-time skew). Spinning the
   executor + checking a non-blocking poll is the only timeout mechanism
   that works uniformly.

## Compliant patterns

### Rust

```rust
let promise = client.call(&request)?;         // non-blocking send
let response = promise.wait(&mut executor, 5000)?; // spins executor
```

`Promise::wait` is the only blocking convenience. The lower-level
`call_raw` (deprecated in Phase 82.9) was deleted from the RMW trait/vtable surface in phase-301 — `send_request_raw` + `take_response_raw` is the one path.

### C

```c
nros_executor_add_client(&executor, &client);  // required registration
nros_client_call(&client, req, req_len, resp, cap, &resp_len);
```

`nros_client_call` reads the stashed `executor_ptr` from the client's
internal storage and spins it via `rclc_executor_spin_some`. No
signature change — the executor dependency is captured at registration
time. Same pattern for `nros_action_send_goal` and
`nros_action_get_result`.

### C++

```cpp
auto fut = client.send_request(req);           // non-blocking send
NROS_TRY(fut.wait(executor.handle(), 5000, resp)); // spins executor
```

Every operation with a deferred response returns `Future<T>`. There are
no blocking overloads.

## Reentrancy guard

`nros_executor_t` carries an `in_dispatch` flag set by
`rclc_executor_spin_some` for the duration of the dispatch loop.
Blocking helpers check this flag and return `NROS_RET_REENTRANT` (C) or
`ErrorCode::Reentrant` (C++) immediately if a callback re-enters.

## History

- **Phase 77**: established the rule for the action client path.
- **Phase 82**: extended to the service client and documented as a
  project-wide invariant.

## Verification

```bash
# No remaining zpico_get calls in core/zpico paths
grep -rn 'zpico_get\b' packages/core/ packages/rmw/zenoh/nros-rmw-zenoh/
# Should return zero results outside zpico_get_start / zpico_get_check
```
