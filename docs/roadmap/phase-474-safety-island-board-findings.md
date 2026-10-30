# Phase 474 -- what the safety island found on the S32K344: violations nobody can see, monitors armed too early

**Status (2026-10-06). IN PROGRESS.** D5 had already landed before this
phase was written (issue 1567, see D5). D1, D2, I1 and I2 landed on branch
`feat/violation-ring-and-arming` (issues 1714, 1715); T4 landed as a host
executor test and a native_sim fixture; its QEMU run is open, and its board run
moved to [phase-512](phase-512-on-hardware-measurements.md) H6 on 2026-10-24. **2026-10-26:** the
board halves of T4, I6, I8 and I9 are met by the island's board rounds on nano-ros
8b95e90dd6 and 5feeb32a7b (section "The board halves, 2026-10-26" below). Every other item is
open. Records the open work the Autoware safety island's RTSS@Work 2026 demo
(simple-autoware-safety-island, phase 8) found in nano-ros while running its
three acts on an NXP S32K344 (MR-CANHUBK344) behind a 921,600-baud UART and an
island gateway router. Nothing below was measured for this document; every
number is quoted from the island's documents named under **Source**.

**Prior:** RFC-0052 W3b (the contract monitors and `drain_violations`), issue
0514 (violations were never drained; resolved by logging at detection),
phase-462 W2 (`silence-runtime`), phase-473 (graph discovery off, latched
reads), issues 1533, 1534, 1567, 1574.

**Source:** in simple-autoware-safety-island at main `73e6b6d`:
`docs/takeover-trace.md` section 10 ("Follow-ups" F1-F5) and section 11
(including "Runtime contract violations"); `docs/roadmap/phase-8-rtss-work-demo.md`,
the "Open:" notes of W1, W2, W8, W8a, W14, W17, W24, W26, W30 and W31;
`docs/boot-through.md` (phase8-W1, W14, W15, W26); `docs/serial-link.md`
section 11 (phase8-W10); `docs/tracing.md` (phase8-W17).

---

## Why

The island ran 13 board acts in phase8-W8 and 9 fresh ones in phase8-W31, all
PASS, on an image whose generated entry bakes `max-latency-runtime` monitors
at 206 ms on `mrm_state` and `takeover_request_state`. After those runs the
island could not say whether any runtime monitor had fired during an act:

- the console the monitor writes to (`log_violation`, `log_warn`) is lpuart0,
  which is not wired on the board; lpuart2 carries the zenoh link;
- the executor's violation ring, read over SWD at bring-up (`w31-bringup`),
  was already full: 8 of 8 slots, all start-up entries, so a later violation
  is not stored;
- no trace marker records a violation.

The island's only evidence is offline: the trace's ENTRY/EXIT brackets show
the longest handler callback that published a monitored topic was 6.86-24.93
ms per run, far below 206. That is a measurement the island had to make
around nano-ros, not one nano-ros reported. A contract monitor whose verdict
cannot leave the board is not yet a safety measurement.

The same runs exposed what the monitors measure (one callback, not the
route the contract charges), monitors that judge start-up as if it were steady
state, and sizing figures (heap, frame_id bounds) that the derivation and the
board disagree on.

## Design

### D1 -- violation reporting on a board without a console

What the tree does today (`packages/core/nros-node/src/executor/`):
`run_contract_monitors` (spin.rs) calls `log_violation` (monitor.rs, a
`log_warn` of "contract violation: <rule> <fqn> measured=... declared=...")
when `report_violations` is set, then pushes the violation into a
`heapless::Vec` of `MAX_VIOLATIONS` = 8. When the Vec is full the push fails
and `monitor_violations_dropped` is incremented, so the ring keeps the FIRST
8 violations since boot. `Executor::drain_violations` empties it, and
`nros_cpp_executor_drain_violations` mirrors it over the C++ FFI; the only
caller of either outside unit tests is the test binary
`packages/testing/nros-tests/bins/contract-monitor-cpp`. The generated Zephyr
entry does not drain it. `violations_dropped()` exists and nothing reports it.

On the island board the result was a ring full of start-up entries and a
console nobody reads. Options to decide between (more than one may land):

1. **SWD drain** -- a fixed-layout, versioned record (like the boot report)
   holding the ring, the dropped count and a sequence number, readable by name
   over a debugger. Cheapest; needs no link; only useful on a bench.
2. **A trace marker per violation** -- rule id, endpoint index, measured and
   declared, written into the image's trace stream when tracing is compiled
   in. Puts the violation on the same clock as the callbacks it judges.
3. **A violations topic** -- a publisher the entry declares (for instance a
   `diagnostic_msgs/DiagnosticArray`, or a compact message), fed by the
   drain. Reaches the host over whatever link the image has; costs an entity
   and link bandwidth, and must not itself be able to starve the link.

Ring policy, whichever channel is chosen: keep the LATEST N, not the first N,
and keep the dropped counter beside it, so a reader sees the most recent
violations and knows how many it missed. Decide whether the entry glue drains
the ring every spin (and into which channel) or whether the ring is the
channel.

**Decided (2026-10-06, I1).** Options 1 and 2 both land; option 3 is
already issue 1635's `/diagnostics` sink and is unchanged. The ring keeps the
latest N with a dropped and a total count. Every stored verdict goes, at
detection, to the SWD record `NROS_VIOLATION_RECORD` (option 1; versioned,
all-`u32`, present with `CONFIG_NROS_BOOT_REPORT`, never drained) and to four
trace markers (option 2; with `CONFIG_NROS_TRACE_CALLBACKS`). The ring stays
for a drain; an opt-in drain-and-report hook
(`CONFIG_NROS_VIOLATION_DRAIN_REPORT`) drains it every spin into the log. The
hook is a build-time executor default rather than template code, for the
reason issue 1635 gave for its sink: the runtime owns every board's spin loop,
so one default reaches the C, C++ and Rust entries.

### D2 -- arm the monitors only after start-up

The 8 entries the W31 bring-up found, all before any act:

| count | rule | endpoint | detail |
| --- | --- | --- | --- |
| 4 | `timer-overrun-runtime` | `timer` | |
| 1 | `release-jitter-runtime` | `spin` | measured=57751 declared=10000 |
| 1 | `silence-runtime` | `/mrm_handler/operation_mode_availability` | declared=500 |
| 2 | `rate-hierarchy-runtime` | the comfortable-stop operator's `clear_velocity_limit` and `max_velocity_candidates` | on-demand topics with a 10 Hz `min_rate_hz` |

None of these is a fault of the running system: registration, the first
join and inputs that have not started yet all look like overruns, jitter and
silence. (The two rate entries are also a contract question -- an on-demand
topic has no rate to keep -- which is the play_launch phase's on-demand-topic
key; see Cross-repo.)

Proposal: a monitor is armed only once the application says its start-up is
over. The island already has the event: its `mrm_handler` runs an INIT/RUN
state machine (phase8-W27), and INIT ends only when every input is
established, not merely heard (phase8-W28). Options:

1. an API the application calls (`arm_monitors()`, or a per-node "running"
   flag) when it reaches its own RUN;
2. a grace period stated in the contract or derived from it (for example the
   largest `max_age` / silence bound after FirstSpin);
3. both: arm at the API call, and arm anyway at the grace deadline so an
   application that never calls it is still monitored.

Violations before arming are counted separately, not discarded, so start-up
trouble stays visible.

**Decided (2026-10-06, I2).** Option 3, from Kconfig and not the contract
(rlm has no key, and inventing one is the play_launch phase's business): the
default stays "armed at the first spin"; `CONFIG_NROS_MONITOR_ARM_ON_CALL`
waits for the application's call, and `CONFIG_NROS_MONITOR_ARM_GRACE_MS` arms
anyway that long after the first spin (0 = no deadline). Arming is per
executor, triggered image-wide by the application's call. Pre-arm verdicts
are counted in `suppressed_before_arm` (executor and SWD record).

### D3 -- the route, not the callback

`max-latency-runtime` measures one dispatch's elapsed time and charges it to
each monitored publisher whose publish count advanced in that dispatch
(`attribute_latency` in `executor/spin.rs`). On the island that bounds the
handler's work inside one callback (longest 24.93 ms in the W31 runs, r03's
EMERGENCY_STOP tick). The contract's 206 ms `call_mrm` is a route: the serial
link (57 ms stated, 67.02 seen), the wait for the handler's 100 ms tick (118
stated), and the work in the tick (31 stated). The runtime monitor therefore
cannot fail on the case the contract budgets: a 206 ms route made of a slow
link and a full tick wait passes a callback-level check with 180 ms to spare.

