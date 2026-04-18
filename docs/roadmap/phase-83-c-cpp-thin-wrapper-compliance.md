# Phase 83: C/C++ Thin-Wrapper Compliance — Action-Server Goal Tracking & CDR Header Centralization

**Goal**: Bring `nros-c` and `nros-cpp` back in line with the thin-wrapper
principle established in `CLAUDE.md`:

> `nros-c` must be a thin FFI wrapper over `nros-node` — delegate to Rust
> types, don't reimplement logic. New C API features must first be
> implemented in `nros-node`, then wrapped.
>
> `nros-cpp` is a freestanding C++14 library... wrapping `nros-node`
> directly via typed `extern "C"` FFI.

An audit of both crates found the action-server paths reimplement goal
queueing and state tracking that `nros-node`'s arena already owns, and
CDR header framing (`0x00 0x01 0x00 0x00` + offsets 4/20/24) is
scattered as magic numbers across both crates.

**Status**: Not Started
**Priority**: Medium — architectural cleanup, no user-visible regression on
passing tests. Blocks future multi-goal / multi-language consistency work
because the current duplication means any arena change needs parallel
updates in C and C++.
**Depends on**: Phase 77 (async action client — partially closes the
related blocking-flag issue), Phase 82 (executor-driven blocking APIs —
established the lifecycle pattern)

## Overview

### The rule

From `CLAUDE.md`:

- `nros-c` is a thin FFI wrapper over `nros-node`. It may not own state
  that `nros-node` already owns.
- `nros-cpp` is a freestanding C++14 library that wraps `nros-node`
  directly via typed `extern "C"` FFI. Header-only C++ with an
  FFI staticlib. It may not own state that `nros-node` already owns.

Two mechanical consequences:

1. If `nros-node`'s arena already tracks a goal's lifecycle, the C/C++
   layer must not track it too.
2. If `nros-serdes` already knows the CDR encoding, the C/C++ layer must
   not hardcode the bytes.

Both invariants are broken today in the action-server paths and in
scattered CDR-munging sites.

### Audit findings

Summary of the code-quality review against
`packages/core/nros-c/src/action/` and
`packages/core/nros-cpp/src/action.rs`:

| # | Site                                                                 | Severity  | Owner in nros-node                             | Status            |
|---|----------------------------------------------------------------------|-----------|------------------------------------------------|-------------------|
| 1 | `nros-cpp/src/action.rs:25–94` — `PendingGoal[]` + auto-accept queue | blocker   | `ActionServerArenaEntry::active_goals`          | Phase 83 fix       |
| 2 | `nros-c/src/action/client.rs:280–312` — static mut BLOCKING_ACCEPTED | blocker   | Executor spin + promise                        | Closes via Phase 77 |
| 3 | `nros-c/src/action/server.rs:56–90` — `goals[NROS_MAX_CONCURRENT_GOALS]` array | blocker | `ActionServerArenaEntry::active_goals`          | Phase 83 fix       |
| 4 | CDR header magic numbers (`0x00 0x01 0x00 0x00`, offsets 4/20/24)    | warn      | `nros-serdes`                                   | Phase 83 fix       |
| 5 | `nros-cpp/src/action.rs:636` — same static mut pattern as #2         | nit/warn  | Executor spin + future                         | Closes via Phase 77 |

**#2 and #5** are duplicates of the Phase 77 blocking-flag issue and will
be resolved by the Phase 77 migration to the async + arena-polled
pattern; they are tracked here only for completeness and are not
re-deliverables of this phase.

**#1, #3, #4** are independent and are the subject of Phase 83.

### #1 — nros-cpp action server buffers goals in user space

`packages/core/nros-cpp/src/action.rs:25–94`:

```rust
struct PendingGoal {
    goal_id:  nros::GoalId,
    data:     [u8; DEFAULT_RX_BUF_SIZE],
    data_len: usize,
    occupied: bool,
}

pub(crate) struct CppActionServer {
    handle:    Option<nros_node::ActionServerRawHandle>,
    pending:   [PendingGoal; MAX_CONCURRENT_GOALS],   // ← duplicate queue
    action_name: [u8; MAX_ACTION_NAME_LEN],
    _action_name_len: usize,
    type_name:  [u8; MAX_TYPE_NAME_LEN],
    _type_name_len:   usize,
    type_hash:  [u8; MAX_TYPE_HASH_LEN],
    _type_hash_len:   usize,
}
```

