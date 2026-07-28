---
id: 339
title: "rclcpp_compat's spin_until_future_complete ignores the future on the timeout path — always burns the full timeout, and returns void so SUCCESS is indistinguishable from TIMEOUT"
status: open
type: bug
severity: medium
area: core, api
related: [issue-0338, rfc-0019]
---

## Finding (deep audit C,E 2026-07-28 — C6)

`packages/core/nros-cpp/include/nros/rclcpp_compat.hpp:477` — the compat shim for
`rclcpp::spin_until_future_complete`:

- On the **explicit-timeout** path it calls the bounded `Executor::spin(timeout_ms)`
  and never consults the future. So it **always burns the whole timeout**, even when
  the future completed on the first `spin_once`. A `wait_for_service` /
  `send_request` sequence written against rclcpp habits therefore pays the full
  timeout on every successful call.
- It returns **`void`**, so a caller cannot distinguish success from timeout.

**Upstream counterpart:** `rclcpp::spin_until_future_complete` returns
`rclcpp::FutureReturnCode` (`Success` / `Timeout` / `Interrupted`) and returns as
soon as the future is ready. Both properties are load-bearing for the standard
service-client idiom:

```cpp
if (rclcpp::spin_until_future_complete(node, future) == rclcpp::FutureReturnCode::SUCCESS) { … }
```

which cannot be written against this shim at all.

Note the `timeout_ms < 0` branch immediately above already has the correct shape
(it loops and checks readiness) — so the right implementation is present in the same
function, and the timeout branch simply does not use it.

## Fix

1. In the timeout branch, loop `spin_once` while
   `ok() && !future.is_ready() && !deadline_passed()` — the same shape as the
   `timeout_ms < 0` branch above it.
2. Return an enum mirroring `rclcpp::FutureReturnCode`
   (`Success` / `Timeout` / `Interrupted`). Keep a `void`-returning overload only if
   source compatibility demands it, and mark it deprecated.

## Why this is worth fixing even though it is "just" a compat header

`rclcpp_compat.hpp` exists to let ported rclcpp code compile unchanged. A shim that
compiles the standard idiom but silently changes its timing and discards its result
is worse than not providing the symbol: the port looks successful and the timing
regression shows up later as "nano-ros services are slow".
