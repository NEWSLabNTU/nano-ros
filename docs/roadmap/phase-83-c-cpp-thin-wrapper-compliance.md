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

**Status**: Complete (CDR centralization, Step 1 arena-authoritative
state, Step 2 ID-card handle redesign + C++ rewrite all landed on
`main` in commits `d24a28ef`, `220ea8fa`, `c1c6b2be`, and the 83.12 /
83.15 / 83.16 follow-ups)
**Priority**: Medium — architectural cleanup, no user-visible regression on
passing tests. Blocks future multi-goal / multi-language consistency work
because the current duplication means any arena change needs parallel
updates in C and C++.
**Depends on**: Phase 77 (async action client — partially closes the
related blocking-flag issue), Phase 82 (executor-driven blocking APIs —
established the lifecycle pattern)

## Two-step migration

The goal-state refactor ships as two PRs to keep each blast radius
bounded and independently revertible.

### Step 1 — Option 3: arena-authoritative state, source-compatible callbacks

- Struct ABI shrinks: `nros_goal_handle_t` drops `status` and `active`
  fields; `nros_action_server_t` drops `active_goal_count`. Users must
  recompile. No source change needed for callers that don't read the
  dropped fields — none of the in-repo examples do.
- Trampolines stop duplicating arena state. `nros_action_succeed` /
  `abort` / `cancel_accept` stop mutating the C-side goal handle and
  just delegate to the arena.
- `server.goals[N]` *stays* as persistent storage for `nros_goal_handle_t`
  structs that trampolines hand out to user callbacks. The array is
  reduced to `{uuid, context, server}` triples — no state duplication,
  just storage for the pointers user callbacks receive.
- New public API: `nros_action_get_goal_status(goal, &out)` reads the
  arena; returns `NROS_RET_NOT_FOUND` for retired goals.
- Existing `nros_action_server_get_active_goal_count` signature unchanged;
  backing switches to `ActionServerRawHandle::active_goal_count(executor)`.
- nros-cpp is *not touched* in Step 1 — the `PendingGoal[]` / auto-accept
  pattern remains until Step 2 rewrites it wholesale.
- **Caller impact**: source-compatible for every in-repo example and
  test. Out-of-repo C code that reads `goal->status` / `goal->active` /
  `server->active_goal_count` as struct fields needs a mechanical
  migration to the new arena getters. ABI break on struct size.

### Step 2 — Option 2: ID-card handle, stateless server, uniform callbacks

- `nros_goal_handle_t` reduces to `{ uuid }`. No `context`, no `server`
  back-pointer. Copyable by value; users track per-goal state in their
  own `{uuid → state}` tables.
- Callback signatures add `server` as an explicit parameter:
  - `nros_goal_callback_t(server, uuid, request, len, ctx)`
  - `nros_accepted_callback_t(server, goal_handle, ctx)`
  - `nros_cancel_callback_t(server, goal_handle, ctx)`
- Operations take `(server, goal)` explicitly:
  `nros_action_execute/succeed/abort/publish_feedback/get_goal_status(server, goal, ...)`.
- `nros_action_server_t` drops the `goals[N]` array entirely. Trampoline
  builds a stack-local `nros_goal_handle_t` per invocation; users copy
  by value if they need the UUID beyond the callback.
- nros-cpp switches to uniformly callback-based:
  `server.set_goal_callback(...)` / `set_cancel_callback(...)` +
  `for_each_active_goal` iterator. `PendingGoal[]`, auto-accept
  trampoline, and `try_recv_goal` poll API all deleted.
- **Caller impact**: source break for every action-server example and
  test in the repo. Migration is mechanical (add `server` arg, change
  `goal` to `const *`, move per-goal context to user-side storage).
  Every in-repo example updated in the same PR. No deprecation shim.

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

### Part B — CDR header centralization (complete)