The goal callback trampoline auto-accepts every incoming goal and buffers
it in `pending[]`:

```rust
unsafe extern "C" fn goal_callback_trampoline(
    goal_id:   *const nros::GoalId,
    goal_data: *const u8,
    goal_len:  usize,
    context:   *mut c_void,
) -> nros::GoalResponse {
    let server = &mut *(context as *mut CppActionServer);
    let id     = *goal_id;
    for slot in &mut server.pending {
        if !slot.occupied {
            slot.goal_id  = id;
            let copy_len  = goal_len.min(DEFAULT_RX_BUF_SIZE);
            core::ptr::copy_nonoverlapping(goal_data, slot.data.as_mut_ptr(), copy_len);
            slot.data_len = copy_len;
            slot.occupied = true;
            return nros::GoalResponse::AcceptAndExecute;   // ← forced
        }
    }
    nros::GoalResponse::Reject
}
```

`nros_cpp_action_server_try_recv_goal` drains the queue back out:

```rust
for slot in &mut server.pending {
    if slot.occupied {
        let len = slot.data_len;
        if len <= buf_len {
            core::ptr::copy_nonoverlapping(slot.data.as_ptr(), goal_buf, len);
            ...
            slot.occupied = false;
            return NROS_CPP_RET_OK;
        }
    }
}
```

**What nros-node already does**: `ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>`
stores `active_goals: heapless::Vec<ActiveGoal<A>, MG>` and dispatches
the goal callback during `spin_once`. The C++ layer's `pending[]` array
is a second-order buffer of the same data, bolted on because `nros-cpp`
couldn't express "auto-accept every goal and expose them as a poll
interface" in terms of the typed `add_action_server` API.

**Consequence**: the user-supplied `goal_callback` in `nros_cpp_action_server_create`
is effectively ignored — the trampoline always returns `AcceptAndExecute`.
The C++ Node API (`create_action_server`) is a poll-based receiver that
can't reject goals, while the Rust and C APIs can. This is a silent API
divergence.

### #3 — nros-c action server tracks goal state independently

`packages/core/nros-c/src/action/server.rs:56–90`:

```rust
pub struct nros_action_server_t {
    ...
    pub goals: [nros_goal_handle_t; NROS_MAX_CONCURRENT_GOALS],
    pub active_goal_count: usize,
    ...
}
```

`nros_goal_handle_t` carries its own `status` enum
(`NROS_GOAL_STATUS_ACCEPTED`, `_EXECUTING`, `_SUCCEEDED`, ...), and the
goal trampoline + accepted trampoline in `server.rs` push state
transitions (`ACCEPTED → EXECUTING`) into this C-side array on top of
the transitions the arena entry is already doing. `active_goal_count`
mirrors the arena's `active_goals.len()`.

**What nros-node already does**: same as above — `active_goals` in the
arena entry owns the full lifecycle. The `nros_goal_handle_t` array is a
projection that goes stale whenever the arena mutates without the C
trampolines firing (e.g. arena-internal cleanup of terminal goals).

**Consequence**: `nros_action_get_goal_status(server, &uuid)` reads from
the C array, not from the arena. If the arena transitions a goal to
`Aborted` during spin and the C trampoline for that transition doesn't
fire (e.g. the transition is driven by an internal timeout), the C
caller sees stale state. This is a latent correctness bug, not just a
style issue.

### #4 — CDR header magic numbers scattered across both crates

Every C/C++ call site that publishes a ROS 2 message or reads one back
either strips or prepends the 4-byte CDR header:

```
[0x00][0x01][0x00][0x00]  representation-id + options
[........data........]    aligned payload
```

Current offenders (non-exhaustive):

