# RFC-0107 -- One executor core per image, one thin dispatcher per tier

**Status:** Draft (2026-10-19). Proposal only; nothing here is implemented.

**Follows:** issue 1970 (each tier's executor sized from its own members,
PR #2404) and issue 1971 (what is left is per executor: the remap table and
the arena floor). A first fix for 1971 (per-executor remap COUNT, PR #2411)
is parked as a draft: this RFC replaces its remap half with a table in
flash and keeps its arena half.

## Summary

A tiered image opens one `Executor` per tier, and every executor carves its
own copy of tables that describe the IMAGE, not the tier: the remap table,
the scheduling-context tables, the contract-violation ring, the parameter
services' home, an extra node slot, the arena's base overhead and (since the
arena is floored) up to 8 KiB of unused arena. On the safety island's
two-tier split that is +14,936 B of DTCM for a tier that hosts one node,
and the image misses its 8 KiB DTCM guard.

Three changes, each its own PR:

1. **Launch remaps become one `const` table in flash**, emitted by codegen
   and shared by every executor; a RAM table exists only for `--ros-args -r`
   rules on a target with process arguments. Layering is unchanged: launch
   rules are authoritative, argv rules the fallback.
2. **`ARENA_FLOOR` applies only where demand is unknown.** A derived image
   or tier arena is its model, unfloored. The upper-bound assertions in
   `executor/arena.rs` and the compile-time `arena_oracle` remain the guard.
3. **One shared executor core per image plus one dispatcher per tier.** The
   core holds what describes the image (node tables, sessions, parameters,
   scheduling contexts, monitor tables, the arena); a dispatcher holds what
   one tier's thread needs to run (its ready set, its callback slots, its
   spin state). Each tier keeps its own thread and priority, so preemption
   between tiers is unchanged.

## Measurements (safety island, S32K344, nano-ros main d5fe58a518)

The island repo's board build, not flashed. Two shapes: one tier `mrm` (as shipped),
and the split with `estop` = `mrm_emergency_stop_operator` alone.

| image | DTCM used | free | 8 KiB guard |
| --- | ---: | ---: | --- |
| one tier | 112,464 | 18,608 | passes |
| two tiers | 127,400 | 3,672 | FAILS (link refused; measured with the guard lowered) |

What the second tier adds (ELF symbol diff, both shapes):

| symbol | one tier | two tiers | delta |
| --- | ---: | ---: | ---: |
| `__nros_tier_executor_storage` (one `CppContext` per tier, incl. the 1,800 B `Executor` value) | 2,672 | 5,344 | +2,672 |
| `__nros_tier_backing_0` / `_1` | 44,632 | 19,656 + 36,728 | +11,752 |
| `__nros_tier_thread_1` | 0 | 272 | +272 |
| contract monitor tables (`__nros_mode_storage_t1`; the rest only moves) | 0 | 240 | +240 |
| **total** | | | **+14,936** |

Per-region breakdown of each backing. Unit sizes probed on thumbv7em with
the island's features (`remap_rule` 404, `node_record` 324,
`concrete_session` 528, `extra_session_id` 168, `node_sched_entry` 140,
`group_sched_entry` 176, `parameter_slot` 152, `param_home` 560,
`sporadic_state` 48, `sched_context` 28, `callback_meta` 24, `violation` 24,
`alive_slot` 24, `active_group` 48; `Executor` 1,800). Sums match the
symbols to within 9 B of alignment padding.

| region | count | one tier | `estop` | `mrm` (split) | split - one tier |
| --- | --- | ---: | ---: | ---: | ---: |
| arena | derived | 25,724 | 8,192 (floored; model 7,500) | 20,272 | +2,740 |
| callback entries | cbs | 336 | 72 | 264 | 0 |
| SC bindings + slot tags | cbs | 70 | 15 | 55 | 0 |
| group sched table | cbs | 2,464 | 528 | 1,936 | 0 |
| sched contexts + sporadic + atomic + alive | sc = 5 (image-wide) | 560 | 560 | 560 | +560 |
| remap table | 16 (fixed) | 6,464 | 6,464 | 6,464 | +6,464 |
| node records | nodes | 1,296 | 648 | 972 | +324 |
| extra sessions | nodes | 2,112 | 1,056 | 1,584 | +528 |
| extra session ids | nodes | 672 | 336 | 504 | +168 |
| node sched + dispatch + component + active groups | nodes | 832 | 416 | 624 | +208 |
| violation ring | 8 | 192 | 192 | 192 | +192 |
| parameter store | params | 3,344 | 608 | 2,736 | 0 |
| parameter services' home | 1 per store | 560 | 560 | 560 | +560 |
| padding | | 6 | 9 | 5 | +8 |
| **backing** | | **44,632** | **19,656** | **36,728** | **+11,752** |