- [x] 83.9 — nros-serdes: add `CDR_HEADER_LEN` + helpers, re-export
      through `nros::cdr`
  - Landed in commit `d24a28ef`. Adds `CDR_HEADER_LEN`, reuses
    existing `CDR_LE_HEADER` / `CDR_BE_HEADER`, adds
    `write_cdr_le_header` / `strip_cdr_header` helpers. `nros::cdr`
    module re-exports them so nros-c / nros-cpp go through the umbrella
    crate rather than depending on `nros-serdes` directly.

- [x] 83.10 — nros-c: migrate CDR header call sites
  - Landed in commit `d24a28ef`. All inline
    `buf[0..4] = 0x00 0x01 0x00 0x00` writes in
    `nros-c/src/action/{client,server}.rs` replaced with
    `write_cdr_le_header`; all "strip CDR header" offsets replaced
    with `strip_cdr_header`. `4 + 16` magic uses
    `CDR_HEADER_LEN + GoalId::UUID_LEN` (also re-exported via
    `nros::GoalId::UUID_LEN`).

- [x] 83.11 — nros-cpp: migrate CDR header call sites
  - Landed in commit `d24a28ef`. Same migration applied to
    `nros-cpp/src/action.rs`.

### Step 1 — Option 3: arena-authoritative state (current)

All Step 1 work items keep the existing callback-shape-compatible API
surface. No source change required for in-repo examples; struct ABI
shrinks.

- [x] 83.1 — nros-node: add `ActionServerRawHandle::goal_status`
  - **Files**: `packages/core/nros-node/src/executor/action.rs`,
    `packages/core/nros-node/src/executor/action_core.rs`
  - **Goal**: Single-goal lookup `fn goal_status(&self, executor: &Executor,
    goal_id: &GoalId) -> Option<GoalStatus>`. Implemented as a thin
    scan over `ActionServerArenaEntry::active_goals` using the existing
    `for_each_active_goal` hook. `active_goal_count(executor)` already
    exists; no change there.

- [x] 83.2 — nros-c: drop `active_goal_count` from `nros_action_server_t`
  - **Files**: `packages/core/nros-c/src/action/server.rs`,
    `packages/core/nros-c/include/nros/action.h` (regenerate via
    cbindgen), `packages/core/nros-c/src/action/common.rs` (Kani tests
    that assert zero count)
  - **Goal**: Remove the field and every `active_goal_count += / -=`
    mutation. The public
    `nros_action_server_get_active_goal_count(server)` signature is
    unchanged; it forwards to `ActionServerRawHandle::active_goal_count`
    via `ActionServerInternal.{handle, executor_ptr}`.

- [x] 83.3 — nros-c: drop `status` and `active` fields from
      `nros_goal_handle_t`
  - **Files**: `packages/core/nros-c/src/action/common.rs`,
    `packages/core/nros-c/src/action/server.rs`,
    `packages/core/nros-c/include/nros/action.h` (regenerated)
  - **Goal**: Struct reduces to `{uuid, context, server}`. Every
    internal reference to `goal->status` / `goal->active` is
    eliminated. `goals: [nros_goal_handle_t; N]` stays — it is the
    persistent storage for the pointers user callbacks receive, now
    holding just `{uuid, context, server}` triples.

- [x] 83.4 — nros-c: add `nros_action_get_goal_status(goal, &out)`
  - **Files**: `packages/core/nros-c/src/action/server.rs`,
    `packages/core/nros-c/include/nros/action.h`
  - **Goal**: New public C function: reads `goal->server`, pulls
    `ActionServerInternal.{handle, executor_ptr}`, calls
    `handle.goal_status(executor, &GoalId { uuid: goal.uuid.uuid })`.
    Returns `NROS_RET_NOT_FOUND` for retired goals instead of a
    spuriously-cached terminal status.