- `nros-c/src/action/client.rs:418–428` — `nros_action_get_result`
  prepends `[0x00, 0x01, 0x00, 0x00]` to the result buffer.
- `nros-c/src/action/client.rs:472–486` — `nros_action_try_recv_feedback`
  strips 4-byte header, then prepends it back.
- `nros-c/src/action/server.rs:141–155` — `goal_callback_trampoline`
  strips-then-rebuilds the header around the goal data.
- `nros-c/src/action/server.rs:431–433, 480–482` —
  `nros_action_publish_feedback`, `nros_action_succeed` strip the header
  before calling the raw Rust API.
- `nros-cpp/src/action.rs:610–614, 823–826, 892, 948, 1151, 1172` —
  goal-send, feedback-read, result-read, `offset = 4 + 16` (= CDR header
  + goal UUID).

All of these encode the same two facts:

1. The header is exactly 4 bytes.
2. The header bytes are `[0x00, 0x01, 0x00, 0x00]` for little-endian,
   options-all-zero.

**What nros-serdes already does**: the `CdrWriter` in
`packages/core/nros-serdes/src/cdr_writer.rs` writes exactly this header
on `new()`, and `CdrReader::new` skips it. The canonical knowledge lives
there; the C/C++ layer is re-asserting it with magic numbers.

**Consequence**: any change to the CDR encoding (e.g., adding
extended-CDR support, bumping representation identifier) would require
hunting down every `0x00 0x01 0x00 0x00` and every bare `4` or `20` or
`24` offset in the C/C++ layer. Already a maintenance footgun; will
become a correctness bug the first time the encoding changes.

## Design

### Part A — Arena-authoritative goal state (fixes #1 and #3)

The pattern Phase 83 applies is the same one Phase 77 applied to action
clients and Phase 82 applied to service clients: **move the state into
the arena, make the C/C++ struct a view over it**.

#### nros-node changes

Add arena accessors that expose active goals to raw/FFI callers:

```rust
impl Executor {
    /// Iterate active goals for a raw action server.
    ///
    /// Callback is invoked with `(&GoalId, GoalStatus, raw_goal_bytes)`
    /// for each goal currently in the arena entry's `active_goals`. The
    /// iteration is arena-driven, so the C/C++ caller always sees a
    /// consistent snapshot.
    pub fn for_each_active_goal_raw<F>(
        &self,
        handle: &ActionServerRawHandle,
        f: F,
    ) where F: FnMut(&GoalId, GoalStatus, &[u8]);

    /// Query one goal by UUID. Returns `None` if the arena has already
    /// retired it (completed + delivered or cancelled + acknowledged).
    pub fn active_goal_raw(
        &self,
        handle: &ActionServerRawHandle,
        goal_id: &GoalId,
    ) -> Option<(GoalStatus, &[u8])>;
}
```

Both read directly from `ActionServerArenaEntry::active_goals`. Neither
mutates — transitions stay arena-driven, exactly as they are today.

#### nros-c changes

`nros_action_server_t` drops its own goal tracking:

```diff
  pub struct nros_action_server_t {
      pub state: nros_action_server_state_t,
      pub action_name: [u8; MAX_ACTION_NAME_LEN],
      pub action_name_len: usize,
      // ...
      pub callback: nros_goal_callback_t,
      pub cancel_callback: nros_cancel_callback_t,
      pub accepted_callback: nros_accepted_callback_t,
      pub context: *mut c_void,
-     pub goals: [nros_goal_handle_t; NROS_MAX_CONCURRENT_GOALS],
-     pub active_goal_count: usize,
      pub node: *const nros_node_t,
      pub _internal: [u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S],
  }
```

`nros_action_get_goal_status(server, uuid, out_status)` becomes a thin
query:

```rust
pub unsafe extern "C" fn nros_action_get_goal_status(
    server:  *const nros_action_server_t,
    uuid:    *const nros_goal_uuid_t,
    status:  *mut nros_goal_status_t,
) -> nros_ret_t {
    validate_not_null!(server, uuid, status);
    let internal = (*server)._internal_as::<ActionServerInternal>();
    let handle   = internal.handle.as_ref().ok_or(NROS_RET_NOT_INIT)?;
    let exec     = get_executor(internal.executor_ptr);
    let goal_id  = GoalId { uuid: (*uuid).bytes };
    match exec.active_goal_raw(handle, &goal_id) {
        Some((s, _)) => { *status = s.into(); NROS_RET_OK }
        None         => NROS_RET_NOT_FOUND,
    }
}
```