Reading the delta: what is PARTITIONED between tiers (callback slots,
parameters) costs nothing extra. What is DUPLICATED costs: the remap table
(6,464), the arena (2,048 base overhead + 692 floor), the image-wide SC
tables (560), the violation ring (192), the parameter home (560), and one
more node slot per executor (1,228: each tier opens its own executor node,
`tier_node`, so `nodes` is members + 1).

Two findings outside the tier question:

- **Extra sessions are dead weight on a one-RMW image.** `extra_sessions`
  carves 528 B per node slot for nodes bound to a second RMW. The island
  has one RMW: 2,112 B (one tier) / 2,640 B (split) carved and never
  pushed. Sizing it from the descriptor's backend count (0 when one RMW)
  is independent of this RFC and worth its own issue.
- **Each executor has its own arena.** The arena is a region of the
  executor's backing (`carve`), bump-allocated by that executor alone.

## 1. Launch remaps: one const table in flash

### Today

`RemapRule` (404 B: node name 64, namespace 64, from 128, to 128, tier) in
`executor::spin`, carved `MAX_REMAPS` = 16 times in every backing.

| who | reads / writes | road |
| --- | --- | --- |
| `Executor::declare_remap` | writes a `Launch` rule | C/C++ generated entry (`shared/declare_calls.jinja`, one `nros_cpp_declare_remap` per `<remap>`, per tier on that tier's executor); hand-written C/C++ |
| `ros_args::install_argv_remaps` | writes `Argv` rules (all or nothing) | `nros::init_with_args` / `Context::create_executor{,_in}` (Rust, std), `nros_cpp_install_argv_remaps` (`rclcpp::init(argc, argv)`) |
| `Executor::resolve_entity_name_for` | reads both tiers via `tier_rules` -> `names::resolve_name_layered` | C and C++ entity registration |
| `Executor::session_and_remaps_at_mut` | lends the table to `NodeHandle::new` | Rust `create_node` road; `NodeHandle` calls `names::remap_layered` |
| `ros_args::argv_fallback` | reads `Argv` rules | Rust component sink; its launch tier is already a STATIC slice, `RuntimeCtx::remaps: &[(&str, &str)]`, emitted by `entry.rs.jinja` |

The Rust road already does what this RFC proposes: its launch remaps are a
`&'static` slice in flash, and only argv rules touch the executor table.

### Proposed

- **Codegen emits the launch table once per image, in flash.** The C and C++
  entry packs emit
  `static const nros_launch_remap_t __nros_launch_remaps[N] = {{node, ns, from, to}, ...};`
  (pointers to string literals: 16 B per rule plus the strings, in
  `.rodata`) and hand it to the runtime once, before any tier setup
  (`nros_cpp_set_launch_remaps(table, n)` / the C twin). Every tier's
  executor reads the same table. No `declare_remap` calls are emitted.
- **The executor stores a borrow, not a copy.** `Executor` gains
  `launch_remaps: &'static [LaunchRemap]` (8 B). `tier_rules(.., Launch, ..)`
  iterates it; nothing else changes in `resolve_name_layered`, so launch
  stays authoritative and argv the fallback by construction (two
  parameters, never interleaved).
- **Argv rules keep a RAM table, only where argv exists.** The carved
  `remap_table` becomes the argv table: `ExecutorSizing::argv_remaps`,
  default 16 on a target with process arguments (`target_os != "none"`,
  NuttX included: its task dispatch passes `argc/argv`), 0 on bare metal
  (Zephyr, FreeRTOS, ThreadX-on-hardware). `install_argv_remaps` refuses by
  name when 0. Hosted std images may instead keep argv rules on the
  `Context` (heap) and lend them to every executor, which removes the RAM
  table there too; either is compatible.
- **`declare_remap` stays as API** for hand-written C/C++ that declares at
  runtime. It writes the RAM table as a `Launch` rule; on bare metal its
  capacity is a stated knob (`NROS_EXECUTOR_RUNTIME_REMAPS`, default 0 when
  the entry is generated). A full table stays a named refusal.

Platforms: the change is in `nros-node`, the two entry packs and the two
FFI crates; no board runner touches remaps. Zephyr / FreeRTOS / NuttX /
ThreadX tier runners need no change (the table is installed before the
runner spawns tiers). Native/hosted keeps argv behaviour bit for bit.

Savings: 16 x 404 = 6,464 B of RAM per executor on bare metal (12,928 B on
the island split), for 16 B + strings of flash per launch rule (0 on the
island: its launch states no remap). Hosted: unchanged unless argv moves to
the `Context`.

## 2. Arena floor: only where demand is unknown

Rationale today (`nros-node/build.rs`, the comment above `ARENA_FLOOR`,
issue 1190): the floor is POLICY, not a margin for model error. Issue 1190
showed `.max()` only raises and did not hide its bug (the action-client
term's margin did); raising the floor to one subscription entry was
rejected because 8,192 already holds 128 timers. "A floor is safe exactly
when the per-entry model is right, which is why the fix went there and why
the model's terms are now asserted to be upper bounds."

So the floor buys nothing where the model ran: the model's terms are
asserted upper bounds (`executor/arena.rs`: per-backend `const` assertions
that every concrete entry fits its term, `PUBSUB_ENTRY_STRUCT` above every
`Sub*Entry`, the action terms above `size_of`), `arena_oracle` refuses at
compile time a stated arena below `arena_model::REQUIRED`, and a short
arena is still `NodeError::BufferTooSmall` naming the registration, not
corruption. Proposal (as implemented in draft PR #2411):
`derived_arena = model_required` when every entity count is declared, the
floored worst-case budget otherwise; a tier's arena is its own model,
unfloored. Island: `estop` 8,192 -> 7,500.

A shared arena across tiers would additionally save `ARENA_BASE_OVERHEAD`
(2,048 B) per extra tier. It is part 3 (the arena belongs to the core), not
a separate change: on its own it would need a lock, because each executor
allocates from its arena at registration and frees on entity destroy.

## 3. Shared core, per-tier dispatcher

### Classification

| state | per tier by nature | shareable (image) | note |
| --- | --- | --- | --- |
| ready set, spin cursors, park/wake state, current-dispatch | yes | | the dispatcher (most of the 1,800 B `Executor`) |
| callback entries, slot tags, SC bindings, group sched rows | yes (partitioned by cbs) | | already sized per tier (1970) |
| thread, stack, tier SC | yes | | stays per tier: priority and preemption unchanged |
| arena | partitioned by entity | yes | one region, entries tagged to their dispatcher |
| node records, node sched, dispatch, component, active groups | | yes | indexed by node; each tier filters its own |
| sessions (primary + extra) | | yes | the primary is ALREADY shared by the Zephyr runner (one session, borrowed executors) |
| sched-context table + sporadic + alive | | yes | image-wide `MAX_SC`; each SC owned by one tier |
| parameter store + services' home | | yes | one store, one home |
| launch remaps | | yes (flash) | part 1 |
| contract monitor rows, violation ring | | yes | per-tier monitor rows stay with their tier |

### Shape

- `ExecutorCore` (one per image, one backing): sessions, node tables,
  arena, SC tables, params store + home, violation ring, launch-remap
  borrow, argv table. Carved once by `carve_core(backing, CoreSizing)`.
- `Dispatcher<'c>` (one per tier, small backing): borrows the core; owns
  its callback-slot tables (`cbs`), ready set and spin state.
  `spin_once` / `spin_period` move to `Dispatcher`.
- `Executor` stays as the one-tier facade (`core + 1 dispatcher` in one
  backing), so every non-tiered caller and the Rust `Executor` API keep
  their shape. `open_in(backing, sizing)` is unchanged for them.
- FFI: `nros_cpp_init_tier` / `nros_cpp_executor_open_over_session_tier`
  become `nros_cpp_core_init` + `nros_cpp_dispatcher_open(core, tier)`.
  Generated header: `NROS_CPP_CORE_BACKING_SIZE` and
  `NROS_CPP_DISPATCHER_BACKING_SIZE_<i>` replace
  `NROS_CPP_TIER_BACKING_SIZE_<i>`; the entry packs emit one core backing
  and one dispatcher backing per tier.
- Tier runners: Zephyr (`zephyr_run_tiers.c`, `entry_tiers.rs`) already
  opens one session and spawns borrowed executors; it opens dispatchers
  instead. FreeRTOS (`freertos_run_tiers.c`), NuttX (`nuttx_run_tiers.c`),
  ThreadX (`nros-board-threadx/src/entry.rs`) and native
  (`nros-board-linux` `run_tiers`, threads) today keep one whole-image
  executor per tier; they move to core + dispatchers in their own PRs and
  gain the most.

### Concurrency

| state | written after boot by | rule |
| --- | --- | --- |
| node tables, sessions table, SC table layout, launch remaps, argv table | nobody (setup is serialized at boot, issue 144) | read-only after boot; no lock |
| arena allocator | registration / destroy of an entity | boot-only on generated entries; a runtime create/destroy takes a core lock (critical section on single core) |
| a callback's arena entry and buffers | its own dispatcher | single writer (the entry is tagged to one dispatcher) |
| sporadic / alive state of an SC | the tier that owns the SC | single writer; reads by the supervisor use the existing atomic road |
| parameter store | parameter services, user `set_parameter` from any tier | lock (critical section) around the store; it is not on the dispatch path |
| violation ring | any tier's monitor | lock, or one ring per tier (192 B each) |
| session (zenoh-pico) | every tier, publish / spin-read | unchanged: today's shared-session discipline |

Multicore: dispatchers may run on different cores. Everything marked
read-only after boot is safe as is; the three locked items need a real
spinlock (not a critical section) there. Partitioning tiers across cores by
load is a separate, later phase.

### Expected DTCM (island)

| step | one tier | two tiers | guard (two tiers) |
| --- | ---: | ---: | --- |
| main today | 112,464 | 127,400 | fails by 4,520 |
| 1: remaps in flash | 106,000 | 114,472 | passes (16,600 free) |
| 2: no floor on derived arena | 106,000 | 113,780 (measured 113,784 with #2411) | passes |
| 3: shared core | 106,000 | about 107,700 to 109,200 | passes |
| (separate) extra sessions sized to RMW count | -2,112 | -2,640 | |

Step 3's estimate: the second tier stops paying the SC tables (560), the
violation ring (192), the params home (560), the extra node slot (1,228)
and the arena base overhead (2,048) = 4,588 B; what remains is one
dispatcher (the per-tier part of the 2,672 B context, at most that), the
thread (272) and its monitor rows (240). The range is the unknown share of
the `Executor` value that is dispatch state.

## Migration plan (each step its own PR)

1. **Arena floor** only for unknown demand (the arena half of #2411).
   Tests: tier and image arenas equal their model; floored budget unchanged
   for an undeclared image.
2. **Launch remaps in flash**: `LaunchRemap` + executor borrow; entry
   packs emit the table; `declare_remap` kept on the RAM table; argv table
   sized 0 on bare metal. Goldens for both entry packs; the existing
   layering tests (`ros_args`, `node.rs`) must pass unchanged.
3. **Extra sessions sized from the backend count** (separate issue).
4. **Core/dispatcher split inside `nros-node`**, `Executor` as facade; no
   FFI change yet. Unit tests: one-tier backing size unchanged.
5. **FFI + Zephyr runner + entry packs** to core + dispatchers; island
   two-tier measured.
6. **FreeRTOS, NuttX, ThreadX, native runners**, one PR each.

## Risks

- **Codegen-emitted remaps must be installed before any tier setup.** The
  order is load-bearing (rclcpp constructors register entities
  immediately, issue 0255). The entry packs set the table before the
  runner is called; a test asserts the call precedes every setup.
- **Hand-written C/C++ that calls `declare_remap` on bare metal** gets a
  refusal by default. Named, and the knob restores it.
- **Unfloored arenas are as right as the model.** A model term that is not
  an upper bound now fails at registration (`BufferTooSmall`) instead of
  being masked on small images. The assertions in `arena.rs` are what
  stands behind this; a backend added without them is the exposure.
- **Shared core concurrency.** The parameter store and violation ring gain
  a lock; everything else must be proven read-only after boot. A debug
  assertion that freezes the core after the last tier setup catches a
  late writer.
- **API churn**: tier FFI and generated header names change in step 5;
  the one-tier road does not.
