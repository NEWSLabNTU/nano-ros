---
id: 2114
title: "`rclcpp::ParameterCallbackHandle` is the on-set token (`uint16_t`), not upstream's event-handler handle"
status: open
type: bug
area: [cpp, api-parity, parameters]
severity: low
found: 2026-10-27
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
