The zenoh read task can be given a frame budget. Set
`NROS_ZENOH_RX_TASK_BUDGET_FRAMES` (Kconfig `CONFIG_NROS_ZENOH_RX_TASK_BUDGET_FRAMES`,
or `[knobs.zenoh.wire] rx_task_budget_frames` in a board or platform TOML) and
the read task processes at most that many frames back to back, then sleeps
`NROS_ZENOH_RX_TASK_BUDGET_REST_US` (default 1000), so a message flood can no
longer hold the CPU and stall the tiers below the transport. Unset (or -1) keeps
today's unbounded task, and 0 is refused. RFC-0074 has the sizing rule.
