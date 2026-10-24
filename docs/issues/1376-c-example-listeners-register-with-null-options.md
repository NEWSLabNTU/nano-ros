---
id: 1376
title: "Five of the six rclc subscription entry points pass a receive hint of 0
  although the subscription carries its type's bound, so they take the
  `c_raw_no_hint` row while the sizing descriptor credits the image with the
  typed hint"
status: open
type: bug
area: [api, examples, sizing]
related: [1319, 0896, 0964, phase-456, phase-454, phase-394, phase-460, phase-492]
---

**Home: [phase-460](../roadmap/phase-460-formal-checks-on-the-contract-chain.md)** — re-homed 2026-10-23 from phase-394, archived that day. The descriptor credits a hint the image does not supply, which is a trusted fact nobody checks: that phase's class.

## Re-checked 2026-10-24: the seven sites are gone, and the class moved into the API

The design below was written against `nros_cpp_subscription_register(..., NULL)`
call sites. None remains. Phase-492 (`43ed911c08`, 2026-10-13) moved every
listener in the table onto rclc's own calls, and none of the seven files names
`nros_cpp_subscription_register` now:

```c
rcl_ret_t rc = rclc_subscription_init_default(&self->sub, node,
                                              ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                 ON_NEW_DATA);
```

That path supplies the hint. `rclc_subscription_init_*` keeps the type
descriptor on `rcl_subscription_t.type_support`. The generated descriptor sets
`serialized_size_max` to the type's `__RX_MAX_SERIALIZED_SIZE`
(`packages/cli/rosidl-codegen/packs/c/message.c.jinja:19`). And
`rclc_executor_add_subscription_with_context` forwards it as `rx_bytes`
(`packages/api/nros-c/src/executor.rs:2317`). So the seven images register on
the typed row, which is the row the descriptor credits them with. The fix this
issue planned (seven consumer edits onto `nros_cpp_subscription_register_hinted`)
is obsolete. That function no longer exists either.

**What survives is the same defect, one layer down.** The other five rclc entry
points take the same `rcl_subscription_t`, whose descriptor holds the same
bound, and still pass a literal `0`:

| entry point (`packages/api/nros-c/src/executor.rs`) | hint passed |
| --- | --- |
| `rclc_executor_add_subscription_with_context` (:2293) | `type_support.serialized_size_max` |
| `rclc_executor_add_subscription_raw_validated` (:2203) | `0` |
| `rclc_executor_add_subscription_raw` (:2373) | `0` ("this path states no hint") |
| `rclc_executor_add_subscription_raw_with_info` (:2428) | `0` ("predates the options struct") |
| `rclc_executor_add_subscription_with_info` (:2537) | `0` |
| `rclc_executor_add_subscription_in_group` (:2742) | `0` |

The comments on those zeros date from before phase-492, when a raw registration
had no type bound to hand. Now it has one. One example still takes such a path:
`examples/workspaces/safety/src/c_safety_listener_pkg/src/SafetyListener.c:84`
(`_raw_validated`). User code reaching any of the five has the same problem, and
the descriptor still cannot see which entry point a call site uses.

### Revised remedy

The fix is in the API, not the examples. One helper reads the bound off the
subscription:

```rust
// 0 = the type states no bound (or the subscription was built from names
// alone): the image default applies, never a claim of zero bytes.
unsafe fn subscription_rx_bound(sub: *const rcl_subscription_t) -> usize {
    (*sub).type_support.as_ref().map_or(0, |t| t.serialized_size_max)
}
```

All six entry points call it, and `with_context` stops reading the field
inline. Then no C caller can pick the no-hint row by choosing an entry point.
The only way onto that row is a descriptor with no bound, which is a fact the
type states, not a choice the call site makes. This changes no signature and no
example source. The safety listener's image changes size because it then
registers on the typed row.

### Revised acceptance

* All six rclc subscription entry points forward `subscription_rx_bound`, and
  none passes a literal `0`.
* A gate keeps it that way. It could be a C arm of
  `check-cpp-subscription-bound-supplied`, or a sibling gate that refuses a
  literal `0` hint in an `add_arena_subscription_c_*` call in
  `packages/api/nros-c/src/`. A selftest proves it fires.
* Before and after arena measurement on the safety listener's entry, the way
  phase-454 W5 measured `contract-monitor-sub`.

## As filed (2026-09-24, superseded by the section above)

### Symptom

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

### Why it matters beyond the bytes

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

### The sites, measured

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

### The remedy

`nros_cpp_subscription_register_hinted` (`packages/api/nros-c/include/nros/component.h:267`)
is the same call with the hint as a scalar parameter, and it already takes the
QoS as an argument — so all seven convert, including the two that pass a custom
profile, which the generated `{Msg}_subscribe` macro could not serve because it
hardcodes `nros_c_qos_default()`.

The number is the type's own `{PREFIX}_RX_MAX_SERIALIZED_SIZE`, emitted beside
the type by `packages/cli/rosidl-codegen/packs/c/message.h.jinja`, which is what
`{Msg}_subscribe` passes.

### Why it was not fixed with phase-456 W7

W7 made the bound non-optional at every C++ registration site in the nros-cpp
headers, and `check-cpp-subscription-bound-supplied` keeps it that way. These
seven are CONSUMER files reaching the C API, whose own helper already requires
the hint — the API half of the C side has no defect. Converting them touches
seven example leaves in four workspaces and re-stales their fixtures, so
acceptance here is a fixture BUILD and a re-measure, not a header edit, and it
belongs in its own change rather than inside a C++ API work item.

### Acceptance

* No C example registers a subscription with `options = NULL` while the type is
  in scope.
* The gate that covers the C++ headers grows a C arm, or a sibling gate covers
  the C call sites, so the class cannot come back silently.
* A before/after arena measurement on one converted entry, the way phase-454 W5
  measured `contract-monitor-sub`.
