# Phase 503 -- Deadline-armed route monitor with a reaction hook

**Status (2026-10-15).** W1-W5 done. W1-W4 landed in PR #2264; W5 (the
model read, ros-launch-manifest pinned by rev 33577ff until its v0.1.61 tag)
and the end-to-end test in the follow-up.

**Continues:** phase10-B3, which added `route-latency-runtime` (a node path
judged from the take of its trigger input to the dispatch that publishes its
output).

## Why

`route-latency-runtime` is a completion verdict. The take of the trigger
stamps the route and the dispatch that publishes the output judges it, so a
route whose output never comes -- a handler that stopped publishing, a timer
that is never released, a callback that returns without publishing -- stays
pending forever and records nothing. A budget that is only checked when it is
met late cannot be supervised: a fallback has to start at the deadline, not at
an output that may never be produced.

A user who supervises a measured budget instead of proving it states the
expectation (the budget), and the runtime must catch the overrun and start a
fallback within a known time. On the executor that needs three things the
route rule does not have: a due time per pending route, a wake at that time,
and a hook through which the application starts its fallback. The existing
`DeadlineAction::Fault` hook (`Executor::set_fault_handler`) is Rust-only,
takes no context, and panics when unset, so a C or C++ image could not use
it.

## Design

### Row fields

`MonitorSpec` (and the C row `nros_cpp_monitor_row_t`, and the codegen rows)
carry three more fields, appended:

| field | meaning | default |
| --- | --- | --- |
| `route_deadline_ms` | supervised deadline from the take of the trigger | 0 = not supervised |
| `reaction_id` | the id the hook receives for the reaction the contract names | 0 = none |
| `mechanism` | 0 `executor` (checked in the spin loop), 1 `timer` (the port also arms its own timer) | 0 |

A row with `route_deadline_ms == 0` behaves exactly as before. A C or C++ row
baked before the fields existed leaves them zero by aggregate initialisation,
and the generated entries print them only on a supervised row, so an
unsupervised image's generated source is unchanged.

### The rule

`route-deadline-runtime` (rule code 11, appended to `RULE_IDS`):

1. `route_take` stores the take (unchanged). The due time is `take +
   route_deadline_ms`, derived from the stored take: both are the clock's low
   32 bits, exact across a wrap for any route shorter than ~71 minutes.
2. The executor offers the earliest due time of a pending supervised route as
   a deadline source of its own (`WakeSourceId::RouteDeadline`) in the same
   `min` as timers, the session and the platform sources, so the park ends at
   the deadline.
3. `run_contract_monitors` runs right after the park. A pending supervised
   route with `now >= due` is overdue: its take is dropped (the route stops
   pending, so the next take starts a new route), it is marked
   `ROUTE_VIOLATED`, the verdict is recorded once until a route is within its
   budget again (the latency rule's report-once rule, shared with
   `route_judge`), and the reaction hook is called -- on every overdue take,
   since each is a missed deadline the fallback has to cover.
4. Before the monitors are armed (phase-474 I2) the verdict is only counted
   and no reaction is requested.

`route-latency-runtime` is unchanged: a route answered after its budget but
before its deadline is still judged on completion, and an unsupervised route
is judged only that way.

Detection delay under `mechanism: executor`: the park granule after the
deadline (one tick on Zephyr), plus whatever dispatch is running when the
deadline passes. A callback that never returns holds the check too; that is
what `mechanism: timer` is for, and the executor exposes the due time for it
(`route_next_due_us`).

### The reaction hook

```c
typedef struct nros_cpp_reaction_event_t {
    uint32_t reaction_id;  /* the row's reaction_id; 0 = none named */
    uint32_t row;          /* row index in the installed table */
    uint32_t rule_code;    /* 11 route-deadline-runtime, 4 deadline-miss-runtime */
    uint32_t measured_ms;  /* take to check */
    uint32_t declared_ms;  /* the deadline */
    uint8_t  mechanism;    /* NROS_CPP_MECHANISM_EXECUTOR / _TIMER */
} nros_cpp_reaction_event_t;

typedef void (*nros_cpp_reaction_hook_t)(const nros_cpp_reaction_event_t *event, void *ctx);

nros_cpp_ret_t nros_cpp_executor_set_reaction_hook(void *executor,
                                                   nros_cpp_reaction_hook_t hook,
                                                   void *ctx);
nros_cpp_ret_t nros_cpp_executor_route_next_due_us(void *executor, uint64_t *out_us);
```

