---
id: 2114
title: "`rclcpp::ParameterCallbackHandle` is the on-set token (`uint16_t`), not upstream's event-handler handle"
status: resolved
type: bug
area: [cpp, api-parity, parameters]
severity: low
found: 2026-10-27
resolved: 2026-10-29
related: [2024, 2056, phase-515]
---

## Measured

Upstream's `rclcpp::ParameterCallbackHandle` is the handle
`ParameterEventHandler::add_parameter_callback` returns. Ours has two
things under that name:

- `packages/api/nros-cpp/include/rclcpp/node_parameters.hpp`:
  `using ParameterCallbackHandle = uint16_t;` — the token
  `Node::remove_on_set_parameters_callback` takes. Upstream's on-set handle
  is `OnSetParametersCallbackHandle`.
- `packages/api/nros-cpp/include/rclcpp/parameter_event_handler.hpp`:
  `ParameterEventHandler::ParameterCallbackHandle`, a nested struct with
  upstream's `SharedPtr` / `WeakPtr` / `ParameterCallbackType` (issue 2024).

So a ported

```cpp
rclcpp::ParameterCallbackHandle::SharedPtr h =
    handler->add_parameter_callback("rate", cb);
```

fails to compile on `uint16_t`, with a message that does not name the cause.
`auto h = handler->add_parameter_callback(...)` compiles.

Found by phase-515 P3's `misdeclared_absences` check: the ledger row
`cpp:ParameterCallbackHandle` said `declined`/`absent` while the name
correlated `same`. It is a `gap` with an `owed` witness now.

## Fix shape

Make the top-level name upstream's: hoist the event-handler handle to
`rclcpp::ParameterCallbackHandle` (the nested name can stay an alias), and give
the on-set token upstream's `OnSetParametersCallbackHandle` spelling, with
`ParameterCallbackHandle`-as-token callers moved. The ledger row's witness is
the `using ParameterCallbackHandle = uint16_t;` line, which the fix deletes.

## Resolution (2026-10-29)

The fix shape above, with one change: the on-set token's new name.

- `rclcpp::ParameterCallbackHandle` is upstream's handle, at namespace scope in
  `parameter_event_handler.hpp`. `ParameterEventHandler::ParameterCallbackHandle`
  stays as an alias of it. A public type has one layout on every platform
  (`check-cpp-capability-layout`), so it is declared everywhere. Upstream's
  `parameter_name` / `node_name` / `callback` members are held behind a
  pointer, as `ParameterEventCallbackHandle` holds its callback, and are not
  public members here.
- The `uint16_t` token is `rclcpp::OnSetParametersCallbackToken`. It could not
  take upstream's `OnSetParametersCallbackHandle` spelling, because that name is
  already the owning hosted handle (`node_interfaces::OnSetParametersCallbackHandle`,
  issue 2056). The token form's callers on `Node` and `LifecycleNode`, and two
  compile tests, moved to it. No alias keeps the old meaning: the name now
  means the event-handler handle. A `breaking` changelog fragment says so.
- Ledger: `cpp:ParameterCallbackHandle` goes from `gap` to `adopt-bounded`
  (hosted envelope); `cpp:OnSetParametersCallbackToken` is a new `extension`
  row. Four rows that described the token under the old name, or named
  rclcpp's on-set handle `ParameterCallbackHandle`, are corrected.
- Test: `parameter_event_handler_runtime.cpp` declares
  `rclcpp::ParameterCallbackHandle::SharedPtr abs = handler->add_parameter_callback(...)`
  and asserts that the nested name is the same type and is not integral
  (`just check cpp`).