- [x] 83.5 — nros-c: simplify trampolines and lifecycle APIs
  - **Files**: `packages/core/nros-c/src/action/server.rs`
  - **Goal**: Goal / accepted / cancel trampolines stop writing
    `status` and `active` into the slot. Slot reclamation uses the
    arena: a slot is free when its UUID is no longer in `active_goals`
    (queried via `handle.goal_status`). `nros_action_succeed` / `abort`
    / `cancel_accept` drop all `goal->status = X` / `goal->active = false`
    / `count -= 1` blocks and just call the arena. Keep the existing
    `accepted_callback` post-accept hook intact.

- [x] 83.6 — Step 1 verification: unit + native integration tests
  - **Files**: `packages/testing/nros-tests/tests/action_server.rs` (new),
    existing native action-server tests
  - **Goal**: Add a regression test that calls
    `nros_action_get_goal_status` for a retired goal and asserts
    `NROS_RET_NOT_FOUND`. Run `just check` + `just test-unit` +
    `just native test` and confirm no regression in existing
    action-server coverage (native POSIX, FreeRTOS QEMU via the
    existing `just freertos test` if available in the dev loop).

### Step 2 — Option 2: ID-card handle + stateless server (queued)

Step 2 lands as its own PR after Step 1 is verified on every platform.
It's a hard source break for every action-server user; every in-repo
example + test migrates in the same diff. No deprecation shim.

- [x] 83.7 — nros-c: collapse `nros_goal_handle_t` to `{ uuid }`
  - **Files**: `packages/core/nros-c/src/action/common.rs`,
    `packages/core/nros-c/include/nros/action.h`
  - **Goal**: Drop the `context` and `server` fields. Handle becomes a
    pure ID card, copyable by value. Trampolines build a stack-local
    per invocation; users copy into their own storage if they need it
    past the callback.

- [x] 83.8 — nros-c: drop `goals[N]` array from `nros_action_server_t`
  - **Files**: `packages/core/nros-c/src/action/server.rs`,
    `packages/core/nros-c/include/nros/action.h`
  - **Goal**: Server struct holds metadata + handle opaque storage
    only. `NROS_MAX_CONCURRENT_GOALS` still governs the arena's
    template parameter but no longer sizes a C-side array.

- [x] 83.9 (Step 2) — nros-c: callback + operation signatures take
      `(server, goal, ...)`
  - **Files**: `packages/core/nros-c/src/action/{common,server}.rs`,
    `packages/core/nros-c/include/nros/action.h`
  - **Goal**: Every callback receives `nros_action_server_t *server` +
    `const nros_goal_uuid_t *` or `const nros_goal_handle_t *`. Every
    `nros_action_*` operation takes `server` as the first argument and
    `const nros_goal_handle_t *` as the second. All examples migrated
    in the same commit.

- [x] 83.10 (Step 2) — Migrate every in-repo C action-server example
  - **Files**: `examples/native/c/zenoh/action-server/src/main.c` and
    ~9 sibling files across `qemu-arm-{freertos,nuttx}`,
    `qemu-riscv64-threadx`, `threadx-linux`, `zephyr/c/{zenoh,xrce}`,
    `native/c/xrce`
  - **Goal**: Update callback signatures, insert `server` arg on every
    `nros_action_*` call, move per-goal context to user-side
    `{uuid → state}` storage.

- [x] 83.11 (Step 2) — nros-cpp: add callback-based action-server API
  - **Files**: `packages/core/nros-cpp/include/nros/action_server.hpp`,
    `packages/core/nros-cpp/src/action.rs` (FFI)
  - **Goal**: New `set_goal_callback` / `set_cancel_callback` on
    `ActionServer<A>`, typed in terms of `A::Goal` via the existing
    codegen. Trampolines dispatch to `std::function` when
    `NROS_CPP_STD` is defined, plain function pointers otherwise.