The goal-callback trampoline stops pushing state into the C struct. The
accepted-callback trampoline stops maintaining `active_goal_count`. Both
become pure ABI converters.

`NROS_MAX_CONCURRENT_GOALS` stays exported (it still affects
`#[repr(C)]` layout concerns on the arena side via the template parameter
`MAX_GOALS` on `add_action_server_raw_sized`), but the C struct no longer
owns an array of that size.

#### nros-cpp changes

The `PendingGoal[]` array and the auto-accepting trampoline go away.
`CppActionServer` becomes:

```rust
pub(crate) struct CppActionServer {
    handle:           Option<nros_node::ActionServerRawHandle>,
    executor_ptr:     *mut c_void,
    goal_callback:    nros_cpp_goal_callback_t,    // user-provided
    cancel_callback:  nros_cpp_cancel_callback_t,
    context:          *mut c_void,
    action_name:      [u8; MAX_ACTION_NAME_LEN],
    _action_name_len: usize,
    type_name:        [u8; MAX_TYPE_NAME_LEN],
    _type_name_len:   usize,
    type_hash:        [u8; MAX_TYPE_HASH_LEN],
    _type_hash_len:   usize,
}
```

The C++ header (`action_server.hpp`) exposes two pairs of
callback-driven accessors plus a poll-based iterator:

```cpp
template <typename A>
class ActionServer {
public:
    using Goal    = typename A::Goal;
    using Result  = typename A::Result;

    // Callback registration — mirrors Rust's `add_action_server`.
    // User's goal_callback returns AcceptAndExecute / AcceptAndDefer /
    // Reject, exactly like Rust.
    using GoalCallback   = std::function<GoalResponse(const GoalUuid&, const Goal&)>;
    using CancelCallback = std::function<CancelResponse(const GoalUuid&, GoalStatus)>;

    Result set_goal_callback(GoalCallback cb);
    Result set_cancel_callback(CancelCallback cb);

    // Poll-based iteration over arena's active_goals. Replaces the old
    // PendingGoal[] queue. Backed by for_each_active_goal_raw.
    template <typename F>
    void for_each_active_goal(F&& f);  // F: (const GoalUuid&, GoalStatus, const Goal&)

    // Lifecycle operations — already thin wrappers, unchanged by Phase 83.
    Result publish_feedback(const GoalUuid&, const typename A::Feedback&);
    Result complete_goal(const GoalUuid&, const Result&);
    Result abort_goal(const GoalUuid&);
    Result cancel_goal(const GoalUuid&);
};
```

Existing C++ examples that auto-accepted via the old `try_recv_goal`
poll gain a trivial `set_goal_callback([](…){ return AcceptAndExecute; })`
line during migration. Examples that actually want to reject or defer
gain the ability to do so for the first time — this is the latent-bug
fix.

### Part B — Centralize CDR header (fixes #4)

`nros-serdes` gains a tiny constants module:

```rust
// packages/core/nros-serdes/src/cdr_header.rs

/// Size of the CDR encapsulation header, in bytes.
pub const CDR_HEADER_LEN: usize = 4;

/// The canonical CDR header nros-serdes emits:
/// little-endian representation identifier (0x0001) + zero options.
pub const CDR_HEADER_LE: [u8; CDR_HEADER_LEN] = [0x00, 0x01, 0x00, 0x00];

/// If `bytes` begins with a CDR header, returns the payload slice.
/// Returns the original slice unchanged otherwise (callers that require
/// the header to be present should check length separately).
#[inline]
pub fn strip_header(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= CDR_HEADER_LEN { &bytes[CDR_HEADER_LEN..] } else { bytes }
}

/// Writes the canonical header into the first 4 bytes of `out`.
/// Returns the tail slice `&mut out[CDR_HEADER_LEN..]`.
#[inline]
pub fn prepend_header(out: &mut [u8]) -> &mut [u8] {
    out[..CDR_HEADER_LEN].copy_from_slice(&CDR_HEADER_LE);
    &mut out[CDR_HEADER_LEN..]
}
```

