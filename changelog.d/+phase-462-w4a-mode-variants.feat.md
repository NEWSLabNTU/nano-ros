A contract's operational modes now reach a Rust image. Each mode in
`contracts.modes` is baked as a variant of the executor's monitor tables, with
the same monitored endpoints and the mode's own rates, latencies and ages. Call
`nros::monitor::request_mode(NROS_MODE_<NAME>)` to switch every executor at its
next spin. A mode that would add or remove a monitored endpoint is refused at
bake time, and so is an override naming an entity the base contract does not
declare. An override that no on-target monitor observes (a scope path's budget,
a topic's rate) is listed as not lowered by `nros codegen-system`. C and C++ entries follow.
