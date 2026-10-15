A supervised route (`route_deadline_ms`) answered at or after its deadline is
now a deadline miss too: `route-deadline-runtime` is recorded and the reaction
hook is called at completion, once per take. Before, a single long callback
that took the trigger and published the output late was caught only by
`route-latency-runtime`, because the executor's overdue check cannot run while
the callback holds the spin thread, and the hook never ran. Under
`mechanism: executor` that completion is the only detection such a callback
gets, late by its overrun; `mechanism: timer` is what reacts at the deadline.