These are `#[inline]` + `const` where possible so there is zero runtime
cost relative to the current inline constants.

Every call site in `nros-c/src/action/*.rs` and `nros-cpp/src/action.rs`
that currently writes `[0x00, 0x01, 0x00, 0x00]` or indexes past `4`/`20`/`24`
as "CDR header + something" switches to:

```rust
use nros_serdes::cdr_header::{CDR_HEADER_LEN, CDR_HEADER_LE, strip_header, prepend_header};
```

plus named constants like `GOAL_ID_LEN = 16` and
`CDR_HEADER_PLUS_UUID = CDR_HEADER_LEN + GOAL_ID_LEN` at the top of each
module.

No ABI or behaviour change; purely a naming + locality pass. The C
header file for serdes is not touched — these helpers are Rust-only
because only the Rust FFI shim uses them.

## Work Items

- [ ] 83.1 — nros-node: add `for_each_active_goal_raw` + `active_goal_raw`
  - **Files**: `packages/core/nros-node/src/executor/action.rs`,
    `packages/core/nros-node/src/executor/action_core.rs`
  - **Goal**: Read-only accessors on `Executor` keyed by
    `ActionServerRawHandle`. Iterate/query
    `ActionServerArenaEntry::active_goals`. Document that the callback is
    invoked synchronously and must not call back into the arena.

- [ ] 83.2 — nros-c: drop `goals[]` + `active_goal_count` from
      `nros_action_server_t`
  - **Files**: `packages/core/nros-c/src/action/server.rs`,
    `packages/core/nros-c/include/nros/action.h` (regenerate via
    cbindgen)
  - **Goal**: Remove the fields. Update every internal reference.
    `nros_goal_handle_t` stays as an output struct for the existing
    getters; it is no longer *stored* in the server.

- [ ] 83.3 — nros-c: rewrite `nros_action_get_goal_status` on arena query
  - **Files**: `packages/core/nros-c/src/action/server.rs` (the getter),
    `packages/core/nros-c/include/nros/action.h`
  - **Goal**: Call `Executor::active_goal_raw` through `ActionServerInternal`.
    Return `NROS_RET_NOT_FOUND` for retired goals instead of returning
    stale `SUCCEEDED` / `ABORTED` cached status.

- [ ] 83.4 — nros-c: simplify goal / accepted trampolines
  - **Files**: `packages/core/nros-c/src/action/server.rs`
  - **Goal**: Remove the "fill C goal slot" and "increment
    active_goal_count" blocks from the trampolines. They become pure ABI
    shims that call the user's C callback and return the response.
    Keep the existing `accepted_callback` post-accept hook intact.

- [ ] 83.5 — nros-cpp: add callback-based action server API
  - **Files**: `packages/core/nros-cpp/include/nros/action_server.hpp`,
    `packages/core/nros-cpp/src/action.rs` (FFI)
  - **Goal**: New `set_goal_callback` / `set_cancel_callback` on
    `ActionServer<A>`, typed in terms of `A::Goal` via the existing
    `NROS_TRY`-style codegen. Trampolines dispatch to `std::function` when
    `NROS_CPP_STD` is defined, plain function pointers otherwise.

- [ ] 83.6 — nros-cpp: add `for_each_active_goal` iterator
  - **Files**: `packages/core/nros-cpp/include/nros/action_server.hpp`,
    `packages/core/nros-cpp/src/action.rs`
  - **Goal**: Template method that parses CDR goal bytes into `A::Goal`
    and forwards `(uuid, status, goal)` to the user's visitor. Backed by
    the new `for_each_active_goal_raw` FFI call.

