C timers take rclc's and rcl's shape: `rclc_timer_init_default(timer, support,
timeout_ns, callback)`, `rcl_timer_init(timer, clock, context, period, callback,
allocator)` and their Jazzy `2` forms, with rcl's callback
`void (*)(rcl_timer_t *, int64_t last_call_time)`. A ported rclc timer now
compiles unchanged. `rclc_timer_init_with_context`, `rclc_timer_init_on_clock`
(and their `2` forms) and the context-carrying callback are removed; a callback
reaches its state through the timer it is handed, with
`RCLC_CONTAINER_OF(timer, my_t, timer)` from `<rclc/timer.h>`.
`rclc_timer_init_with_info` and its callback no longer take a context either.
`support` and `context` may be NULL.