Decide what a route-level runtime check is in nano-ros: which stamp starts it
(the sample's source stamp, the take, or the trigger edge), where the clock
domains differ (the link term is cross-clock; the island stamps it against
the host offline), and which monitor reports it. This pairs with the
play_launch phase that works on the same findings (F1, transport charged on
the reaction walk): the static walk and the runtime check should charge the
same terms.

### D4 -- a deadline is not a monitor (F3, the nano-ros half)

A node-path `max_latency` becomes BOTH the node's derived deadline (the
realizer's `max_latency_ms`, `nros-orchestration-ir` `rtos_realizer.rs`) and
a `max-latency-runtime` monitor row in the generated entry. So a budget
cannot be stated where a cost happens without changing scheduling: the
comfortable-stop operator serves `operate` after the handler's tick and
publishes the velocity limit 0.85-1.18 ms after the handler returns
(takeover-trace section 10), but a `max_latency` on that path would become a
2 ms runtime monitor and a 2 ms deadline in the image. The island states no
number there and folds the cost into `call_mrm`'s work term.

Decide how a contract says "this is a budget to check" separately from "this
is a deadline to schedule by" -- a separate key, or a rule for which
`max_latency` the realizer may take -- so the two can be stated at different
places. The rlm/play_launch half of F3 (a service edge has no transport or
queueing key) is the play_launch phase's.

### D5 -- skip `externals` publishers when deriving the image (DONE)

phase8-W8a found nano-ros composing the contract's external
`/availability_gate/availability` publisher into the image's entity
inventory: the transient-local count was refused, queryables derived to 2,
and the comfortable-stop operator failed `create_publisher_in (code=-3)` on
QEMU. The island worked around it with `CONFIG_NROS_MAX_QUERYABLES=4`.

Resolved in nano-ros by issue 1567 (`da272e419`: an endpoint on an external
side of a topic whose node is not in `structure.nodes` is skipped), with its
"not fixed here" residue in issue 1572 (`00fbcb405`: a refused
transient-local count sizes for the worst case). The island moved its pin to
`da272e419` in phase8-W14 and retired the workaround (phase8-W13; the table
derives to 4). Kept here only so the island's open note has an answer; no
work remains.

## Implementation

### I1 -- the violation channel and the ring policy

Implement D1's decision: the ring keeps the latest `MAX_VIOLATIONS`, the
dropped counter is reported with it, and the generated Zephyr entry drains
the ring into the chosen channel (SWD record, trace marker, topic). Whatever
the channel, `violations_dropped()` stops being a number nothing reads.

**Result (2026-10-06).** Landed as decided under D1. The ring
(`monitor::ViolationChannel`, `executor/monitor.rs`) evicts its oldest entry
when full, numbers entries per executor and counts evictions. Knobs:
`NROS_EXECUTOR_MAX_VIOLATIONS` (default 8: 24 B a ring slot, 28 B a record
slot on a 32-bit target, so 192 B per executor plus 40 + 224 B for the record)
and `NROS_VIOLATION_DRAIN_REPORT` (default off). SWD layout, version 1, all
little-endian `u32`: header `magic "NRVR" (0x4e525652), version, capacity,
slot_words (7), total, head, dropped, suppressed_before_arm, armed, reserved`,
then `capacity` slots of `seq (1-based, 0 = empty, written last), rule code,
fqn FNV-1a, measured, declared, fqn_addr, fqn_len`; newest entry in slot
`(head + capacity - 1) % capacity`. Rule codes are `monitor::RULE_IDS` index +
1 (append only). Trace markers: 21 `seq << 8 | rule code`, 22 fqn hash, 23
measured, 24 declared. C++ FFI: `nros_cpp_executor_set_violation_drain_report`,
`nros_cpp_executor_violation_counts`.

For the island's `w31-logs/tools/vscan.py`: its heuristic still finds ring
entries (a `Violation` is unchanged: rule `&str`, fqn `&str`, measured,
declared, 24 B), but the ring is now a rolling window and can be drained, and
the "FULL: later violations are dropped" note no longer holds. Read
`NROS_VIOLATION_RECORD` by symbol instead
(`read-violation-record.py --addr-only`, then decode the dump): it needs no
scan. The island's marker ids 1-31 overlap nano-ros's 16-24 if it installs
`nros_set_trace_sink` into its own tracer; it must offset one block.

### I2 -- arming

Implement D2's decision in the executor (an armed flag per monitor table, or
per node) and in the generated entry (where the grace deadline is baked).
Pre-arm violations go to a separate counter.

**Result (2026-10-06).** Landed as decided under D2. API:
`nros_monitors_arm()` (C), `nros::arm_monitors()` / `nros_cpp_monitors_arm()`
(C++), `nros::monitor::request_monitor_arming()`, `Executor::arm_monitors`,
`set_monitor_arming`, `monitors_armed`, `violations_suppressed_before_arm`,
`nros_cpp_executor_set_monitor_arming`. Arming reopens every rate/age/silence
window, discards latency/age measured before it, clears the release-jitter
statistics, and re-reports a stack low-water mark already past its minimum
(stack depth reached at start-up is a real fault). The generated C/C++
entries' monitor-install comment states the knobs. For the island: set
`CONFIG_NROS_MONITOR_ARM_ON_CALL=y` (and a grace backstop) and call
`nros::arm_monitors()` where the handler logs `INIT_DONE`.

### I3 -- trace hooks the island's measurements need (F4)

The island's trace keeps no per-sample take marker for `kinematic_state`,
`operation_mode_state` and `control_cmd`, so the link hop of those inputs is
not measured; and the emergency operator's 30 Hz timer ticks are kept one in
ten, so its tick jitter is not measured (takeover-trace section 10, F4). The
island's marker table and sampling policy are generated by the island's own
tool from its contract (`src/safety_island_tracing/gen_markers.py`); nano-ros
owns the dispatch-level hooks (`executor/callback_trace.rs`, register /
start / end per dispatch, `trace-callbacks` feature). The nano-ros part:

- a per-sample take hook that carries the sample's identity and, where the
  message has one, its source stamp, so an image can trace takes of chosen
  subscriptions without hand-placed markers;
- a configurable sampling rate for timer ticks (every tick, 1 in N, or on
  change), per timer, so a 30 Hz tick can be traced in full when its jitter
  is the measurement.