- [ ] 83.7 — nros-cpp: delete `PendingGoal[]` + auto-accept trampoline
  - **Files**: `packages/core/nros-cpp/src/action.rs:25–94`,
    related destruction + size-assertion sites
  - **Goal**: Remove `struct PendingGoal`, the `pending` field on
    `CppActionServer`, the auto-accept trampoline, and the
    `nros_cpp_action_server_try_recv_goal` FFI entry. Shrink
    `CppActionServer` — the opaque-storage estimate in `build.rs`
    updates with it.

- [ ] 83.8 — nros-cpp: migrate action-server examples
  - **Files**: `examples/native/cpp/zenoh/action-server/src/main.cpp`,
    `examples/qemu-arm-freertos/cpp/zenoh/action-server/src/main.cpp`,
    `examples/zephyr/cpp/zenoh/action-server/src/main.cpp`
  - **Goal**: Replace the old `try_recv_goal` polling loop with
    `server.set_goal_callback([](uuid, goal){ return GoalResponse::AcceptAndExecute; })`
    plus `server.for_each_active_goal(...)` for iteration. At least one
    example (native) should show a non-auto-accept goal callback as
    documentation.

- [ ] 83.9 — nros-serdes: introduce `cdr_header` module
  - **Files**: new `packages/core/nros-serdes/src/cdr_header.rs`,
    `packages/core/nros-serdes/src/lib.rs` (module export)
  - **Goal**: Publish `CDR_HEADER_LEN`, `CDR_HEADER_LE`, `strip_header`,
    `prepend_header`. Add a Verus proof (or a plain unit test if Verus
    is overkill) that `strip_header(prepend_header(buf))` is a no-op on
    the tail.

- [ ] 83.10 — nros-c: migrate all CDR header call sites
  - **Files**: `packages/core/nros-c/src/action/client.rs:418–428,
    472–486`, `packages/core/nros-c/src/action/server.rs:141–155,
    431–433, 480–482`, `packages/core/nros-c/src/service.rs` (similar
    sites), `packages/core/nros-c/src/publisher.rs` and
    `packages/core/nros-c/src/subscription.rs` (audit for the same
    magic bytes)
  - **Goal**: Replace inline `[0x00, 0x01, 0x00, 0x00]` writes, bare
    `4`-byte offsets, and `20`/`24` offsets documented as "CDR + UUID"
    with named constants from `nros_serdes::cdr_header` + module-local
    `const CDR_HEADER_PLUS_UUID: usize = CDR_HEADER_LEN + GOAL_ID_LEN;`.

- [ ] 83.11 — nros-cpp: migrate all CDR header call sites
  - **Files**: `packages/core/nros-cpp/src/action.rs:610–614, 823–826,
    892, 948, 1151, 1172`, any equivalent sites in `service.rs` /
    `publisher.rs` / `subscription.rs` surfaced by grep
  - **Goal**: Same migration as 83.10 — no inline header bytes, no bare
    `4`/`20`/`24` offsets. Use the Rust-side constants via
    `use nros_serdes::cdr_header::*;` in the FFI staticlib.

- [ ] 83.12 — Test coverage: stale-goal-status regression
  - **Files**: `packages/testing/nros-tests/tests/action_server.rs`
  - **Goal**: Test that calls `nros_action_get_goal_status` for a goal
    the arena has retired and asserts `NROS_RET_NOT_FOUND`, not a
    spuriously-cached terminal status. Exercises the Part-A fix
    end-to-end.

- [ ] 83.13 — Test coverage: C++ goal rejection works
  - **Files**: `packages/testing/nros-tests/tests/cpp_action.rs`
  - **Goal**: Test with a `set_goal_callback` that returns
    `GoalResponse::Reject` and asserts the client sees a rejection.
    This case is currently untestable on the C++ side because the
    auto-accept trampoline ignores the user's callback.

- [ ] 83.14 — Thin-wrapper compliance audit re-run
  - **Files**: `docs/design/thin-wrapper-audit.md` (new) — summary of
    the audit methodology and a checklist for future reviewers
  - **Goal**: Document how the audit was run, the five findings, and the
    resolution for each. Future code review uses this as the
    compliance checklist so the same violations don't re-appear.

## Acceptance Criteria

