C API: a subscription the executor dispatches no longer carries its own
subscriber storage, callback or context — the executor arena owns them.
`nros_subscription_t` is 744 bytes instead of 2536 on x86_64 (700 instead of
2368 on Cortex-M3). What a C caller changes:

- `nros_subscription_init_with_qos(sub, node, type, topic, qos)` and
  `nros_subscription_init_with_options(sub, node, type, topic, qos, options)`
  no longer take `callback` and `context`. Pass them at registration instead.
- `nros_executor_add_subscription(exec, sub, invocation)` is gone. Use
  `nros_executor_add_subscription_raw(exec, sub, callback, context, invocation)`
  (CDR bytes) or `nros_executor_add_subscription_typed` (a deserialised
  message, which the generated `<Msg>_executor_add_subscription` macro wraps).
- `nros_executor_add_subscription_in_group` takes `callback` and `context`
  after the subscription.
- A subscription you drain yourself is the new type `nros_polling_subscription_t`,
  and its functions are `nros_polling_subscription_*`:
  `nros_subscription_init_polling[_with_qos]` became
  `nros_polling_subscription_init[_with_qos]`, and `take_serialized`,
  `take_sequence`, `take_validated`, `borrow`, `release`, `set_wake_callback`,
  `get_actual_qos` and `fini` moved the same way.