**Result (2026-10-07), issue 1730.** Marker ids appended to the block,
16-24 unchanged: 25 take (`handle << 24 | take seq`, emitted before the
callback's start 18), 26 take stamp sec, 27 take stamp nanosec (after a 25,
when the stamp's place is known). Takes are emitted at all 19 subscription
dispatch sites; opt-in per slot (`callback_trace::set_take_trace`, C
`nros_trace_set_take(handle, on, stamp_offset)`) or image-wide
(`CONFIG_NROS_TRACE_TAKES`). The stamp comes from the type's
`STAMP_OFFSET` on typed Rust paths and from the slot's stated offset on C/C++
paths (4 for a type that starts with a `Header` or a `Time stamp`). Timer
ticks: one in N keeps its 18/19 pair, per slot (`set_timer_trace_every`, C
`nros_trace_set_timer_every`) or image-wide (`CONFIG_NROS_TRACE_TIMER_EVERY`,
default 1, 0 = none); other dispatches are always traced. Host tests in
`nros-node` (`trace-callbacks`). For the island: `CONFIG_NROS_TRACE_TAKES=y`
or, per input, `nros_trace_set_take(<slot of kinematic_state /
operation_mode_state / control_cmd>, true, 4)` after registration (the slot is
the handle the register event names); keep the 30 Hz operator whole and thin
the rest with `CONFIG_NROS_TRACE_TIMER_EVERY=10` plus
`nros_trace_set_timer_every(<operator slot>, 1)`; forward 25-27 at 256 + id
(281-283) beside 277-280. Not run on the board.

### I4 -- a bounded `frame_id` for the host metadata probe

`nros sync` prints "no producer" for the source metadata of the island's
four components (phase8-W26, still as before; `docs/boot-through.md`
phase8-W1). Two causes, per the island: two probes build and then halt at
`declare_parameter (code=-16)` because capabilities are not lowered into the
probe (issue 0543, since resolved; the island saw it on an older pin), and
two fail to build on the host on an unbounded `header.frame_id` (Odometry,
VelocityReport). The image's tables come from the launch model, so the image
is not affected; the probe is. Give the probe a bound for `frame_id` (from
the contract or the type-bound sizing of phase-403) so it can build these
components, and re-run on the current pin to confirm the capability half is
gone. Related: issue 0939 (the probe links the node name, not a target).

**Result (2026-10-07).** No code change: the way to bound `frame_id` for
the probe exists on main. Issue 1470 (resolved 2026-09-27) made the CMake
lane discover `nros-codegen.toml` field caps from the SOURCE tree, ancestor
to descendant, so a cap at the WORKSPACE ROOT (`<ws>/nros-codegen.toml`, or
`<ws>/src/nros-codegen.toml`) reaches every package the probe compiles. Its
gate, `just check probe-workspace-caps`, builds a workspace type that embeds
`std_msgs/Header` (the shape of `VelocityReport`) and a node that subscribes
to it; re-run on this branch: 5 of 5 checks pass (uncapped: the node's TU
refuses on `NROS_UNBOUNDED__..._field_header_frame_id`; capped at the root:
`RX_MAX_SERIALIZED_SIZE = 104` and the TU compiles). The capability half is
issue 0543 (resolved 2026-08-13: the probe includes `nros config show
--format cmake` for a single-bringup workspace). Why the island still sees
"no producer": its caps live in `src/island_interfaces/nros-codegen.toml`, a
PACKAGE scope that is not an ancestor of the component packages, so the
probe of `mrm_handler` / the operators never reads them. For the island:
move the `[fields]` caps (`"std_msgs/Header.frame_id"`,
`"nav_msgs/Odometry.child_frame_id"`) and `[defaults]` to a workspace-root
`nros-codegen.toml` and re-run the island's sync recipe; if a probe still halts at
`declare_parameter (code=-16)` (`Unsupported`), that is a separate gap (a
census run without the parameter store the launch seeds) to file with the
probe log -- it was not reproduced here, because running the island's tree
was out of this branch's reach.

### I5 -- the heap: one derivation the board and the report agree on

Three numbers, from three different images; they must not be compared as if
they were one:

| figure | image | where |
| --- | --- | --- |
| HEAP HEADROOM REFUSED, "set `CONFIG_NROS_ZEPHYR_HEAP_SIZE` >= 133952" (peak 109,376 at capacity 123,392, floor 24,576) | phase8-W1 QEMU image: 4 nodes, parameter store, heap 122,880 | boot-through.md phase8-W1 |
| FirstSpin peak 77,160 of capacity 102,912 (heap configured 102,400), headroom ok, 25,752 spare | phase8-W31 board image (3 nodes), joined to the container Autoware through the gateway | takeover-trace.md section 11 |
| `HEAP EXHAUSTED` in `_z_slice_init` from `zpico_read`, 7.3-8.0 s after boot when Autoware joined, at 102,400; the runs then used 1 MiB | phase8-W17 QEMU image over TCP, straight into the stock router and behind the gateway (runs w17-qb1, w17-qb2) | tracing.md phase8-W17 |

So the 133,952 ask is stale for the board image (phase8-W8a shrank the image;
its QEMU peaks were 74,856-76,080 and the heap went to 102,400). The live
problem is the third row: the headroom check judges the peak at FirstSpin,
and on QEMU the heap ran out after FirstSpin, in the read path, when a large
host graph started sending. The island's explanation is that over TCP nothing
paces Autoware's traffic the way the board's UART does.

Work: (a) reproduce the third row on the current pin with graph discovery off
(phase-473 W1), to separate a graph burst from sample traffic; (b) make the
heap the image derives cover the read path's worst case for the image's
subscriptions and link (or bound `zpico_read`'s allocation so it cannot grow
with the remote graph), so the configured heap and the derived one agree and
both hold after FirstSpin; (c) say in the boot report which figure the
headroom line judged (FirstSpin peak or running peak), and why capacity reads
102,912 for a configured 102,400.

**Result (2026-10-07), issue 1728.** The three figures were read, not
reconciled by a new derivation, because nano-ros has none for the heap
(issue 1424 chose "checked, not derived"): the 133,952 ask is
`read-boot-report.py`'s `peak + floor` on the phase8-W1 QEMU image (4 nodes,
parameter store), a measurement of a different image; the Kconfig default
already reads the image's own declarations (`NROS_CAPABILITY_PARAM_SERVICES`,
`NROS_PARAM_STORE`, `RUST`), which for the 3-node C++ board image without
parameter services is 65,536. The W4 "HEAP HEADROOM REFUSED" is the fixed
24,576-byte floor: 102,912 - 79,712 = 23,200 spare. The record's peak is a
high-water the heap updates on every allocation, so a dump always judges
the RUNNING peak as of the dump; capacity is the knob plus zpico-alloc's
8 x 64 B slab (102,400 reads 102,912). Landed: the boot record (layout 11)
keeps `heap_peak_at_first_spin` and the verdict says which peak it judged
and how much came after the first spin; the knob it names is
`peak + floor - 512`; and a stated value of a DERIVABLE knob below the
image's derivation is a configure WARNING (above or equal: a STATUS line;
phase-478 D2, which nobody had implemented). Not done: a QEMU heap read
after the host graph joins (guest RAM, no monitor socket on the west run
lane), and (a)/(b) above (reproducing phase8-W17's read-path exhaustion with
graph discovery off; bounding `zpico_read`'s allocation) -- both need the
island's QEMU image under a host Autoware. For the island: from the W4 dump,
`CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 103776`; set 104448 (102 KiB) and re-read the
dump after an act on the new pin.

## Test / check

### T1 -- the island dropped its zenoh session when a host peer joined a plain router

phase8-W8a recorded it on QEMU at any heap. phase8-W15 found the cause and it
is configuration, not version: zenoh-pico measures router silence against
min(router lease, own lease) = 10 s, while a stock `rmw_zenohd` (`lease:
60000`, `keep_alive: 2`) keepalives an idle link every 30 s; the CLOSE (reason
5, EXPIRED) is decided at about OPEN+20 s and reaches the wire with the next
byte the router sends, which is why a host peer's join looked like the
trigger. Fixed with a 60 s lease: the island states
`CONFIG_NROS_ZENOH_LEASE_MS=60000`, and nano-ros moved the Zephyr default to
60000 in issue 1574 (`58541dc14`, PR #1461).

What remains open:

- reproduce on current main with the island's explicit lease lines removed
  (the default alone), a stock router and host peers joining, and close
  W8a's note on that run;
- the `min()` itself is a zenoh-pico deviation from zenoh's lease semantics
  (a lease is how long the OTHER side may wait); the W15 handoff calls it an
  issue candidate, not filed. File it, or record why the 60 s default is the
  answer;
- under QEMU's `guestfwd` a reconnect never comes back (one host TCP
  connection for QEMU's lifetime); a harness artifact, not a board one, but
  any QEMU lane that tests reconnect must know it.

**Result (2026-10-07), issue 1731.** Reproduced on main with the default
alone, on native_sim (not the island's QEMU image): the Zephyr `c/talker`
(zenoh, `CONFIG_NROS_ZENOH_LEASE_MS=60000`, the default since issue 1574)
against a stock Humble `rmw_zenohd` (`lease: 60000`, `keep_alive: 2`),
`RUST_LOG=zenoh_transport=debug`. One client transport opened at +0 s, a
host peer (`ros2 topic echo`) joined at +75 s and left at +115 s, and the
image's transport stayed open until the image was stopped at +160 s: no
CLOSE, no re-INIT, no "Closing session because it has expired" on the
console. The `min()` is filed as issue 1731 (open, an upstream zenoh-pico
deviation: one number expires the peer and paces our keep-alives, where zenoh
gives each side's lease to the other); the 60 s default is the answer while
it stands, and the rule is not to state a lease below the router's. QEMU
`guestfwd` caveat recorded there: a QEMU lane can show the absence of a drop,
never a reconnect. For the island: delete the explicit
`CONFIG_NROS_ZENOH_LEASE_MS=60000` lines if wanted (the default is the same).
**Resolved 2026-10-30 (issue 1731, archived):** the pinned zenoh-pico fork now
judges the router on the router's announced lease and paces our keep-alives on
ours, as zenoh does, so a lease below the router's no longer drops the session.

### T2 -- `island_trace_cost_max` read 0 in two runs

phase8-W8's open note. The counter is the island's, not nano-ros's: a DWT
CYCCNT bracket around each marker write in
`src/safety_island_tracing/include/island_trace.h`, read over SWD by name.
Listed here because the board's figure for the cost of a trace write is the
only one either repository has, and I3 adds writes. Find why a run that wrote
markers reads a maximum of 0 (bracket compiled out, the symbol resolved to a
different copy, or the readout racing the reset), and fix it in whichever
repository owns the cause.

### T3 -- joining while inputs already flow (issues 1533, 1534)

Status on origin/main (2026-10-01): **both issue files read `status: open`.**
They are tracked as files under `docs/issues/`, not as GitHub issues
(`gh issue view 1533` resolves to nothing).

- [Issue 1533](../issues/archived/1533-zenoh-pico-read-task-stops-on-one-rejected-message.md)
  (the read task stopped for good on one rejected message): its fix is
  on main -- zenoh-pico `52f60b79`, pinned by PR #1389 (`10b2782be`), and
  carried forward in the current pin `e28ff603`. The file still says "prepared
  on the fork, not yet pinned"; its status is stale and should be resolved
  and archived after a check that the pin contains the change.
- [Issue 1534](../issues/archived/1534-zephyr-tx-flush-task-outranks-the-read-task.md)
  (the tx-flush task outranked the read task): PR #1443 (`8d59adba7`) gave
  the flush task the read band and made Zephyr serial TX interrupt-driven.
  Still open per the file: nano-ros does not state the priority the MAIN
  thread registers at (`CONFIG_MAIN_THREAD_PRIORITY`, default 0, above every
  transport band), and the 1 KiB RX ring default was measured at its edge.

The island works around both by configuration (phase8-W10, `serial-link.md`
section 11): main registers at `CONFIG_MAIN_THREAD_PRIORITY=5` against the
read task's 4, and `CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES=4096` (the issue
file says 2048; the island's board conf states 4096). Before W10 the
workaround was join order: the island joined before the inputs flowed
(phase8-W2). Close 1534 with a derived main priority, or a priority-plan
check that refuses main above the read band, and a measured RX-ring default.

**Result (2026-10-07).** Issue 1533 resolved and archived: its fix is in the
pin (`_z_handle_network_message_in_frame` and `_z_rx_rejections` in zenoh-pico
`e28ff603`'s `src/transport/common/rx.c`, since PR #1389); only the file was
stale. Issue 1534 stays open with its residue narrowed: the main-thread
priority is now CHECKED, not derived (a Zephyr configure with a serial zenoh
link warns when `CONFIG_MAIN_THREAD_PRIORITY` is not below the read task,
computed from the image's `.config` the way `nros_zephyr_native_priority`
maps the band: 200 on 15 preempt priorities is k_thread 4, so it asks for 5 or
more); Zephyr owns that symbol's default, and an image joining an idle domain
does not need it. The RX ring is documented (the island's 815-1024 high water
at 1024 and its 4096) but not re-defaulted, because no lane measures a serial
join. For the island: keep `CONFIG_MAIN_THREAD_PRIORITY=5` and
`CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES=4096`; the configure now agrees with
the first.

### T4 -- an end-to-end violation test on the board

Once I1 and I2 land: build an image with a deliberate overrun (a handler that
busy-waits past a monitored `max_latency`, after arming), run it on the
S32K344 with lpuart0 unwired, and see the violation through D1's channel with
the right rule, endpoint and measured value, and no start-up entries ahead
of it. The same test on QEMU or native_sim guards it in CI; the board run is
the acceptance.

**Result (2026-10-06).** Host half landed:
`t4_an_overrun_after_arming_is_the_one_stored_violation` (`nros-node`
executor tests, run by `node-std-tests` and `test-unit`) drives the real
`max-latency-runtime` rule on the MockSession executor with an INIT/RUN node
that arms on entering RUN, then overruns once: nothing stored and the
suppressed count advancing before arming, exactly one violation after it
(rule, endpoint, measured >= declared), the drain hook's line, the four
markers, and the ring keeping the latest when overfilled. NOT done: the
native_sim / QEMU fixture. It needs a new contracted Zephyr workspace image
(bringup, contract, `nros sync`, west build, router, `nros-tests` case) --
estimated past half a day in this environment, so stopped here. The cheapest
route is a variant of `workspace-zephyr-cpp-derived-tiers` with
`CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `CONFIG_NROS_VIOLATION_DRAIN_REPORT=y`,
asserting `contract violation #1: max-latency-runtime` on the console. The
board run stays the island's.

**Board result (2026-10-06, the island's phase9-W4, image at this phase's
PR head).** The channel works on the S32K344: `armed` reads 0 before the
inputs are established and 1 after (the handler arms when every input is
established, not at INIT_TIMEOUT, because INIT always times out on the board
before Autoware starts), start-up noise lands in `suppressed_before_arm`
(5-8 entries), the record is read by symbol into every act's
`violations.txt`, and a stored violation is four markers in the island's
trace (written at 256 + id, 277-280, beside the island's own 1-31). A
commanded 250 ms overrun of the handler's tick after RUN (`ISLAND_DEBUG_
OVERRUN`, written over SWD) shows as a 250.65 ms callback in the trace
followed by `timer-overrun-runtime` x4 and `release-jitter-runtime`
246918 us; an HPC-loss act afterwards stored nothing during the act
(VERDICT PASS). The gate as written ("exactly one `max-latency-runtime`
verdict, ring empty after RUN") did NOT pass, on three counts that are this
runtime's, now I6-I8 below. Readouts verbatim in the island's
`docs/takeover-trace.md` section 12.

**native_sim result (2026-10-07).** The CI guard landed as a new one-node
workspace rather than a variant of `workspace-zephyr-cpp-derived-tiers`,
because a latency row needs a contracted `max_latency` and adding one to the
derived-tiers contract would also give that fixture a 50 ms derived deadline
(D4). `examples/workspaces/violation-cpp` (fixture
`workspace-zephyr-cpp-violation`, router port 7697): a handler that overruns
80 ms in INIT, arms on tick 3 and overruns 150 ms on tick 6, built with
`CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `CONFIG_NROS_VIOLATION_DRAIN_REPORT=y`.
`tests/violation_channel_e2e.rs` asserts exactly one
`contract violation #<n>: max-latency-runtime /handler/state measured=>=150
declared=50` line, after the RUN line, and the arming line's suppressed count;
4 of 4 runs passed (about 3 s each). The line is `#2`, not `#1`: on
native_sim's 1 ms spin cadence the release-jitter rule stores
`measured=1000 declared=1000` right after arming, a wake exactly one period
late, which `check_release_jitter` reports because its tolerance is "less than
a full period"; the derived-tiers fixture shows the same line. That is a
rule-at-quantization question, not a channel defect, and is left here. Found
on the way: a `steady_clock` busy-wait never ends on native_sim (simulated time
does not advance while the CPU spins); the fixture uses `k_busy_wait`.

### I6 -- `max-latency-runtime` never judges a timer-fired callback

The 250 ms overrun produced no `max-latency-runtime` verdict at all, on a
handler whose `on_timer` path carries a 206 ms monitor. Reading of the
executor (unconfirmed by a unit test yet): timers fire in the spin sweep,
and that sweep measures nothing, so `attribute_latency` never sees a timer
dispatch. Every contracted path on the island is timer-driven or a service
handler invoked from one, so on this image the contract's latency rows are
never judged at run time; the host T4 test passes because its overrun is a
subscription callback. Fix: time the timer dispatch like a take-driven one
and charge it to the publishers whose count advanced; add the timer case to
T4's host test (it must fail before the fix). D3 (the route) stays separate;
this is the callback half not working for timers.

### I7 -- the /diagnostics reporter runs on the spin thread

The first stored violation halted the board 60 ms later (fatal 35,
`K_ERR_ARM_USAGE_ILLEGAL_EPSR`): the reporter builds its message in a 512 B
stack buffer on the spin thread and overflowed the 16 KiB main stack into
the idle thread's. The island raised `CONFIG_MAIN_STACK_SIZE` to 24 KiB
(high-water 18,780 B) to get past it. The reporter also costs 15-20 ms of
spin per verdict over the serial link, which itself trips
`release-jitter-runtime` and `timer-overrun-runtime`, so one verdict breeds
more. Fix: size the main stack from the reporter's need in the derivation
(or move the reporter's buffer to `.bss`), and report off the spin path
(deferred to the next idle slot, or coalesced), so a verdict does not cost
the tick that is being judged.

**Result (2026-10-07).** Stack half: issue 1726 (PR #1750), the report is
streamed and `publish_violation`'s frame went from about 10 KB to about 1 KB
on the host. Scheduling half: issue 1727. A stored verdict is queued for the
sink (four deep) and leaves from `Executor::flush_violation_reports` at the
end of a spin, after its dispatches and rules, once the next timer is due no
sooner than the last report took (backstop 1 s); the pending verdicts are
coalesced into one `DiagnosticArray`, as many as fit the 512 B buffer; the
reporter's time is stated (`violation_report_cost_us`) and not charged to the
next release-jitter sample. The ring, the SWD record, the markers and the log
stay at detection. Host test `i7_a_slow_report_breeds_no_overrun_or_jitter_verdict`:
an 80 ms handler overrunning a 5 ms budget on two publishers beside a 40 ms
timer, with a sink that costs 40 ms per message, stores the two latency
verdicts and nothing else, in one sink call; with the report at detection it
also stores `timer-overrun-runtime` 1 and `release-jitter-runtime` 92 ms, the
board's shape. For the island: nothing to set; re-try
`CONFIG_MAIN_STACK_SIZE=16384` on the new pin and read the stack paint, and
expect `/diagnostics` messages with up to four statuses.

### I8 -- the first rate window after arming reads short

Four `rate-hierarchy-runtime` verdicts appear once, right after arming, at
9984-9990 mHz against 10000 (0.03-0.16 % short) and never recur. Arming
reopens the rate windows mid-period, so the first window counts one period
less than a full window. Fix: start the first window at the next sample
after arming (or allow one period of tolerance on the first window only).
With I6-I8 in place the island's ring should read EMPTY after RUN apart
from the silence verdict that needs an epoch clock (island phase 9 W3).

**Result (2026-10-09, island rehearsal at `899dd1068c`, measured on its pre-rebase copy -- same patch).** The re-anchor
moved the figure from 9984-9990 to 9999 of 10000 mHz and the verdicts
stayed, one per topic, once: the window's phase was not the cause. I9.

### I9 -- `check_rate` floors a stream at its declared rate

`check_rate` judged `published * 1e9 / window_us < min_rate`. The window
closes at the first tick that finds it elapsed, so `window_us` is the
interval plus that tick's lateness, while a stream at exactly its declared
minimum has published one sample per period: 50 over 5.0005 s is 9999 mHz,
in every window, reported once because `violated_last_window` holds the
rest. Fix: judge the count against `floor(min_rate * window_us)`, which is
the fewest samples a stream at the declared minimum can deliver in a window
of that length whatever its phase; `measured` in the verdict stays the
quotient. Test `a_stream_at_its_rate_checked_late_is_not_judged_slow`:
10 Hz publishes on the grid, the check runs 700 us late, 60 s, no verdict;
a 5 Hz stream checked the same way is judged at about 5000 mHz.

The island's emergency operator read 29773 of 30000 mHz on its two topics
in the same window, 149 samples in 5.004 s. Measured on the board
(2026-10-09, island run `w4r-ticks`, the timer's events 18/19 forwarded):
the operator's timer ticks every 33.000 ms (1073 ticks over 35.4 s, mean
32.999 ms, 28.9-37.3 ms; its period is `1000 / update_rate` in integer
milliseconds, 30.30 Hz), and the executor loses no time (the spin
accumulates `now - last_spin_end`, `timer_try_process` carries the
remainder). The shortfall was the log floor above: six verdicts logged
back to back stalled the spin about 100 ms, the timer's `Skip` policy
fired once and dropped two periods (`timer-overrun-runtime timer
measured=2`), and 151.6 - 2 = 149 in that window. With the island's log at
errors only the record holds no operator rate verdict. Not a nano-ros
defect; what remains is the contract's margin: a `min_rate_hz` equal to
the producer's own timer rate makes any two-period stall a rate verdict.

### I7, on the board -- the 15 ms were the log floor, not the reporter

The rehearsal at `899dd1068c` still stored verdicts 15-17 ms apart, each
followed by `timer-overrun-runtime` and `release-jitter-runtime`. Cause:
`ViolationChannel::record` calls `log_violation` at detection whenever the
drain hook does not report, and the island's board runs
`CONFIG_LOG_MODE_IMMEDIATE=y` with the console on an unwired lpuart0 at
115200 baud: one ~170-character warn line is about 15 ms on the spin
thread. The island turns its log to errors only. For nano-ros: the log
floor (issue 0514) costs the judged tick on any immediate-mode console; an
image that keeps the SWD record or a trace sink should be able to turn the
detection log off (a knob beside `NROS_VIOLATION_DRAIN_REPORT`), or the
line should leave with the deferred report. Not done here.

### I6, on the board -- one verdict per monitored publisher

The 250 ms overrun of the handler's tick stored three `max-latency-runtime`
verdicts, one per publisher the tick advanced (hazard_lights_cmd 100 ms,
mrm_state 206 ms, takeover_request_state 206 ms). That is
`attribute_latency` as designed: each publisher's own budget was broken.
The island's gate reads "one verdict per monitored publisher of the tick".

### I5, on the board -- 106496, not 104448

`CONFIG_NROS_ZEPHYR_HEAP_SIZE=104448` read `HEAP HEADROOM: REFUSED --
24056 bytes, floor is 24576` after an encore (peak 80,904 of 104,960);
106496 reads `ok -- 26104 bytes spare (peak 80904 of 107008, floor
24576)`. The figure above should read 106496 for the three-node board
image with the takes traced.

### I10 -- the serial reader held the CPU for a frame's wire time

The island's encore `w4-gate-encore` (pin `319715968`) stored four
`release-jitter-runtime` verdicts, 13.6-17.9 ms late on the 10 ms spin,
during scenario setup, each beside a `kinematic_state` take. The rule
judges `spin_once` entry to entry (`record_release_jitter`, spin.rs), so a
verdict is an interval of 23.6-27.9 ms.

**Measured** (island branch `phase9-spin-latency` on island `9727131`, real
encores on the S32K344, 921,600 baud). A diagnostic build (not merged:
`experiments/spin-latency/*.patch` in the island) marks five phases of
each spin with a trace event, and the island's sink charges every
segment of every spin-to-spin interval to the spin thread, the idle
thread and the rest (Zephyr thread runtime stats), with the serial ISR's
own time and every contended zenoh mutex wait beside it. Run
`sl-diag-before` (pin `319715968`), the act alone: 3589 intervals, 74 of
them 10 ms or more late (max 19 ms), 199 at 5 ms or more; 8 verdicts
stored, 10.0-18.8 ms. Every interval 10 ms or more late in the record has
one shape: the wait segment (park bound 10 ms) lasted 18-24 ms, the spin
thread ran about 50 us of it, and `zpico_read` ran 11.5 ms (one 1 KB
frame, about 816 ISRs) to 17.3 ms (back-to-back frames). The spin thread
waited on no zenoh mutex in any of them. The read task's CPU equals the
frame's wire time.

**Mechanism.** zenoh-pico `eb16447f`, `src/system/zephyr/network.c`: the RX
ISR gave the reader's semaphore on every FIFO drain (:1074), and
`_z_read_serial_internal` moved the ring one byte per loop through
`_z_serial_rx_get` (:1158, :1304) with a `k_uptime_get` (:1298) and a
`uart_err_check` driver call (:1338) per byte. At 921,600 baud a drain
carries one or two bytes (a byte is 10.9 us, the ISR runs 7.6 us), so each
byte cost a wake, two context switches and that bookkeeping, about 6.6 us
of reader CPU; with the ISR that is the whole CPU for as long as a frame is
on the wire. The read task runs at k_thread 4 (read band 200) and the
island's main at 5 below it, as T3 / issue 1534 ask, so a spin due during a
frame is released when the frame ends. Ruled out: the park granularity
(1 kHz tick, adds at most 1 ms: the 1-2 ms bin), the session or link lock
(no wait by the spin thread in any late interval), the wake path (data
breaks the park, timers bound it), and the clock (the k_cycle intervals
agree with the executor's verdicts).

**Fix** (zenoh-pico `199e611f`, fork branch `fix/serial-frame-wake`, one
commit on `nano-ros`). The ISR gives the semaphore only for a drain that
carried a frame delimiter (0x00) or left the ring half full; the reader
moves whole runs with `ring_buf_get_claim` up to and including the
delimiter and checks the UART error flags once per frame (they are
sticky). The TX ISR wakes a waiting sender when half the TX ring is free,
not after every FIFO fill. The polled path is unchanged. A new counter,
`_z_zephyr_serial_stats.rx_wakes`, reads about one per frame.

**Result.** Run `sl-diag-fix` (pin plus this commit), the act alone: 3997
intervals, 1 at 10 ms or more (14.5 ms), 13 at 5 ms or more, 99th
percentile in the 3-4 ms bin (before: 12-13 ms); `zpico_read` at most
1.5 ms per interval; `rx_wakes` 1548 for 1542 frames; no bad frame, no
partial, no ring overflow (high water 1106 of 4096). The one late interval
is the handler's reaction tick: 13.2 ms of the spin thread's own CPU in
one callback (two `std::printf` lines to the 115,200 baud console,
4.9 ms each, plus a 2.9 ms wait for the link in the express service
send), the island's to remove. The clean image at the backport
(`fix/spin-release-latency-319715968`, run `sl-bp-encore`, encore PASS)
stored one verdict, 10.3 ms, on the same kind of tick (the cancel, with
its two printf lines). No host test: the defect is the CPU cost of a
Zephyr ISR and reader pair and no host lane runs the Zephyr serial port;
the counter makes it checkable over SWD.

Still open, measured: the RX ISR still runs once per byte or two (about
6.5 us each, half the CPU at line rate; the LPUART RX watermark is the
driver's); a `spin_once(10)` loop parks 10 ms from its own end, so an
interval is the previous spin's work plus up to one 1 ms tick late (the
1-4 ms bins, 603 of 3997 intervals at 1 ms or more), which a declared
cadence or a park anchored to the last release would remove; the
tx-flush task still takes up to 5.3 ms of CPU in some intervals.

### I11 -- a periodic spin is released on a grid, not after its last end

After I10 (PR #1857) a `spin_once(10)` loop still parked 10 ms from its own
entry: every interval was the previous spin's work plus up to a tick late
(603 of 3997 at 1 ms or more, island run `sl-diag-fix`), and
`release-jitter-runtime` judged exactly that entry-to-entry interval.

**Changed** (spin.rs `record_release_jitter`, `record_grid_release`,
`ReleaseGrid`). With no declared cadence and a non-zero timeout, the
timeout is the period of an absolute release grid: the first spin anchors
it (its park is a whole period, as before), and every later park is
bounded by the time left to the next grid point, not a period from now.
The release instant is the clock read after the park. A spin woken before
its grid point (data, a timer, a platform source) dispatches and is not a
release; the grid does not move. A changed timeout re-anchors.
Unchanged: a zero timeout (poll, `spin_some`) and a declared cadence
(`set_spin_nominal_us`, the tier loops that sleep themselves).

**Catch-up** (the timer `Skip` policy). A loop that comes back after its
next point was due is released at once, late by the difference. One that
comes back a whole period or more after it is released once, for the
latest due point; the older points are dropped and counted
(`Executor::skipped_releases`), never released back to back. A park that
oversleeps past the following point serves one release and skips the rest.

**Measured now.** Lateness is release minus scheduled grid point (less the
reporter's own time, I7); a dropped point counts as late by its age when
dropped. `release-jitter-runtime` still fires at one period, so a 1.5
period callback is 5 ms of jitter and no verdict, while 2.5 periods (a
lost release) is. This is the contract's `jitter:`.

**Zephyr wait.** `nros_platform_wake_wait_ms` used `K_MSEC(n)`, which
Zephyr extends by a tick, so every grid release on the tick clock was one
tick late. It now waits for the absolute tick `now + n`
(`CONFIG_TIMEOUT_64BIT`); a wait that ends early is re-parked.

Host tests: tests.rs `an_idle_periodic_spin_releases_on_the_grid` and the
three after it (overrun, data-driven early spin, the rule's measure).

### I12 -- a publish waited for another frame's wire time

After I10 and I11 the island's run `sl-cadence-diag` (the act alone, decoded
in the island's `build/timeline/sl-cadence-diag/spin-diag-act.txt`) still
showed late spins: 10968 spins, 29 at 2 ms or more late, the worst 5.9 ms.
None is the park or the reader any more. In every late interval the spin
thread is inside a callback that sends, and it is either doing the framing
itself or waiting on a zenoh mutex that a sender holds while it sleeps for
the wire: 1.4-2.8 ms per late interval at `_z_link_send_wbuf` (link.c:253)
and `_z_send_n_msg` (tx.c:660), with `zpico_flush` (k_thread 4) the other
side.

**Mechanism.** zenoh-pico `199e611f`, `src/system/zephyr/network.c`:

- A. The interrupt-driven TX ring (issue 1534) was 256 bytes
  (`_Z_ZEPHYR_SERIAL_TX_RING_BYTES`). A frame longer than that slept in
  `_z_serial_tx_put` until the ISR drained it at line rate, about 11 us a
  byte at 921,600 baud, and the sender sleeps holding the transport TX
  mutex, so a publish from the spin thread also waited behind the tx-flush
  or lease task's frame for its whole wire time.
- C. Framing CPU on every frame, both directions: `_z_crc32`
  (src/utils/checksum.c) ran a bitwise loop, 8 iterations a byte, and
  `_z_send_serial_internal` z_malloc'ed two ~1.5 KiB buffers per send and
  copied the payload twice (into a temporary frame, then COBS-encoded);
  `_z_read_serial_internal` z_malloc'ed two more per received frame.

The per-byte RX/TX ISRs (about 6.5 us each) are the third cost and are not
touched here; DMA is a later item.

**Fix** (zenoh-pico fork branch `nano-ros`, pinned by this change):

1. The TX ring defaults to 2048 bytes, and
   `CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES` (zephyr/Kconfig, mapped to
   `_Z_ZEPHYR_SERIAL_TX_RING_BYTES` in zephyr/cmake/nros_rmw_zenoh.cmake)
   defaults to the same. 2048 holds one maximum-size frame
   (`_Z_SERIAL_MAX_COBS_BUF_SIZE`, 1516) whole with room to spare, so a
   send that finds the ring empty is a copy and a return, the way a Linux
   tty's xmit buffer or an AUTOSAR COM send behaves; the wire drains it in
   the background. A send that does not fit (earlier frames still queued)
   queues what fits and sleeps for the rest, bounded by the existing
   `_Z_ZEPHYR_SERIAL_TX_TIMEOUT_MS`: reliable traffic is never dropped to
   make room. Two counters join `waits`, `timeouts` and `ring_high_water`
   in `_z_zephyr_serial_tx_stats`: `frames_waited` (sends that did not fit
   whole; zero means the ring never filled) and `max_frame` (the longest
   frame queued). The RAM cost is the ring, static.
2. `_z_crc32` is table-driven, with a 1 KiB const table in flash. The
   result is bit-for-bit the old loop's, which is NOT the standard CRC-32:
   it shifts right with the unreflected polynomial 0x04C11DB7, so
   "123456789" gives 0xFC4F2BE9 (standard: 0xCBF43926). The value is on the
   wire and stays.
3. `_z_serial_msg_serialize_direct` COBS-encodes header, length, payload
   and CRC straight into the output, with no temporary frame; the CRC runs
   over the caller's payload. The output is byte-identical to the old path,
   including the encoder's trailing code byte after a frame that ends on a
   full 254-byte block. `_z_serial_msg_serialize` keeps its signature and
   calls it, so the other ports get the single copy too.
4. The Zephyr port's framing buffers are static: one send buffer, one
   receive buffer and one decode buffer, each set under its own k_mutex.
   The transport TX mutex does not cover every send (the handshake and the
   split-lock fragment path send without it), so the port does not lean on
   it; the locks are uncontended in steady state. The receive path did no
   full-buffer clear (its `for (i < _Z_SERIAL_MAX_COBS_BUF_SIZE)` is the
   polled path's byte loop); its two mallocs per frame are gone.

Per send that is: no heap allocation (was two, ~3 KiB, plus their frees),
one copy of the payload (COBS into the static buffer) plus the ring copy
(was two plus the ring), and a CRC at one table lookup a byte (was eight
shift-and-xor steps). Per received frame: no heap allocation (was two).
Static RAM: +1516 send, +1516 + 1507 receive, +1792 ring at the default.

**Verify.** Host: zenoh-pico `tests/z_msgcodec_test.c` checks the table
CRC against the bitwise loop (the "123456789" vector and every single byte)
and the direct serializer against the old tmp-and-COBS path byte for byte
(empty frames, runs at 250-256 and 507-509 bytes, all zeros, a zero at
either end, the MTU, random payloads of every length to 600), and that one
byte short of room fails without writing past it. Board: build with
`CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES` (the island states 4096), run the
act, and read over SWD `_z_zephyr_serial_tx_stats.frames_waited` (0 means
no publish waited for the wire), `max_frame` and `timeouts` (0); the
late-spin decode should lose the lock waits at link.c:253 and tx.c:660.
Not yet measured on the board.

### I12, on the board -- the waits are gone; the serial ISRs are what is left

Island run `sl-rclcpp-diag` (nano-ros `4f91dfd7b`, which carries I12 as
#1946, island TX ring 4096; encore with the spin diag): `_z_zephyr_serial_tx_stats`
reads `waits` 0, `timeouts` 0, `frames_waited` 0, `max_frame` 778, and the
lock waits at link.c:253 and tx.c:660 are 2-10 us (were 1.4-2.8 ms). The
late spins did NOT go away: 34 at 2 ms or more in 10121 (the I11 run had 29
in 10968), the worst 5.1 ms. What fills each late interval now is (a) the
transport tasks, all three at k_thread 4, running ahead of the executor at 5
when its release is due (1.7 ms median), and (b) the per-byte serial ISRs,
2.5-5.7 ms over 250-800 calls in every late interval.

(a) is the application's priority plan, not this phase's: the island raises
its executor to 3 on its first spin, after registration (island
`e93e258`), and `sl-rclcpp-prio` reads 6.4 late spins at 2 ms or more per
10k (was 33.6), 22 at 1 ms or more (was 51), worst 4.0 ms, RX ring high
water 1006 of 4096, no overflow. What remains is one callback's dispatch
(about 1.8 ms) with the serial ISRs inside it: I13-I16.

### I13 -- the serial ISR path costs 7-10 us a call

**Mechanism.** At 921,600 baud a byte arrives every 10.9 us; the RX ISR
(`_z_serial_isr`, network.c) costs about 7 us, so it runs about once a byte
(the LPUART FIFO is 4 deep and the mcux driver leaves both watermarks at 0).
TX moves 1-2 bytes a call (`uart_fifo_fill` stops when TDRE drops). That is
about 2,000 ISRs per 1 KiB frame each way, 1,100-1,600 cycles each: code in
flash, the ring and stats in cacheable SRAM, and stats the ISR updates on
every chunk (`ring_high_water`, a volatile counter per call).

**Fix**, the per-call cost (I14-I16 cut the count):

1. `CONFIG_NROS_ZENOH_SERIAL_TCM` (Zephyr, default n, depends on
   `CODE_DATA_RELOCATION` and a `zephyr,dtcm` / `zephyr,itcm` chosen node):
   zephyr/cmake/nros_rmw_zenoh.cmake relocates zenoh-pico's
   `system/zephyr/network.c` text to ITCM and its bss (the RX and TX rings,
   the framing buffers) to DTCM with `zephyr_code_relocate`. On the S32K344
   that moves about 12.5 KiB off the 320 KiB SRAM (95.7% used) into DTCM
   (66 KiB free), and DTCM is not behind the D-cache, which I15/I16's DMA
   buffers need anyway.
2. The ISR body keeps only what the reader needs: the high-water updates
   move out of the per-chunk loop (once per ISR, not per 32-byte read).
3. A diag hook: `_z_zephyr_serial_isr_cycles` / `_isr_calls`, built only
   under `CONFIG_NROS_ZENOH_SERIAL_ISR_STATS` (default n), so a reader of
   the island's spin diag can charge ISR time to the callback it fell in.

**Verify.** Board, the island's spin diag: `isr_us / isr_n` per late
interval drops (was 7-10 us); RX `ring_overflows` and UART overruns stay 0.

**On the board** (island `sl-i13`, `sl-i13-b`, `sl-i13-notcm`; spin-diag
encores, the act segment only, about 5,500-6,000 spins each): 6.2 us per
ISR call with TCM, 6.0 without, about 505k calls per act either way. The
ISR is not memory-bound; TCM is for the SRAM (98.7% -> 94.8% on the
island) and for I16's buffers, not for speed. The late-spin count did not
move (60.6 and 76.8 per 10k spins >= 2 ms late with TCM, 10.9 and 62.4
without: run to run, the same image varies that much), so the count of
ISRs, not their cost, is the lever: I14 and I16.

### I14 -- one interrupt per FIFO, not per byte

**Fix.** `CONFIG_NROS_ZENOH_SERIAL_FIFO_WATERMARK` (Zephyr, default n; NXP
LPUART only, `DT_HAS_COMPAT_STATUS_OKAY(nxp_lpuart)`): after the device is
configured, set RXWATER to 2 with the receiver-idle interrupt (`FIFO.RXIDEN`)
so a frame's tail is not stranded below the watermark, and fill TX to the
FIFO depth on each TDRE. The ISR reads and fills the FIFO registers itself
on this path, since the stock `fifo_read` loops on RDRF (bytes below the
watermark) and `fifo_fill` stops at TDRE. About 3x fewer calls; overrun
margin falls to about 2 characters (22 us), so it is off by default and the
UART overrun counter is the check. A board with DMA should prefer I15/I16;
this is the path for one without.

**Implemented** (zenoh-pico `eb3eb60c`, nano-ros `b7f20867d7`):
`CONFIG_NROS_ZENOH_SERIAL_FIFO_WATERMARK` (depends on
`NROS_ZENOH_LINK_SERIAL && UART_INTERRUPT_DRIVEN && UART_MCUX_LPUART &&
DT_HAS_NXP_LPUART_ENABLED`) -> `_Z_ZEPHYR_SERIAL_FIFO_WATERMARK`. The bind
looks the device up in a table built from `DT_FOREACH_STATUS_OKAY(nxp_lpuart)`
(`{DEVICE_DT_GET, DT_REG_ADDR}`, no driver-private state) and, after every
`uart_configure`, writes WATER (RXWATER 2, TXWATER 0) and FIFO.RXIDEN 1 with
TE/RE briefly off, as `LPUART_Init` does. `_z_serial_isr_body_lpuart`
counts and clears STAT.OR (the driver clears it right after the callback),
drains `WATER.RXCOUNT` bytes from DATA, and while TIE is on and TDRE set
writes `PARAM.TXFIFO`-depth minus `WATER.TXCOUNT` bytes; ring, wake rules and
counters as before, interrupt enables still through `uart_irq_*`. Registers
through `LPUART_Type` (`fsl_device_registers.h`, S32K344_LPUART.h). Board
image builds (TCM on): FLASH +432 B, RAM 0, ITCM +404 B, DTCM +8 B against
the I13 image. Not run on the board.

**On the board** (RXWATER 2): 0 spins >= 1 ms late in the whole run (61 in
the act before), worst 0.76 ms; serial ISR calls 221k against 719k, 2.99
us/call against 6.2; gate PASS. But 4 overruns and 4 bad frames a run
(`_z_zephyr_serial_stats`), and the TX ring touched its 4096 high water
once at boot (frames_waited 1).

**Watermark knob** (zenoh-pico `17b14961`, nano-ros `bc84b08878`):
`CONFIG_NROS_ZENOH_SERIAL_FIFO_RXWATER` (int 0..3, default 1) ->
`_Z_ZEPHYR_SERIAL_LPUART_RXWATER`, clamped at arm time to `PARAM.RXFIFO`
depth - 1. RDRF asserts at RXCOUNT > water, so the margin from RDRF to OR
is depth - water characters: 44/33/22/11 us for 0/1/2/3 on the 4-deep
S32K344 FIFO at 921600. 1 halves RX interrupts against 0 and keeps 33 us.
Checked: clearing OR before the drain drops nothing (the lost character is
the one that found the FIFO full; FIFO contents survive; the receiver
stores nothing while OR is set, so clearing early resumes sooner), only OR
is written as 1 (IDLE and the other W1C flags written 0), and RXIDEN needs
no clear (it asserts RDRF only while idle with RXCOUNT > 0, which the drain
takes to 0). Board image (TCM on): FLASH 572492 B, RAM 300624 B, ITCM 14548
B, DTCM 75400 B.

**On the board** (RXWATER 1, island `sl-i14w1`): 0 spins >= 1 ms late in
the whole run (11,179 spins), worst 0.94 ms; 0 overruns, 0 bad frames, 0
ring overflows; 289k ISR calls at 2.94 us (0.85 s of CPU against 4.46 s
for I13); gate PASS (total 3492.69 of 4934.67 ms).

### I15 -- TX by DMA

**Fix.** `CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC_TX` (Zephyr, default n, needs
`UART_ASYNC_API` and `dmas` on the link's UART): `_z_serial_tx_put` still
copies into the ring; when no transfer is in flight it claims the ring's
longest contiguous run and starts `uart_tx`. `UART_TX_DONE` finishes the
claim, starts the next run or clears the in-flight flag, and gives the TX
semaphore. RX stays interrupt-driven, which needs
`UART_EXCLUSIVE_API_CALLBACKS` (default y). With I13's DTCM ring the eDMA
driver translates the DTCM address itself (dma_mcux_edma.c), and no cache
maintenance is needed. One or two DMA interrupts per frame instead of
about 700.

**Implemented, folded into I16.** I15 alone is not possible on Zephyr 4.4:
with `UART_ASYNC_API` and `UART_INTERRUPT_DRIVEN` both on, the mcux LPUART
driver requires `UART_EXCLUSIVE_API_CALLBACKS` (BUILD_ASSERT,
uart_mcux_lpuart.c:50) and keeps one API per device: `uart_callback_set`
returns -ENOTSUP once the IRQ callback is set (:785-788),
`uart_irq_callback_user_data_set` clears the async callback (:412-425), and
`mcux_lpuart_isr` dispatches to one API by `api_type` (:1088-1092). Interrupt
RX next to DMA TX on one UART therefore cannot be had; both go to DMA under
one option, below.

### I16 -- RX by DMA

**Fix.** `CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC_RX` (default n, implies I15's
callback): `uart_rx_enable` on one of two 256-byte DTCM buffers, answering
`UART_RX_BUF_REQUEST` with the other; `UART_RX_RDY` puts the bytes into the
existing RX ring and gives the reader's semaphore by the same rule as the
ISR (a frame delimiter, or the ring half full); the COBS reader is unchanged.
`UART_RX_DISABLED` re-arms. Known costs of Zephyr 4.4's lpuart async RX:
the idle-line flush runs on the system workqueue after a `k_work_delayable`
(uart_mcux_lpuart.c:1060-1064, :968-981), so a frame's last bytes reach the
ring 1-2 ticks late; the configured idle type is set on a local struct and
never applied (:1243-1244), so idle may fire early (extra flushes, no lost
data); any RX error stops reception (:1023-1057). Upstream's only S32K3 use
is a 115,200 loopback test with all RAM in DTCM.

**Implemented** (with I15; zenoh-pico `e3080885`, nano-ros `79b1af0aeb`):
`CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC` (depends on `NROS_ZENOH_LINK_SERIAL &&
UART_INTERRUPT_DRIVEN && UART_ASYNC_API` and `NROS_ZENOH_SERIAL_TCM ||
!DCACHE`) -> `_Z_ZEPHYR_SERIAL_UART_ASYNC`, and
`CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC_RX_TIMEOUT_US` (default 25, range from 1:
0 disables the driver's idle flush). The bind tries `uart_callback_set`
first and keeps the interrupt path on failure, or when the first
`uart_rx_enable` fails (`_z_zephyr_serial_async_stats.active` says which
runs). TX: `_z_serial_tx_put` kicks `uart_tx` on the TX ring's contiguous
run when none is in flight (`SYS_FOREVER_US`, no TX timer); TX_DONE/ABORTED
finishes the claim (partially on abort), chains the next run, and gives the
TX semaphore by I13's waiting rule. RX: two 256 B buffers, alternated on
BUF_REQUEST; RX_RDY puts into the RX ring under a spinlock (the DMA ISR and
the sysworkq flush both produce), same wake rule; RX_STOPPED reasons go to
`overruns`/`framing_errors`/`noise_errors`/`parity_errors`; RX_DISABLED
re-arms. A re-open aborts TX and disables RX before `uart_configure`
(which runs `LPUART_Deinit`). `_z_zephyr_serial_async_stats` adds tx_dma,
tx_aborts, tx_errors, rx_rdy, rx_stopped, rx_rearms, rx_rearm_failed; under
`ISR_STATS` the callback's cycles count as ISR time (the sysworkq flush
included). All of it in network.c's bss/text: DTCM 0x20000000.. and ITCM in
the board image. Latency: the island's tick is 1000/s, so a frame's tail
reaches the reader 1-2 ms after its last byte (the idle flush is a
`k_work_reschedule(K_USEC(25))`, i.e. the next tick or two) -- a cost the
interrupt path does not have, to be weighed on the board against the ISR
time saved. Board image builds (TCM on, `CONFIG_UART_ASYNC_API=y`): FLASH
+11312 B, RAM +3464 B (driver async state, eDMA driver), ITCM +1176 B, DTCM
+2608 B against the I13 image. Not run on the board.

**On the board: the island never joined.** Boot asserted in
`EDMA_TcdSetTransferConfig` (fsl_edma.c:517, `((uint32_t)tcd & 0x1FU) ==
0U`) from `dma_mcux_edma_configure` <- `configure_and_start_rx_dma` <-
`mcux_lpuart_rx_enable`: `dma_tcdpool0` (`__aligned(32)`, `.dtcm_noinit`)
sat at 0x200128b8. Zephyr v4.4.0's cortex_m `linker.ld` and `linker.cmake`
declare the ITCM/DTCM output sections `SUBALIGN(4)`, which FORCES every
input section to 4-byte alignment; it had worked only while the DTCM
content before it ended 32-aligned, and I13's relocated network.c bss moved
that end. Upstream fixed it after 4.4.0 (de3deb75d44d DTCM, issue #107058;
c456d2b1279b ITCM: `ALIGN(4)`). Backported in nano-ros `54e9e518c6` as
`scripts/zephyr/tcm-section-align-patch-4.4.sh` (run by
`scripts/zephyr/patches/4.4.sh`, i.e. `just setup zephyr`) and
`zephyr/patches/tcm-section-align-4.4.patch` (`west patch`); drop both once
the pinned Zephyr has the commits. Rebuilt: `dma_tcdpool0` at 0x200128c0,
`.dtcm_noinit` section align 32; FLASH 583372 B, RAM 304088 B, ITCM 15308 B,
DTCM 78016 B. The assert's message then faulted: the board conf has
`CONFIG_ASSERT=y`, `LOG_MODE_IMMEDIATE`, `LOG_PRINTK` and the shell as log
backend, and the assert fired under `irq_lock` (dma_mcux_edma.c:664), so
the print's shell-lock `k_sem_take` could not pend. A board-conf matter
(deferred log mode, or no shell log backend), not fixed here.

**On the board, with the linker fix** (island `sl-i16b`): the island joins
and runs on DMA (`_z_zephyr_serial_async_stats`: active 1, 1037 TX
transfers, 3300 RX_RDY, no RX stop, no re-arm, no error). 0 spins >= 1 ms
late in the whole run (10,914 spins), worst 0.57 ms; 0 overruns, 0 bad
frames; the serial ISR ran 7.6k times in all (boot, before the async bind),
42 ms of CPU. Gate PASS (total 3497.70 of 4934.67 ms, detect 548.55). The
1-2 tick idle flush is not visible in one run: detect varies with the
handler's 100 ms tick phase (528.62 in the I14 run).

**Which to use.** Both remove the late spins on the S32K344. I14 is the
smaller change (no DMA, no driver async path, no idle-flush latency) at
0.85 s of ISR CPU per act; I16 takes the ISR load to almost nothing but
adds the eDMA and async lpuart drivers (+11 KiB flash, +3.5 KiB SRAM) and
needs the linker backport. The island takes I14 (RXWATER 1) with TCM.

**Verify (I15, I16).** Board: the spin diag's ISR count per late interval
and the late-spin rates; `_z_zephyr_serial_stats` overruns, bad frames and
ring overflows 0; the island's gate act still PASS.

## The board halves, 2026-10-26

Quoted from simple-autoware-safety-island main `f208a38`
(`docs/design/carla-island-demo.md`, the sections for nano-ros 8b95e90dd6 and
5feeb32a7b). Nine board acts on the MR-CANHUBK344, on two images: runs
`i2041-{e1,a1,b1,e2,ov1}` (nano-ros 8b95e90dd6 and its pre-merge parent) and
`i2065-{e1,a1,b1,ov1}` (nano-ros 5feeb32a7b). Every act PASS. The executor's
violation record (`NROS_VIOLATION_RECORD`, capacity 8) was read over SWD after
each act; every record reads `dropped=0 suppressed_before_arm=3 armed=1`.

- **T4 and I6, board half.** The two overrun acts (`RB_OVERRUN=300` on the
  handler's 10 Hz timer tick) each stored three `max-latency-runtime`
  verdicts, one per monitored publisher of the tick, as section "I6, on the
  board" predicts. From `i2065-ov1`:

  ```
  #8: max-latency-runtime /mrm_handler/takeover_request_state measured=301 declared=149 (one dispatch, ms)
  #7: max-latency-runtime /mrm_handler/mrm_state measured=301 declared=149 (one dispatch, ms)
  #6: max-latency-runtime /mrm_handler/hazard_lights_cmd measured=301 declared=100 (one dispatch, ms)
  ```

  A timer-fired callback's overrun reaches D1's channel as a
  `max-latency-runtime` verdict.
- **I8 and I9, board half.** No rate verdict in any of the nine records,
  including the first window after arming. The operators' statuses are
  monitored at `min_rate_hz: 9` against 10 Hz and 30 Hz producers. The
  records hold only the act's own hazard entries (`hazard-detected` x2 and
  `hazard-recovered` x2), the expected `silence-runtime` on
  `operation_mode_availability`, and in the overrun acts the three latency
  verdicts.

Not covered: T4's QEMU leg (still open here), and I9's exact-rate case. The
island's rate floor is 9 Hz under 10 Hz producers, so a stream at exactly its
declared rate was not on the board; I9's host test covers that.

## Cross-repo

- **play_launch** has a parallel phase on the same findings: F1 (charge
  transport on the reaction walk and in a reported fault's detection), F2 (a
  release-jitter key for a timer trigger), F3 (a transport or queueing key on
  a service edge), and a key for an on-demand topic, which also answers D2's
  two `rate-hierarchy-runtime` entries. D3 and D4 here should land against
  the same vocabulary.
- **simple-autoware-safety-island** has its own phase 9, which consumes this
  phase's items (the violation channel, the arming, the trace hooks) and the
  play_launch phase's keys, and re-runs the board acts on them.
- **Carved out of [phase-463](archived/phase-463-host-census-reconciles-contract-with-code.md)**
  when it was archived on 2026-10-12. Both are island-repo work; every
  nano-ros half they depend on has landed.
  - **`island-W2` -- the census becomes the island's configure gate.** The
    island deletes the `NROS_EXECUTOR_MAX_CBS=32` export from its `just build`
    and builds with `[census]` at `refuse`. The nano-ros halves: issue 1419
    made `refuse` the default for every bringup on 2026-10-03, and phase-463
    W6 made a current census the callback count in
    `check_executor_capacity`, so the export now only hides what the census
    would report. Acceptance: the island's board build refuses a stale census,
    and `nros ws entity-census check` passes on its pristine contract
    (phase-463 W3 measured 53 confirmed rows and one error, the seventh
    subscription the contract was missing).
  - **phase-463 W7's replay on the island's own native image.** Run that image
    60 s with `$NROS_PROFILE_OUT` set, then join the profile with the census
    and the contract through `scripts/check-profile-against-census.py`
    (P1-P4). In-tree, `examples/workspaces/derived-tiers-cpp` (the island's
    shape: 4 derived tiers, 4 timers) passes that join 4 <-> 4. What is still
    unverified is the island's real graph and its contract `paths.*.output`.

- **Carved out of [phase-446](archived/phase-446-contract-declares-parameters.md)**
  (its W7) when it was archived on 2026-10-13. Every nano-ros half has
  landed: the contract's `params:`, the store and the parameter services
  sized from them (W1-W6, F1-F3), and replies that refuse with a reason
  (issue 1271). What is left is the island's run.
  - **`446-W7` -- the island's parameters, end to end.** The island declares
    its 21 parameters in its contract, loads each package's parameter file
    in its launch, and builds and boots on native, native_sim and
    MR-CANHUBK344 with parameter services on. Acceptance: the demo's VERDICT
    PASS on both islands, and the board image's heap use with parameter
    services measured and recorded here. The measurement that opened
    phase-446 is the baseline: the worst-case store was 285,440 B at 32
    slots and exhausted the 64 KiB nros heap at boot, the declared store is
    4,200 B for the image's 25 slots, and the parameter services took
    196 KiB of buffers for four nodes before issue 1270.

## Order

D1 before I1 and D2 before I2; I1 and I2 before T4. D3 waits on the
play_launch phase's F1 decision. D4, I3, I4, I5, T1, T2 and T3 are
independent of each other. I13 before I15 and I16 (their DMA buffers are
I13's DTCM rings); I14 is the no-DMA alternative and is independent.

## What this phase does not do

- Change the island's contract or budgets; that is the island's phase 9.
- Change rlm or play_launch; their keys are the play_launch phase's.
- Re-measure anything: every figure above is quoted, with its image named.

## Acceptance

- [ ] D1, D2, D3, D4 each decided and written into this document.
      (D1 and D2 decided 2026-10-06; D3 and D4 open.)
- [x] I1: the generated Zephyr entry drains violations into the chosen
      channel; the ring keeps the latest entries and reports the dropped count.
      (Ring keeps the latest N with total/dropped; SWD record, markers 21-24,
      opt-in drain hook; issue 1714.)
- [x] I2: no start-up violation is recorded as a running one on the island
      image. (Mechanism landed, default unchanged; the island image opts in
      with `CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `nros::arm_monitors()` at
      `INIT_DONE`; issue 1715.)
- [ ] I3: a take hook and a per-timer sampling rate exist and the island
      traces its F4 hops with them. (Hooks exist: markers 25-27, per-slot and
      Kconfig take tracing and timer sampling, issue 1730; the island's trace
      run is its own.)
- [ ] I4: the host metadata probe produces metadata for all four island
      components. (Mechanism on main: workspace-root caps, issue 1470;
      capabilities, issue 0543; `probe-workspace-caps` 5/5. The island must
      move its caps to the workspace root and re-sync.)
- [ ] I5: the derived heap and the configured heap agree, and hold after
      FirstSpin under a host graph on QEMU. (Explained, not closed: no heap
      derivation exists; the record now splits FirstSpin and running peak,
      names the knob without the slab, and a stated derivable knob below its
      derivation warns; island: 104448. QEMU read-path half open; issue 1728.)
- [x] T1: the plain-router join reproduced on current main with the default
      lease and held; the `min()` question filed or answered. (native_sim
      c/talker vs stock rmw_zenohd, host peer joined at +75 s, session held
      to +160 s; `min()` filed as issue 1731, 60 s default is the answer;
      the `min()` itself removed from the fork 2026-10-30, issue 1731 resolved.)
- [ ] T2: `island_trace_cost_max` reads the bracket's maximum in every run.
- [ ] T3: issue 1533 resolved and archived; issue 1534 closed with a derived
      main-thread priority and a measured RX-ring default. (1533 archived;
      1534 open: main priority checked by a configure warning, RX ring
      documented, a measured default still needs a serial-join lane.
      2026-10-30: 1534 resolved -- the flush task follows the read task on
      every port; the main-priority and RX-ring residue is issue 2164.)
- [ ] T4: a deliberate overrun on the S32K344 is reported through D1's
      channel. (**Board leg met 2026-10-26**: three `max-latency-runtime`
      verdicts in the SWD record, see "The board halves, 2026-10-26";
      phase-512 H6 closed. Only the QEMU leg is open. Host executor e2e landed; the board run of 2026-10-06 shows
      the overrun as timer-overrun and jitter verdicts and markers, not as
      `max-latency-runtime`: I6. native_sim fixture landed:
      `violation_channel_e2e`, one latency verdict after arming; QEMU
      open here. The board run moved 2026-10-24 to
      [phase-512](phase-512-on-hardware-measurements.md) H6.)
- [x] I6: a timer-fired callback that overruns its `max_latency` produces a
      `max-latency-runtime` verdict, on the host test and on the S32K344.
      (Host half landed in #1750: the trigger-miss sweep times each timer
      dispatch and charges it through `attribute_latency`, test
      `t4_a_timer_overrun_beside_an_idle_subscription_is_judged`, issue
      1724. Board half met 2026-10-26: "The board halves, 2026-10-26".)
- [x] I7: the reporter neither overflows the main stack nor costs the spin
      thread a tick per verdict; the island's 24 KiB workaround can go.
      (Host: frame ~1 KB (issue 1726); reports queued, coalesced and sent off
      the judged tick, cost not charged as jitter (issue 1727). The 16 KiB
      re-try on the board is the island's.)
- [x] I8: no rate verdict in the first window after arming on a stream
      publishing at its declared rate. (Host half landed in #1750: the first
      window re-anchors on the first new sample, test
      `a_stream_at_its_rate_armed_mid_period_is_not_judged_slow`, issue
      1725. The board rerun of 2026-10-09 still read 9999 of 10000: I9.
      Board half met 2026-10-26: no rate verdict in nine armed records.)
- [ ] 446-W7: the island's 21 declared parameters build and boot with
      parameter services on native, native_sim and MR-CANHUBK344; the demo
      reads VERDICT PASS on both islands, and the board's heap use with the
      services is recorded under "Cross-repo" above.
- [x] I9: a stream at exactly its declared rate is not judged slow in any
      window, however late the closing tick runs. (Host half here: the count
      is judged against floor(min_rate * window), test
      `a_stream_at_its_rate_checked_late_is_not_judged_slow`; the board
      rerun is the island's W4. The operator's 29773 of 30000 was two ticks
      dropped under the log-floor stall, measured on the board; not open.
      Board rerun 2026-10-26: no rate verdict in nine records; the exact-rate
      case is the host test's, see "The board halves, 2026-10-26".)

## Open issues that name this phase

Open issues whose `related:` names this phase (listed 2026-10-12, so the
link runs both ways). Listing one here records the link, not a claim that
this phase will fix it.

- [issue 0902](../issues/archived/0902-action-goal-completion-is-variable.md) — action goals complete between 20 % and 90 % of the time on the same build, with no session expiry and no fault to exp … (homed here 2026-10-13, from the archived phase-433; moved to [phase-512](phase-512-on-hardware-measurements.md) H8 on 2026-10-24, because its open step is a board run)
- [issue 0880](../issues/0880-tcm-unused-while-sram-exhausted.md) — 192 KiB of tightly-coupled memory sits at 0 % while SRAM is exhausted — the Zephyr images place nothing in ITCM or DTCM

## Issues homed here (2026-10-17)

[issue 1799](../issues/archived/1799-boot-report-arena-exhaustion-on-mr-canhubk344.md) moved to [phase-512](phase-512-on-hardware-measurements.md) H7
on 2026-10-24.

Homed when every open issue was given a phase. Each issue has a `**Home:**` line pointing here.

| issue | why it belongs here |
| --- | --- |
| [#1806](../issues/1806-zenoh-constrained-link-unvalidated-on-full-autoware-graph.md) | The constrained-link zenoh changes from phase-473 have never run against the full Autoware graph on the island. |
| [#1822](../issues/1822-can-link-priority-mapping-blocked-by-join-size.md) | The zenoh-rs CAN link's priority mapping cannot be switched on because a QoS `Join` exceeds the CAN FD MTU. It waits on upstream. |