- [ ] **Arena-authoritative goal state**:
      `packages/core/nros-c/src/action/server.rs` contains no field named
      `goals` or `active_goal_count` on `nros_action_server_t`, and no
      mutation of a C-side goal-lifecycle array in any trampoline.
      `packages/core/nros-cpp/src/action.rs` contains no `PendingGoal`
      struct and no `pending` field on `CppActionServer`. `grep -rn
      'AcceptAndExecute' packages/core/nros-cpp/` returns only sites
      inside user-supplied callbacks (no trampoline-forced accept).
- [ ] **Goal-status query correctness**:
      `nros_action_get_goal_status` for a retired (arena-dropped) goal
      returns `NROS_RET_NOT_FOUND`. Covered by 83.12.
- [ ] **C++ goal rejection**: the C++ `set_goal_callback` can return
      `Reject` / `AcceptAndDefer` and the client observes the
      corresponding `GoalResponse` on the wire. Covered by 83.13.
- [ ] **CDR header centralization**: `grep -rn '0x00.*0x01.*0x00.*0x00'
      packages/core/nros-c/ packages/core/nros-cpp/` returns zero
      results outside of `nros-serdes` (the canonical definition site)
      and test fixtures. Bare literal `4` offsets used as "size of CDR
      header" are replaced with `CDR_HEADER_LEN`.
- [ ] **No behaviour regression**: every existing action-server test
      still passes on every platform (native POSIX, NuttX QEMU,
      FreeRTOS QEMU, ThreadX, ESP32-QEMU, MPS2-AN385, Zephyr).
- [ ] **Phase 77 alignment**: this phase does not reintroduce any of the
      Phase 77 closures (no new `static mut BLOCKING_*` flags, no new
      condvar waits in C/C++ action paths).

## Notes & Caveats

- **Scope boundary with Phase 77**: audit findings #2 and #5 (static mut
  BLOCKING_ACCEPTED / BLK_RESULT_*) are not Phase 83 deliverables. They
  are the client-side blocking-flag pattern and are tracked by the
  remaining Phase 77 work items. Phase 83 only touches the action server
  and the CDR centralization.
- **Opaque-storage-size churn**: shrinking `CppActionServer` (Part A)
  changes `CPP_ACTION_SERVER_OPAQUE_U64S` (computed by
  `nros-cpp/build.rs`). The generated C header
  (`nros_cpp_config_generated.h`) regenerates automatically; no manual
  update needed. The same applies to `nros_action_server_t` in
  `nros-c`: its size shrinks when `goals` / `active_goal_count` are
  removed. C users who sized buffers on `sizeof(nros_action_server_t)`
  are unaffected (it was already a fixed allocation, it just got
  smaller).
- **User-visible C++ API break**: removing
  `nros_cpp_action_server_try_recv_goal` is an FFI symbol break. The
  C++ header-level change (old `try_recv_goal` method → new callback
  registration) is a source break for action-server users. Both happen
  in the same PR that lands 83.5–83.8, with every in-repo example
  updated in the same diff (same migration shape as Phase 82).
- **No rmw protocol change**: Phase 83 touches only the
  `nros-c`/`nros-cpp` façade and a small helper module in
  `nros-serdes`. No key-expression construction, no transport shim, no
  rmw_zenoh-interop behaviour changes. ROS 2 interop tests should be
  unaffected.
- **Why not just keep the audit findings as-is?** The C-side goal array
  (#3) is the active correctness risk — arena-internal goal retirement
  already happens during `spin_once` without firing the C trampoline,
  so the getter can return stale status today. The C++ auto-accept
  (#1) silently drops the user's callback, which is an API divergence
  across the three language bindings. The CDR magic numbers (#4) are
  the lowest-priority item but are cheap to fix and pair naturally
  with the rest of the cleanup.
- **Future-forward hook**: once the arena owns goal state in every
  language, a future phase can add real multi-goal semantics
  (concurrent accept, per-goal deferred execution) by extending
  `ActionServerArenaEntry` alone, without another round of parallel
  C/C++ updates. Phase 83 is the enabling cleanup for that work.
