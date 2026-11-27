# Phase 573 — a callback that calls back into its executor is sound

**Status (2026-11-25). OPEN: W1, W2, W2b, W3, W4 and W5a-W5e are done, which resolves issue 2675; W6 is open.** Fixes
[issue 2675](../issues/archived/2675-callback-reentry-aliases-executor.md) by amending
[RFC-0108](../design/0108-executor-preemption-levels.md) ("Re-entry from a
callback"). Ahead of [phase-572](archived/phase-572-rust-component-graph-query.md),
whose W2 waits on it. Owner: nros 241 for W2b, W3 and W4; nros 133 for
W5d (done), and for RFC-0108 D9 and the review of each item.

## Why

A C or C++ callback may call back into the executor it runs on: timer
cancel/reset, releases, parameter calls, graph queries. That is rclc and
rclcpp behaviour users rely on, and the recursive executor lock exists to
allow it. But on the owned pass, Rust holds a live `&mut` to the executor
across the callback in two places, and each re-entering entry derives a
second one. That is undefined behaviour under Rust's aliasing rules.

- **The entry.** `spin_once(&mut self)` is a protected `&mut` for the whole
  pass. The C/C++ spin entries (`rclc_executor_spin*`, `nros_cpp_spin*`)
  reach it through the `DerefMut` of their locked reference.
- **The core.** `Executor::pass` re-derives its `s: &mut Self` from `this`
  after a callback only when a callback WINDOW opened, which is a shared or
  band pass whose lock has a thread id. In an owned pass, or a shared pass
  with no thread-id lock, `s` stays live across `try_process` and is used
  after it.

The windowed shared and band passes are already right. They are what the
FreeRTOS multi-level runner and the RTIC SRP backend (142's W10) use.

The ruling (2026-11-17) is to keep the behaviour and make it sound. Refusing
re-entry from callbacks was rejected: it would take timer cancel, releases,
parameter calls and graph queries out of C and C++ callbacks.

## Work items

### W1 — The design, as an RFC-0108 amendment

- **Status:** DONE (2026-11-17, #4150: RFC-0108 D9)

Who holds what across a callback, written down:
- the dispatch loop holds no reference into the executor across a callback,
  and re-derives from `this` after EVERY callback in every mode, windowed or
  not;
- every spin entry reaches the pass through a raw pointer;
- one helper derives the executor for every FFI entry, so no entry hand-rolls
  the derivation.

### W2 — The dispatch core is sound

- **Status:** DONE (2026-11-17, the W2 PR: core, entries, timers; gate `miri-executor-reentry`)

`pass` re-derives after every callback and every tick. The owned entry is
`unsafe fn spin_once_raw(this: *mut Self, ..)`, which `spin_once(&mut self)`
and the FFI spin entries call. The FFI spin entries pass the locked
reference's pointer and never create a `&mut` that outlives the call.

Acceptance: a pure-Rust harness re-enters the executor from a callback once
per entry kind (timer cancel and reset, a release, a parameter call, a graph
query, a service-client check). It reaches the executor through a raw pointer,
as the FFI does. Run under Miri with Tree Borrows, it passes on the new core
and FAILS on the old one, which is the negative control. The dispatch core is
testable with no FFI and no RMW backend.

As built: the harness is `executor::tests::reentry`. Its cases are a timer
cancelling itself, a timer resetting itself, a timer reading its executor,
and a timer releasing another entry. The first commit shows it failing under
Miri on the old core, at the protected `&mut self` of `spin_once`. Three more
findings followed, each from Miri: the stored `&mut` arena (now `ArenaRef`),
the timer's own entry reference held across its callback (re-derived), and
`pass`'s windowed-only re-derive. The last one's negative control fails at
`s.finish_dispatch(i)`, right after the release case's callback. The Miri gate
runs in 11 s cold. A parameter call, a graph query and a service-client check
read or write executor fields that rule 1 covers; their per-kind cases move to
W2b with the other entry kinds.

### W2b — No entry holds its own reference across its callback

- **Status:** DONE (2026-11-22)

RFC-0108 D9 rule 4 for every entry kind other than timers. Each
`*_try_process` in `arena.rs` holds `&mut Entry` across the user callback, and
under Tree Borrows that breaks when the callback writes the same entry's bytes.
Audit every FFI entry that can write a running entry: an action server's
`send_result` or feedback from inside its own goal callback, a guard condition
triggered from its own callback, a service reply, and the subscription ring
loop that holds `ring` across callbacks. Fix each site by re-deriving after
the callback, or by deferring the write the way a windowed timer cancel
already does.

Acceptance: a Miri case per kind in `executor::tests::reentry`, run by
`just check miri-executor-reentry`.

One pull request for the remaining rows, each row's failing Miri case (where
it has one) committed before its fix. A row with no FFI-reachable writer gets
a passing case, shown sensitive by a mutation measured and reverted.

| Entry kind | Status | Notes |
|---|---|---|
| Raw action server | Fixed | `ActionServerCore::try_handle_cancel` is split into `recv_cancel` (selects, and copies what the handler needs) and `reply_cancel` (finds goals again by id); `action_server_raw_try_process` takes a fresh `&mut` from the entry pointer for each step. Cases: the accepted hook completes its goal, the accepted hook publishes feedback, the cancel callback completes its goal. It also fixes a logic bug: the cancel handler ran inside a loop over `active_goals`, so a goal completed from it moved the goals behind it, and the answers were applied by position to the wrong goals (`a_cancel_answer_lands_on_the_goal_it_was_given_for`, a native test). Cost on thumbv7m (opt-s, nros-c): the dispatch is +440 B. |
| Typed action server | Fixed | A Rust callback reaches its server through `ActionServerHandle` and a raw executor pointer. The cancel callback completing its goal failed under Miri at `as_complete_goal`'s reborrow, against the protected `&mut self` of `try_handle_cancel`. The dispatch now has the raw row's shape: `recv_cancel` / `reply_cancel`, and the typed `try_accept_goal` split into `recv_goal` and `finish_goal`, with a fresh `&mut` to the server from a raw place per step and each callback borrowed through its own field. The `as_*` helpers reborrow only the server. Cases: the cancel callback completes its goal; the goal callback completes another active goal (also UB on the old dispatch). Not changed, and older than this row: a goal request that is taken and then fails to decode is left unanswered. Cost: nros-c (thumbv7m, opt-s) byte-identical, since C uses the raw dispatch; host release, the typed dispatch and its eight helpers, 4,821 B to 4,131 B. |
| Service and action client callbacks | Fixed | Wider than re-entry. `ServiceClientCallback` and `ActionClientCallback` held a pointer made as `&mut (*entry).field as *mut _`, a child of a temporary reference, which the dispatcher's own write to the entry disabled. So the second request through the safe handle was UB even from outside any callback. The fix takes a raw place (`addr_of_mut!`); the guard flag's pointer follows suit. The typed action-client dispatch also re-derives the entry per step, and the service-client info closure holds the entry pointer instead of `&hdr.handle`. Cases: a reply that sends the next request, a goal response that asks for its result, and each handle surviving its reply dispatch. Cost: none (host release, 13,752 B for the ten client dispatch monomorphs, before and after). A raw client's response callback that re-sends on its own client (the chained-request pattern, writing the running entry) passes; a post-callback write of `pending` fails, on both the raw and the typed dispatch (measured, reverted). |
| Service server | Sound as built | No FFI path writes a running server's entry: the reply goes through the `resp` pointer the dispatch hands out, `rcl_service_fini` from the callback defers, and a nested spin returns `REENTRANT`. Cases: a callback reads its own handle; a callback writes its reply, then sends on a client (`service_client_entry_mut` retags the whole arena). Measured: a post-callback write to the reply buffer still passes, so the whole-arena retag does not freeze it. The typed server's protected `&mut` arguments are reachable only from a Rust closure, which borrowck keeps from the executor. |
| Subscription drain | Sound as built | `entry` is used again on the next loop iteration, but no FFI path writes a running subscription's entry. Case: a buffered C subscription with three queued samples whose callback reads its own handle and sends on a client each time. |
| Guard | Sound as built (negative control) | Atomic flag, swapped before the callback; the entry is not used after it. Case: a callback re-triggers its own guard and sends on a client. Measured: a post-callback read of the flag through the entry fails, so the case is sensitive. |

### W3 — One derive helper, and the class sweep

- **Status:** DONE (2026-11-23)

`get_executor`, `get_executor_from_ptr`, `cpp_ctx_checked`, `cpp_ctx_locked`
and the `CLocked` / `Locked` lookups go through one helper, measured at
2026-11-17 as 224 call sites in nros-c and nros-cpp. A sweep lists every
executor-deriving FFI entry and proves each one goes through the helper or
returns `RCLC_RET_REENTRANT` / `Reentrant` from a callback.

Acceptance: the sweep is a gate, so a new entry that derives the executor
another way fails it.

One known site of W2b's client pattern on a different object belongs here:
`Executor::session_ptr` (`spin.rs`) returns `&mut *self.session as *mut _`, a
pointer derived from a temporary reborrow. A caller that keeps it while the
executor writes the session through its own field holds a disabled tag, the
same defect the client handles had. The sweep covers it: the fix is a raw
place, and a Miri case keeps the pointer across a `spin_once`.

As built:

- **Every derive takes the lock.** Nine FFI entries cast a stored pointer to
  the executor and took no lock: the C action client's arena lookup (five
  calls go through it), `rcl_timer_cancel` and `rcl_timer_reset`, and six C++
  subscription creates. Each now goes through its crate's derive. Measured
  today: 212 helper calls, not 224.
- **One refusal per crate.** A callback may make any call stock rclc and
  rclcpp allow. A call refuses (REENTRANT) only when it would block, spin the
  executor again, or release an entry the running pass may still touch. C
  refuses through `refuse_in_dispatch`, which its 12 refusing calls use. C++
  spins take `RawDispatchGuard`, and a refusing call that does not spin calls
  `CtxRef::refuse_in_dispatch`.
- **The sweep is a gate, `check-ffi-executor-derive`.** A cast to the
  executor, a cast of a stored pointer to the C executor header, and a read of
  `in_dispatch` are each allowed only in a listed function, with its reason.
  Every spelling counts, and a stale entry fails. It covers 66 sites. Putting
  back the timer casts fails it, naming both lines. Nothing reads
  `in_dispatch` outside the refusals.
- **C++ `wait_for_service` slept holding the executor's lock.** Inside a
  callback it sleep-polls (issue 2055), and it held the lock for the whole
  wait, so no other thread could enter the executor until it timed out. It now
  drops the lock across each sleep
  ([issue 2843](../issues/archived/2843-cpp-blocking-call-sleep-polls-in-callback.md)).
  The test drives the C++ API over a stub backend with the lock installed,
  and it fails on the code before the fix (the second thread waited 553 ms).
  Whether C should sleep-poll too, where it refuses today, is
  [issue 2848](../issues/archived/2848-c-wait-for-service-in-callback-parity.md).
- **Review fixes.** The C action client's poll paths took the core through the
  locked derive and then ran the user's callbacks still holding it; each step
  now copies the reply out and gives the lock back first. The C++ wait's own
  spin (outside a callback) now marks the context in a callback, as
  `nros_cpp_spin_once` does. The gate counts the executor by every name it
  goes by. A C call from another thread during a callback still gets a
  misleading answer (NOT_INIT from the action client, REENTRANT from the
  executor-wide flag):
  [issue 2855](../issues/2855-c-cross-thread-call-during-dispatch-misleading-code.md).
- **The header class has no Miri case.** A C call that reborrows the executor
  header while `rclc_executor_spin_some` is mid-pass cannot run under Miri:
  nros-c's executor init reaches platform C symbols Miri cannot call. The
  gate's header-cast rule is the guard: a new spinning entry is a new
  function, so it is reviewed before it can hold a header reference across its
  callbacks. A Rust-only mock platform for nros-c host tests would make the
  real Miri case cheap.
- **`session_ptr` needed more than a raw place.** The owned session lived
  inline in its owner, so any pointer to it derived from the owner, and the
  owner's next `spin_once` disabled it. Under `alloc` the owned session is now
  in its own allocation (536 bytes on x86_64), and owner and borrowers are
  equals. If that allocation fails the session stays inline and is never
  shared: `session_handle` returns `None`. Without `alloc` it stays inline
  and shared until it can be carved from the executor's backing
  ([issue 2845](../issues/2845-owned-session-from-executor-backing.md)).
  Cases: `a_session_pointer_survives_its_owner_spinning` (UB before) and
  `an_inline_session_is_never_shared`, both in `miri-executor-reentry`.

### W5a — A guard condition cannot outlive its executor

- **Status:** DONE (2026-11-19)

[Issue 2727](../issues/archived/2727-guard-condition-outlives-executor.md).
A `GuardCondition` kept past its executor triggered through a freed
`WakeCtx`, and could outlive the backing its flag lives in, from safe code.
The handle is now `GuardCondition<'s>`, borrowing the executor's backing,
and owns an `Arc<WakeCtx>` clone of the same size as the pointer it
replaces.

Acceptance:
- A `Weak<WakeCtx>` taken before the executor's `Arc` drops still upgrades
  afterwards.
- A `compile_fail` doctest refuses a handle kept past its backing. Its
  compiling twin is run and counted by `just check node-std-tests`.
- A mutation of each half fails its test.

The use-after-free itself showed only under AddressSanitizer, which no lane
runs: [issue 2728](../issues/archived/2728-no-lane-runs-addresssanitizer.md).

### W4 — The multi-level and RTIC case

- **Status:** DONE (2026-11-25)

A callback that re-enters, for example with a timer reset, while a
higher-priority level preempts it. The shared and band passes, and the RTIC
SRP backend (whose ceiling is not held across a callback), keep W2's
property. 142's W10 adds the same shape on its backend.

Acceptance: the W2 harness runs that case through `spin_band_once` with two
bands.

As built: `executor::tests::reentry_bands`, which `miri-executor-reentry`
runs because its filter matches the module name. Two band workers share one
executor with a lock that opens windows. The less urgent band's timer is in
its callback window when the urgent band's worker runs a pass on another
thread. The urgent timer resets the timer the other band is still
dispatching, and then the preempted callback resets itself. Both writes take
the lock and derive the executor from the raw pointer, as a C callback's
`rcl_timer_reset` does. The case passes under Miri with Tree Borrows. Its
sensitivity was measured and reverted: without the re-derive after each
callback in the ready-entry loop of `pass`, Miri reports a write through a
disabled tag in the urgent band's pass. Removing the timer sweep's re-derive
changes nothing here, because band timers dispatch through the ready-entry
loop. The RTIC SRP backend (142's W10) is not on main yet. It keeps this
property only if it dispatches through the same band pass, so its review
should check that.

### W5b — A C entity's fini releases its executor entry

- **Status:** DONE (2026-11-19)

`rcl_subscription_fini` left its arena entry registered, so a fini without
`rclc_executor_remove_subscription` left the executor deserialising into the
caller's `msg` storage, and dispatching into the finalised struct. The same
held for every C entity the executor registers: timer, service, client,
action server and action client.

Each fini now releases the entry when the entity is still added to a live
executor, through the same `detach_*` core the `rclc_executor_remove_*`
calls use (those are now argument checks over it). From a callback its
executor is dispatching, a fini returns `RCLC_RET_REENTRANT` and changes
nothing, as remove does. A fini after `rclc_executor_fini` has nothing left
to release and succeeds: it reads the executor's state, never its zeroed
`_opaque` (the old subscription fini did read it).

Acceptance:
- `entity_remove_cycles.c` and `action_remove_cycles.c` run the create / add
  / fini loop with NO remove, and it must out-run the handle table and the
  arena with the backend's live count back at its baseline every cycle.
  Before W5b these were the negative controls, which exhausted the executor.
- A fini after the executor's fini succeeds; a timer finalising itself from
  its own callback is refused and stays registered.

### W5c — The C++ lifetime contract is written where it is used

- **Status:** DONE (2026-11-19, #4338)

`create_wall_timer<C, &C::m>(..., C* self)`, the other `create_timer*`
forms and `create_guard_condition(..., context)` keep `self` / `context`
in the executor entry, so the owner must outlive the registration and must
not move while it is registered. `node.hpp` says so at each of them, with a
debug-build check where one is cheap.

Acceptance: each such declaration states the contract; any debug check has a
test that trips it.

### W5d — A sized C subscription registration refuses an undersized buffer

- **Status:** DONE (2026-11-19, `21dd86d85a`; user ruling 2026-11-19: option B). As built: `message_size`, appended to the generated descriptor with a `_Static_assert` in the C pack; `rclc_executor_add_subscription_sized` and `rclc_executor_add_subscription_with_context_sized`; ledger rows; `tests/run/sized_subscription.c`. The descriptor is cbindgen's, so there is no hand mirror to keep in step.

`rclc_executor_add_subscription[_with_context]` take rclc's exact arguments,
so the executor deserialises into `msg` with no size to check it against.

- The generated message descriptor gains `message_size` (the type's
  `sizeof`), appended.
- `rclc_executor_add_subscription_sized` and
  `rclc_executor_add_subscription_with_context_sized` take `msg_size` and
  refuse `msg_size < message_size` with `RCL_RET_INVALID_ARGUMENT`.
- The rclc-named forms keep their signatures; their docs name the sized form
  as the checked one.
- The generated code asserts `message_size` against the struct `__init_in`
  wires.

Acceptance: the FFI-mirror gate covers the appended field; ledger rows for
the two new entry points; a test refuses an undersized buffer and accepts
an exact one.

### W5e — A lane runs AddressSanitizer

- **Status:** DONE (2026-11-20) — [issue 2728](../issues/archived/2728-no-lane-runs-addresssanitizer.md)

W5a's use-after-free showed only under AddressSanitizer, which no lane runs.
A lane that builds the C run probes and the executor harness with ASan makes
that class visible without a hand-run.

Done as `just check address-sanitizer`, run daily by host-tests.yml's `asan`
job:
- the guard-condition lifetime test;
- nros-node's lib tests with `rmw-cffi`.

Red at the commit before W5a's fix, with a probe of the old shape: a
heap-use-after-free in the wake callback. Green on main. It costs 28 s from
an empty target dir. The measurements are in the issue.

### W6 — Without `alloc`, the owned session is carved from the executor's backing

- **Status:** OPEN

[Issue 2845](../issues/2845-owned-session-from-executor-backing.md). W3 left
an inline owned session that `session_handle` still shares when there is no
`alloc`, and the owner's next write disables that pointer. Carve the session
from the caller-supplied backing, as the arena is, so its tag is the
backing's and not the executor's. A C executor, whose session is borrowed
from `rclc_support_t`, must not pay for the slot.

Acceptance: `a_session_pointer_survives_its_owner_spinning` passes under Miri
without `alloc`; every derived backing size (the C and C++ sizes headers,
`NROS_EXECUTOR_BACKING_U64S`, the Zephyr arena pairing) moves with
`ExecutorSizing::u64_len`, and a C executor's does not.

**Planned design (2026-11-27, revised after nros 133's review on #4995).**

- **Sizing: a derived fact, never a default.** `ExecutorSizing` gains
  `session: usize`, 0 or 1: the owned-session slots its backing carves. It
  is 1 exactly where the entry OPENS its own session, and 0 where it borrows
  one. So `DEFAULT` does not carve unconditionally: a borrowing entry pays
  nothing.
  - `sched_task_executor_sizing` is a literal today, and `SchedTaskSizing`
    states no ownership. It gains the fact, derived per task: the boot task
    owns (1), and a spawned task borrows the boot task's session (0).
  - The region is placed by `nros-executor-layout`'s `Counts`, like every
    other table, so a host build script that evaluates the layout for a cross
    target gets the same total. The unit (`concrete_session`) is already in
    `RegionUnits`.
- **Store.** `SessionStore` gains `Carved(NonNull<ConcreteSession>)`, a raw
  pointer carved from the backing over a range disjoint from every other
  table, so it is a sibling of their tags and not a child of the executor's.
  - The session is written into the slot at open and dropped in place when
    the executor drops.
  - Dropping is keyed on the store's VARIANT (`Carved`), never on
    `sizing.session`. A borrowing executor whose sizing happens to carry a slot
    leaves it unused and never drops through it.
- **`Heap`: retire it in W6.** That is 133's recommendation, pending the
  user's ruling. Only an
  `alloc` build with no carved slot reaches it, and `Owned` plus a refusing
  `session_handle` already covers that, so after W6 there is one owned
  shared path in every build.
- **C keeps its size; the C++ slot sits outside `rclc_executor_t`.** C++'s
  executor IS `CppContext.c._opaque` (phase-492 W4: one executor object, and
  C components get `&c`). So growing a C++ inline storage would grow the C
  executor too.
  - The C++ owned-session slot lives in `CppContext`, beside `c` and not
    inside it. The executor is carved over `c`'s backing with `session: 0`
    and handed the slot as its `Carved` pointer.
  - So `rclc_executor_t` and the C sizes header do not move.
  - `CppContext`, and with it `CPP_EXECUTOR_BACKING_U64S` / the C++ context
    size, grows by one session (536 B on x86_64).
  - No second `ExecutorInlineStorage` type is needed.
  - `open_over_session` (`open_with_session_handle_in`) borrows, so it
    carries no slot.
- **Tree Borrows, by access path.** Carving the sibling pointer is sound. The
  hazard is a reborrow that COVERS the slot:
  - `CtxRef::deref_mut` makes a whole `&mut CppContext` on every guarded
    call. Its implicit read would Freeze the session's Active tag, and the
    next session write would be UB.
  - The fix is to reach the slot only by field projection from the raw
    context pointer, never through a `&mut CppContext` that spans it. This is
    the existing carved tables' own rule, applied to one more field.
- **Derived sizes.** These move with `u64_len`:
  - `NROS_EXECUTOR_BACKING_U64S` (the Rust entry owns);
  - the C++ context size;
  - the Zephyr arena pairing;
  - the two committed NuttX buildless snapshot headers, regenerated.
  The C header does not move.

**Acceptance (revised).**

- `a_session_pointer_survives_its_owner_spinning` passes under Miri without
  `alloc`.
- A new Miri case: write the session, take `CtxRef::deref_mut`, write the
  session again. It must pass, and it must fail with the projection rule
  removed.
- `check-config-fallback-macros` passes, and a clean-checkout NuttX C and
  C++ image builds (issue 1115's trap).
- The C executor's `_opaque` and header are byte-identical before and after.
