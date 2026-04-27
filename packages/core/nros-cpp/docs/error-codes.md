# Error Codes {#error_codes}

`nros::Result` wraps an `nros::ErrorCode` (a strongly-typed `int32_t`
enum). Use `result.ok()` for success checks, or the `NROS_TRY(expr)`
macro to short-circuit on the first error.

## ErrorCode Table

| Raw | `nros::ErrorCode` | Cause | Recovery |
|-----|------|-------|----------|
| `0` | `Ok` | Success | — |
| `-1` | `Error` | Generic failure not covered by a specific code. | Inspect logs and the function-specific docs. |
| `-2` | `Timeout` | Operation deadline elapsed before completion. | Retry, increase the timeout, or verify the remote peer is reachable. |
| `-3` | `InvalidArgument` | Null pointer, empty topic name, or out-of-range value. | Validate inputs before the call. |
| `-4` | `NotInitialized` | `nros::init()` was never called or returned an error; or the entity (publisher, subscription, …) is in a default-constructed state. | Call `nros::init()` first; check `is_valid()`. |
| `-5` | `Full` | Static pool exhausted (executor slots, subscription buffers, parameter table, …). | Raise the matching `NROS_*` env var and rebuild. See @ref configuration. |
| `-6` | `TryAgain` | Transient — no data ready yet (non-blocking take). | Retry on the next executor tick. |
| `-7` | `Reentrant` | A blocking call was made from inside a callback. | Re-architect to use the executor or an async path. |
| `-100` | `TransportError` | Underlying transport rejected the operation. | Often paired with a `_Z_ERR_*` log line; see @ref troubleshooting. |

## NROS_TRY

```cpp
nros::Result init_pubsub(nros::Node& node) {
    nros::Publisher<MyMsg> pub;
    NROS_TRY(node.create_publisher(pub, "/topic"));   // early-return on error
    NROS_TRY(pub.publish(seed_msg));
    return nros::Result::success();
}
```

`NROS_TRY(expr)` evaluates `expr` once and, if the result is not
success, returns it from the enclosing function. Only valid inside
functions that themselves return `nros::Result`.

## Pattern: Manual Error Handling

```cpp
auto ret = node.create_publisher(pub, "/topic");
if (!ret.ok()) {
    std::fprintf(stderr, "create_publisher failed: %d\n", ret.raw());
    return ret;
}
```

`Result::raw()` returns the underlying `int32_t` for logging.
`Result::code()` returns the typed `ErrorCode` for `switch` dispatch.

## zenoh-pico Underlying Errors

`TransportError` typically wraps one of these zenoh-pico return codes:

| Code | Name | Meaning |
|------|------|---------|
| -3 | `_Z_ERR_TRANSPORT_OPEN_FAILED` | Cannot connect to router |
| -73 | `_Z_ERR_SESSION_CLOSED` | Session closed after failure |
| -78 | `_Z_ERR_SYSTEM_OUT_OF_MEMORY` | Allocation failed |
| -100 | `_Z_ERR_TRANSPORT_TX_FAILED` | Transport transmission failed |
| -128 | `_Z_ERR_GENERIC` | Generic error |

## See Also

- @ref troubleshooting — symptom-driven diagnostics
- @ref configuration — buffer-size environment variables
