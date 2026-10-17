The zenoh read-task budget is now derived from the contract. Declare
`ingress: { rate_hz: 200, burst: 4 }` on a subscriber endpoint
(`nodes.<n>.sub.<e>.ingress`, ros-launch-manifest v0.1.64) and the build sets
`NROS_ZENOH_RX_TASK_BUDGET_FRAMES` to the burst and
`NROS_ZENOH_RX_TASK_BUDGET_REST_US` to `burst x 10^6 / rate_hz`, from the
subscriber with the largest `rate_hz x burst`. A stated knob still wins, and an
image that declares no ingress keeps an unbounded read task and is unchanged.
`CONFIG_NROS_ZENOH_RX_TASK_BUDGET_REST_US` now defaults to -1 ("derive"), with
1000 us still the builtin. The per-frame board cost is taken as 0 for now, so
the task admits slightly less than the declared rate (RFC-0074).