- [x] 83.12 (Step 2) — nros-cpp: add `for_each_active_goal` iterator
  - **Files**: `packages/core/nros-cpp/include/nros/action_server.hpp`,
    `packages/core/nros-cpp/src/action.rs`
  - **Goal**: Template method that parses CDR goal bytes into `A::Goal`
    and forwards `(uuid, status, goal)` to the user's visitor. Backed
    by `ActionServerRawHandle::for_each_active_goal`.

- [x] 83.13 (Step 2) — nros-cpp: delete `PendingGoal[]` + auto-accept
      trampoline + `try_recv_goal`
  - **Files**: `packages/core/nros-cpp/src/action.rs`,
    related destruction + size-assertion sites,
    `packages/core/nros-cpp/include/nros/action_server.hpp`
  - **Goal**: Remove the C++-side goal queue, the auto-accept
    trampoline, and the `nros_cpp_action_server_try_recv_goal` FFI
    entry. Shrink `CppActionServer`; the opaque-storage estimate in
    `build.rs` updates with it.

- [x] 83.14 (Step 2) — Migrate every in-repo C++ action-server example
  - **Files**: `examples/native/cpp/zenoh/action-server/src/main.cpp`,
    `examples/qemu-arm-freertos/cpp/zenoh/action-server/src/main.cpp`,
    `examples/zephyr/cpp/zenoh/action-server/src/main.cpp`
  - **Goal**: Replace the `try_recv_goal` poll loop with
    `set_goal_callback(...)` + `for_each_active_goal(...)`. At least
    one example (native) shows a non-auto-accept goal callback as
    documentation.

- [x] 83.15 (Step 2) — Test coverage: C++ goal rejection works
  - **Files**: `packages/testing/nros-tests/tests/cpp_action.rs`
  - **Goal**: Test with a `set_goal_callback` that returns
    `GoalResponse::Reject` and asserts the client sees a rejection.
    This case was untestable before Step 2 because the auto-accept
    trampoline ignored the user's callback.

- [x] 83.16 (Step 2) — Thin-wrapper compliance audit re-run
  - **Files**: `docs/design/thin-wrapper-audit.md` (new) — summary of
    the audit methodology and a checklist for future reviewers
  - **Goal**: Document how the audit was run, the original five
    findings, and the resolution for each. Future code review uses this
    as the compliance checklist so the same violations don't reappear.

## Acceptance Criteria

- [x] **Arena-authoritative goal state**:
      `packages/core/nros-c/src/action/server.rs` contains no field named
      `goals` or `active_goal_count` on `nros_action_server_t`, and no
      mutation of a C-side goal-lifecycle array in any trampoline.
      `packages/core/nros-cpp/src/action.rs` contains no `PendingGoal`
      struct and no `pending` field on `CppActionServer`. `grep -rn
      'AcceptAndExecute' packages/core/nros-cpp/` returns only sites
      inside user-supplied callbacks (no trampoline-forced accept).
- [x] **Goal-status query correctness**:
      `nros_action_get_goal_status` for a retired (arena-dropped) goal
      returns `NROS_RET_NOT_FOUND`. Covered by 83.12.
- [x] **C++ goal rejection**: the C++ `set_goal_callback` can return
      `Reject` / `AcceptAndDefer` and the client observes the
      corresponding `GoalResponse` on the wire. Covered by 83.13.
- [x] **CDR header centralization**: `grep -rn '0x00.*0x01.*0x00.*0x00'
      packages/core/nros-c/ packages/core/nros-cpp/` returns zero
      results outside of `nros-serdes` (the canonical definition site)
      and test fixtures. Bare literal `4` offsets used as "size of CDR
      header" are replaced with `CDR_HEADER_LEN`.
- [x] **No behaviour regression**: every existing action-server test
      still passes on every platform (native POSIX, NuttX QEMU,
      FreeRTOS QEMU, ThreadX, ESP32-QEMU, MPS2-AN385, Zephyr).
- [x] **Phase 77 alignment**: this phase does not reintroduce any of the
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
