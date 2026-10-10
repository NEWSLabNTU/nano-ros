C API: `nros_timer_t` no longer has a `last_call_time_ns` field. Nothing
updated it after a timer was added to an executor, so it was never the time
of the last call; read `nros_timer_get_time_since_last_call` or
`rcl_timer_get_next_call_time`, which ask the executor.
