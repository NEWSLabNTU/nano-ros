# Error Codes {#error_codes}

`rcl_ret_t` is the canonical return type for all C-API functions that
can fail. Zero (`RCL_RET_OK`) means success; any non-zero value
identifies a specific error.

## Return Code Table

| Value | Constant | Cause | Recovery |
|-------|----------|-------|----------|
| `0` | `RCL_RET_OK` | Success | — |
| `-1` | `RCL_RET_ERROR` | Generic failure not covered by a more specific code. | Inspect logs; check the function-specific docs. |
| `-2` | `RCL_RET_TIMEOUT` | Operation deadline elapsed before completion. | Retry, increase the timeout, or verify the remote peer is reachable. |
| `-3` | `RCL_RET_INVALID_ARGUMENT` | A required pointer was `NULL`, an enum was out of range, or a string was empty. | Validate inputs before the call. |
| `-4` | `RCLC_RET_NOT_FOUND` | Named resource (parameter, service, topic) does not exist. | Verify spelling; check for race vs creation order. |
| `-5` | `RCLC_RET_ALREADY_EXISTS` | A duplicate name or handle is being registered. | Use a unique name, or skip if idempotent. |
| `-6` | `RCLC_RET_FULL` | Static pool exhausted. | Raise the matching `NROS_*_BUFFER_SIZE` env var and rebuild. See @ref configuration. |
| `-7` | `RCL_RET_NOT_INIT` | Object was never initialised, or `nros_*_init()` returned an error. | Initialise before use; check init's own return code. |
| `-8` | `RCLC_RET_BAD_SEQUENCE` | API contract violated (e.g., `_fini` before `_init`). | Audit the call sequence; rely on `_get_zero_initialized()` for safe defaults. |
| `-9` | `RCLC_RET_SERVICE_FAILED` | Service call returned a server-side error. | Inspect the response payload; check server-side logs. |
| `-10` | `RCLC_RET_PUBLISH_FAILED` | Underlying transport refused the publish. | Often paired with a `_Z_ERR_*` log line; see @ref troubleshooting. |
| `-11` | `RCLC_RET_SUBSCRIPTION_FAILED` | Underlying transport refused the subscription. | Verify locator + zenohd reachability. |
| `-12` | `RCLC_RET_NOT_ALLOWED` | Operation rejected by policy (e.g., immutable parameter). | Check parameter declarations and read-only flags. |
| `-13` | `RCLC_RET_REJECTED` | Goal or request rejected by the server's accept callback. | Inspect server-side logging. |
| `-14` | `RCLC_RET_TRY_AGAIN` | Transient — no data ready yet (non-blocking take). | Retry on the next executor tick. |
| `-15` | `RCLC_RET_REENTRANT` | A blocking call (e.g., service client `take`) was made from inside a callback. | Re-architect to use the executor or an async path. |

## zenoh-pico Underlying Errors

When `RCLC_RET_PUBLISH_FAILED` or `RCLC_RET_SUBSCRIPTION_FAILED` is
returned, the underlying zenoh-pico error is logged to stderr (or the
platform log sink). Common values:

| Code | Name | Meaning |
|------|------|---------|
| -3 | `_Z_ERR_TRANSPORT_OPEN_FAILED` | Cannot connect to router |
| -73 | `_Z_ERR_SESSION_CLOSED` | Session closed after failure |
| -78 | `_Z_ERR_SYSTEM_OUT_OF_MEMORY` | Allocation failed |
| -100 | `_Z_ERR_TRANSPORT_TX_FAILED` | Transport transmission failed |
| -128 | `_Z_ERR_GENERIC` | Generic error |

## Pattern: Handling Errors

```c
rcl_ret_t ret = nros_publisher_init(&pub, &node, &type_info, "/topic");
if (ret != RCL_RET_OK) {
    fprintf(stderr, "publisher_init failed: %d\n", ret);
    return ret;
}
```

There is no exception machinery — every fallible function returns its
own `rcl_ret_t`. Functions that *can't* fail return `void`.

## See Also

- @ref troubleshooting — symptom-driven diagnostics
- @ref configuration — buffer-size environment variables