Rust: `Executor::set_reaction_hook(Option<(ReactionHookFn, *mut c_void)>)`,
`Executor::route_next_due_us()`, with `ReactionEvent` / `ReactionHookFn`
re-exported under `nros::monitor`.

The hook runs on the spin thread inside `spin_once`; it must not block and
must not re-enter the executor. It generalises the fault handler: on a sched
context's `DeadlineAction::Fault` with no `set_fault_handler` installed, the
reaction hook is called (reaction id 0, `row` = the sched-context slot)
instead of panicking. `set_fault_handler` keeps its meaning and precedence.

Registration: a generated C or C++ entry whose table has a supervised row
declares `nros_app_reaction_hook` as a weak symbol and, when the application
defines it, installs it right after `nros_cpp_install_monitors`. An
application that holds the executor handle can call
`nros_cpp_executor_set_reaction_hook` itself instead. The generated rows carry
a comment naming each supervised row's deadline, reaction id, reaction path
and mechanism.

### Reaction ids

Codegen numbers the distinct reaction path refs of the table's rows in sorted
order, from 1 (`contract_monitors::reaction_ids`). The id is opaque to the
executor; the comment in the generated entry maps it back to the path.

## Work items

| item | what | state |
| --- | --- | --- |
| W1 | `route_next_due_us`, `route_overdue`, rule code 11, unit tests (`monitor.rs`) | done |
| W2 | wake source, check after the park, reaction hook, fault fallback (`spin.rs`) | done |
| W3 | `nros_cpp_executor_set_reaction_hook`, `nros_cpp_executor_route_next_due_us`, row fields, header | done |
| W4 | IR / lowered rows, C and C++ entry templates, weak hook install, Rust render | done |
| W5 | fill the rows from the model's supervision (`apply_route_supervision`) | done |

W5: the model (ros-launch-manifest `33577ff`, play_launch phase 93's lowering)
carries `contracts.node_paths.<key>.supervise: { route_deadline_ms, reaction,
mechanism }` on an input-triggered node path. `monitor_rows` copies it onto
the route row of each output the path routes (the tightest route per output,
as before), and `apply_route_supervision` numbers the reactions. Refused: a
supervised path whose trigger input is on no topic (no route could watch it),
and a deadline that rounds to 0 ms (0 means "not supervised" on the row). A
model without `supervise:` bakes the same rows as before.

rlm is pinned by rev because the release carrying the key is not tagged yet;
`just bump-manifest` takes tags only, so the v0.1.61 tag replaces the rev with
it. `nros-launch-resolve` keeps following the play_launch submodule's own rlm
tag, so a model it resolves states `supervise:` once that submodule moves to a
play_launch that writes it.

## Gate

- `route_deadline_rule` (`monitor.rs`): due time as a wake value, nothing one
  microsecond early, the verdict at due + 1 ms, once per take, report-once
  until recovery, across the 32-bit wrap.
- `route_deadline_fires_when_the_output_never_comes` (`executor/tests.rs`):
  on the host executor with a set clock, the park bound is the deadline
  (`WakeSourceId::RouteDeadline`), nothing at due - 1 us, at due + 1 ms one
  `route-deadline-runtime` verdict and one hook call with reaction id 7,
  row 0, mechanism timer and the registered context; a second missed take
  calls the hook again and records nothing.
- The existing route tests (`route_rule`,
  `b3_a_late_tick_exceeds_the_route_while_every_callback_passes`) unchanged.
- Codegen: a supervised row renders its three fields, the comment and the
  weak-hook install in both packs; an unsupervised table renders as before.
- `a_supervised_path_bakes_its_deadline_reaction_and_mechanism`
  (`model_ingest.rs`): `supervise:` in the model reaches the row and the
  rendered Rust table; an unwired supervised path is refused.
- End to end on posix over zenohd (`contract_monitor_parity.rs`):
  `supervised_route_with_no_output_calls_the_reaction_hook` --
  `contract-monitor-pub` feeds the trigger at 20 Hz, `contract-monitor-route`
  takes it and never publishes its output; the hook is called on every missed
  take with `id=7 row=0 rule=11 ... declared_ms=150 mechanism=1`, and
  `route-deadline-runtime` is drained once. The unsupervised twin
  (`CM_ROUTE_DEADLINE_MS=0`) prints neither.

## Not in this phase

- The independent timer of `mechanism: timer` (an RTOS timer and a reaction
  thread above the spin thread). The executor exposes the due time; the port
  or the application arms the timer.
- A reaction run by the executor itself (publishing a fallback request from
  inside the check). The hook only reports which reaction is due.
