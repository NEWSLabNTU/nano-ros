---
id: 1376
title: "Seven C example listeners register with `options = NULL`, so they take
  the `c_raw_no_hint` row while the sizing descriptor credits their image with a
  supplied hint"
status: open
type: bug
area: [api, examples, sizing]
related: [1319, 0896, 0964, phase-456, phase-454]
---

## Symptom

Every C example that subscribes does it like this:

```c
int32_t rc =
    nros_cpp_subscription_register(node, "/chatter", std_msgs_msg_int32_get_type_name(), "",
                                   nros_c_qos_default(), on_raw, self, &handle,
                                   /*options=*/NULL); /* phase-402: NULL = defaults */
```

`NULL` options means `rx_buffer_hint = 0`, which is issue 1319's `c_raw_no_hint`
registration row: the arena slot is priced at the executor's closure buffer
(`RX_BUF`) rather than at the message's own bound.

The message type is right there in the call, one token away from the bound.

## Why it matters beyond the bytes

The sizing descriptor cannot see which row a call site takes. Its own comment
says so:

> A C/C++ entry that registers typed supplies `rx_size_bound<M>`; the raw
> no-hint row is a property of an individual call site, not of the image, and
> nothing this writer reads distinguishes them. The typed hint is therefore what
> a C/C++ entry is CREDITED with.

So each of these images is credited with `c_typed_hint` and registers as
`c_raw_no_hint`. That is a mis-size in the UNDER direction for the descriptor's
consumers, and it is exactly the latent defect phase-454 W5 documented rather
than hid.

## The sites, measured

Seven, all consumer code, none in the API:

| file | QoS it passes |
| --- | --- |
| `examples/templates/pure-c-workspace/src/c_listener_pkg/src/Listener.c:43` | `nros_c_qos_default()` |
| `examples/workspaces/c/src/listener_pkg/src/Listener.c:46` | `nros_c_qos_default()` |
| `examples/workspaces/features/src/c_qos_listener_pkg/src/QosListener.c:62` | `qos_profile()` |
| `examples/workspaces/features/src/c_reading_listener_pkg/src/ReadingListener.c:41` | `nros_c_qos_default()` |
| `examples/workspaces/features/src/mixed_qos_listener_pkg/src/QosListener.c:62` | `qos_profile()` |
| `examples/workspaces/features/src/mixed_reading_listener_pkg/src/ReadingListener.c:41` | `nros_c_qos_default()` |
| `examples/zephyr/c/listener/src/Listener.c:44` | `nros_c_qos_default()` |

## The remedy

`nros_cpp_subscription_register_hinted` (`packages/api/nros-c/include/nros/component.h:267`)
is the same call with the hint as a scalar parameter, and it already takes the
QoS as an argument — so all seven convert, including the two that pass a custom
profile, which the generated `{Msg}_subscribe` macro could not serve because it
hardcodes `nros_c_qos_default()`.

The number is the type's own `{PREFIX}_RX_MAX_SERIALIZED_SIZE`, emitted beside
the type by `packages/cli/rosidl-codegen/packs/c/message.h.jinja`, which is what
`{Msg}_subscribe` passes.

## Why it was not fixed with phase-456 W7

W7 made the bound non-optional at every C++ registration site in the nros-cpp
headers, and `check-cpp-subscription-bound-supplied` keeps it that way. These
seven are CONSUMER files reaching the C API, whose own helper already requires
the hint — the API half of the C side has no defect. Converting them touches
seven example leaves in four workspaces and re-stales their fixtures, so
acceptance here is a fixture BUILD and a re-measure, not a header edit, and it
belongs in its own change rather than inside a C++ API work item.

## Acceptance

* No C example registers a subscription with `options = NULL` while the type is
  in scope.
* The gate that covers the C++ headers grows a C arm, or a sibling gate covers
  the C call sites, so the class cannot come back silently.
* A before/after arena measurement on one converted entry, the way phase-454 W5
  measured `contract-monitor-sub`.
