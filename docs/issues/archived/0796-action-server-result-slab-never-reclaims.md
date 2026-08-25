---
id: 796
title: "Action server: the result slab only ever grows, so a long-running server
  silently stops delivering results; and the C++ callback tier reports every
  goal as SUCCEEDED"
status: resolved
type: bug
area: core, api
related: [rfc-0069, phase-379, phase-237]
---

## Problem 1 — the result slab is never reclaimed (FIXED 2026-08-25)

`ActionServerCore::complete_goal_raw`
(`packages/core/nros-node/src/executor/action_core.rs:449-464`) appends each
completed goal's result CDR to a fixed slab:

```rust
let offset = self.result_slab_used;
let end = offset + result_cdr.len();
let stored = if end <= RESULT_BUF {
    self.result_slab[offset..end].copy_from_slice(result_cdr);
    self.result_slab_used = end;
    ...
    true
} else {
    false
};
```

`result_slab_used` is set to `0` once at construction (line 252) and thereafter
only ever assigned `end`. **Nothing resets it** — there is no reclamation path
anywhere in the file.

So once the slab fills, `stored` is `false` for every subsequent goal and:

* the result is dropped,
* the `pending_get_results` flush below is skipped, so any client already waiting
  on `~/_action/get_result` waits forever,
* and `complete_goal_raw` returns `()` — the server's own callback is told
  nothing.

`RESULT_BUF` defaults to `DEFAULT_RX_BUF_SIZE` (1024), so the ceiling is about a
kilobyte of accumulated results. A server that completes goals in a loop stops
working after a bounded number of them and keeps reporting success.

There were in fact **two** leaks with one cause — nothing was ever evicted.
`result_slab_used` only moved forward, and `completed_results`
(`heapless::Vec<_, MAX_GOALS>`, default 4) was only ever `push`ed, with the push
discarded by `let _ =`. Whichever filled first stopped the server. The typed
mirror `ActionServer::completed_goals` was push-only too, and read by nobody.

rcl handles this with a per-goal `result_timeout` plus
`rcl_action_expire_goals()`, which reclaims the storage of goals whose result has
been collected or has aged out. We had **neither the timeout nor the
reclamation** — recorded in the action stage as a `gap` on
`c:action_expire_goals`.

### Fixed — compaction, plus demand-driven delivered-first eviction

**Storage shape: compaction, not partitioning.** The slab stays one bump region;
`completed_results` is held in completion order, which is also increasing-offset
order, and on every reclamation `compact_result_slab()` walks the survivors in
that order, memmoves each down to the first free byte and rewrites its offset.
Bounded and infrequent: at most `MAX_GOALS` (4) moves totalling at most
`RESULT_BUF` bytes, only when a completion needs room.

The alternative — partitioning the slab into `MAX_GOALS` fixed slots, which makes
eviction O(1) and bounds the worst case per goal rather than in total — was
rejected on its cost: with the shipped defaults it would cut the largest storable
result from 1024 bytes to 256, and that cut is **permanent**, not transient. A
result between 257 and 1024 bytes works today and would stop working forever,
failing even against an empty slab. Compaction keeps "any single result up to
`RESULT_BUF`" intact and pays for it with a few-KB memmove on eviction. Every
entry that could hold that memmove is capped by `MAX_GOALS`, so the price is
bounded where the capability loss would not have been.

**Eviction policy.** rcl keeps a result until it is collected AND a
`result_timeout` elapses. We have no clock in the core (it is `no_std` and no
time source is threaded through it), so reclamation is on demand and
priority-ordered instead of timed:

1. the oldest result already **delivered** to a client, else
2. the oldest result overall.

Rule 1 means a fetched result never pins storage — the case rcl's timeout really
serves. Rule 2 means a client that asks for nothing cannot wedge the server: its
stale result is displaced by newer ones. A goal evicted under rule 2 whose client
asks later is answered `GoalStatus::Unknown` with the default result — degraded,
but an answer, where the old code left the requester hanging forever.
`expire_completed_results()` is the eager analogue of `rcl_action_expire_goals()`
for a server that would rather return the memory before an idle period; calling
it is optional.

**The silence is gone.** `ActionServerCore::complete_goal_raw` returns
`Result<(), NodeError>`, and so do `ActionServerRawHandle::complete_goal_raw`,
`ActionServerHandle::complete_goal` and `ActionServer::complete_goal`. A full
slab is no longer a failure at all — it is reclaimed — so the one remaining error
is a result larger than `RESULT_BUF`, which names the knob to raise. The C and
C++ entry points already returned status codes, so they now propagate it with no
header change (`check-cbindgen-headers` green without a regen).

Two more silent losses fell out of the same read:

* the `pending_get_results` flush sat inside `if stored`, so a client that had
  already sent `get_result` was **never answered** when the store failed. The
  waiter is now answered from the caller's own bytes — only the *retention*
  failed, the bytes are right there.
* `ActionServer::complete_goal` treated a serialization failure as a zero-length
  result and stored it as if it were real. It now reports
  `NodeError::Serialization`.

Regression tests live in `packages/core/nros-node/src/executor/tests.rs`
(`action_results_keep_being_delivered_past_the_slab_capacity` and four siblings).
Mutation-checked: with reclamation reverted and everything else kept, three of
them fail at goal 4 — exactly where a 128-byte slab of 40-byte results fills.

## Problem 2 — the C++ callback tier cannot abort or cancel a goal (FIXED 2026-08-25)

`packages/api/nros-cpp/src/action.rs:445`:

```rust
h.complete_goal_raw(
    &mut ctx.executor,
    &id,
    nros::GoalStatus::Succeeded,   // hardcoded
    result_fields,
);
```

The public `nros::ActionServer<A>::complete_goal(goal_id, result)` takes no
status, and the shim supplies `Succeeded` unconditionally. **A C++
callback-tier server that aborts a goal reports it to the client as
succeeded.**

Every other surface takes a status: C has `nros_action_abort` and
`nros_action_canceled`, the C++ *polling* tier takes one, and both Rust servers
take one. This is the C++ callback tier alone.

**Fixed.** The shim takes an `int32_t status` and validates it through
`GoalStatus::from_i8`; `ActionServer::complete_goal(goal_id, status, result)`
matches `PollingActionServer::complete_goal` and the C API's
`nros_action_server_complete_goal_raw`, so the three surfaces now agree on the
parameter order. A two-argument overload forwards `Succeeded`, so existing calls
keep compiling.

Worth recording how the mirrors were found: the declaration existed in **three**
places — `nros_cpp_ffi.h`, `nros-c/component.h` and `action_server.hpp` — and
editing two of them compiled fine. `just check-c`'s cross-include TU (the gate
CLAUDE.md describes for exactly this drift class) caught the third with
`conflicting types`. Six example call sites used the raw FFI directly and were
updated. `just check-c` and `just check-cpp` are green.

**Both problems are now fixed**, and so is the `Related` list below.

## Related, from the same stage — all FIXED 2026-08-25

### No accepted-callback in C++ — FIXED

`nros::ActionServer<A>` now has `set_accepted_callback` /
`set_accepted_callback_with_ctx`, matching its existing per-callback setter
shape (C takes all three at `nros_action_server_init`; Rust takes all three at
`create_action_server_with_callbacks`; C++ sets them one at a time, and this is
the third setter rather than a fourth parameter on an existing one). The hook
fires once per ACCEPTED goal, after the accept reply has reached the client, for
`AcceptAndExecute` and `AcceptAndDefer` alike — which is the case that mattered:
a user who returned ACCEPT_AND_DEFER was never told the goal had been accepted,
i.e. never told when to start executing it.

The shim registered `accepted_callback: None` with a comment claiming the C++
API "runs user callbacks via `try_accept_goal`, not via the post-accept hook".
That was not a design, just an absence — the raw path has never used
`try_accept_goal`. It now registers a trampoline unconditionally (the arena
captures the pointer when the entry is BUILT, so it cannot be added later) and
the trampoline is a no-op until a callback is installed.

`nros_cpp_action_server_set_accepted_callback` is a SEPARATE FFI entry point
rather than a fourth parameter on `nros_cpp_action_server_set_callbacks`: that
declaration is mirrored in three headers and called directly by four in-tree C
components, so growing it would have broken every caller for no gain. Both
calls pass the same context pointer, and `install_callbacks()` makes both, so
relocation cannot strand the new one.

`CppActionServer` gained one pointer, which is a size change: the layout mirror
in `nros::sizes::CppActionServerLayout` (the const-assert caught it, as
designed) and the NuttX snapshot in `nros_cpp_config_generated_nuttx.h` (an
upper bound, 80 → 88) both moved with it.

### Client-side cancel missing from the C++ callback tier — FIXED

`nros::ActionClient<A>::cancel_goal(goal_id)` sends the request and
`try_recv_cancel_response(CancelReturnCode&)` reads the RPC outcome — the same
fire-then-read shape as C's `nros_action_cancel_goal`. rclcpp_action's
`async_cancel_goal` returns a future; RFC-0021 has no runtime to await one, so
that part of the divergence stands and is recorded as such. A truncated reply
that carries no `return_code` byte is an ERROR, not "no reply yet" (issue
0223's rule).

### `CancelResponse` named two different concepts — FIXED, by following C

C was the only language that named them apart, so the other two now use its
pair:

| concept | C | C++ | Rust |
| --- | --- | --- | --- |
| per-goal Reject/Accept | `nros_cancel_response_t` | `nros::CancelResponse` | `nros_core::CancelResponse` |
| `CancelGoal` RPC status | `nros_cancel_return_code_t` | `nros::CancelReturnCode` | `nros_core::CancelReturnCode` |

`nros_core::CancelResponse` was the RPC return code and is now the per-goal
decision (`Reject` = 0, `Accept` = 1, matching C and C++); the RPC enum kept its
four variants under the new name. `CallbackCtx::set_cancel_response` therefore
takes the per-goal type, which was the actual bug — a per-goal question answered
with `CancelResponse::Ok`.

Keeping the NAME on the per-goal concept, rather than on the type that had it,
is deliberate: `nros::CancelResponse` is what C and C++ already meant by it, it
is what a `set_cancel_response` caller means by it, and every existing Rust use
site is a per-goal decision. It also means no caller changes MEANING silently —
the old variants (`Ok`, `Rejected`, …) do not exist on the new enum, so every
site is a compile error and had to be looked at. Ten in-tree call sites were:
five example action servers, one workspace package, one RTIC example, one
orchestration test package, and the two shims.

The split surfaced a live latent hazard. `ActionServerCore::try_handle_cancel`
wrote the callback's answer straight into the reply's `return_code` field
(`writer.write_i8(response as i8)`). That was correct only because the two
concepts shared a type; with them separated, the same line would have written
`Reject` (0) as `Ok` and `Accept` (1) as `Rejected` — a perfect inversion. The
translation is now explicit, and a unit test
(`test_cancel_response_is_not_a_return_code`) pins the overlap so nobody
reintroduces a cast.

**One thing is deliberately left undone**: `packages/api/nros/src/lib.rs` does
not re-export `CancelReturnCode`, so `ActionClient::cancel_goal`'s
`Promise<CancelReturnCode>` cannot be NAMED from `nros::` alone
(`nros_node::CancelReturnCode` works). That file was reserved by another
session at the time; the fix is one token in the existing
`pub use nros_core::{…}` list.

### `GoalResponse` / `CancelResponse` correlate as `same` and are not drop-in — RECORDED, not changed

Our discriminants are 0-based where rclcpp_action's are 1-based, and our
enumerators are `Reject` where theirs are `REJECT`. Not wire values, so not an
interop bug — but the correlator sees only the name, so the pair reports `same`
and the difference had nowhere to live. It now lives in the ledger:
`cpp:GoalResponse` and `cpp:CancelResponse` are rows *for items that correlate
as `same`*, which the tool permits (it only DEMANDS a row for non-matching
rows) and which is the only place a name-blind comparison can be corrected.

**Decision: do not align the discriminants.**

1. They are a C ABI, not a preference. The value crosses three FFI seams as an
   integer — a `#[repr(i8)]` Rust enum returned directly from `extern "C"`
   trampolines, `nros_c_goal_response_t` / `nros_c_cancel_response_t` in
   `component.h`, and an `enum class : int32_t` in C++. Renumbering produces no
   compile error anywhere; it produces a half-rebuilt tree that starts deciding
   accept where it meant reject.
2. It would buy nothing. A ported `switch` names
   `rclcpp_action::GoalResponse::REJECT`, so it must be edited regardless; once
   edited it uses our enumerators and gets the right value under either
   numbering. Only code that hardcodes the integers is affected, and that code
   is already wrong.
3. `Reject` = 0 is load-bearing. The arena's decision slots are zero-initialised
   static storage with no allocator and no constructor, and the null-callback
   path yields `Default`. Making 0 invalid would make "nobody answered" mean
   accept.

## Evidence

`packages/core/nros-node/src/executor/action_core.rs:218,252,449-464`;
`packages/api/nros-cpp/src/action.rs:440-450`;
`scripts/api-parity.py --topic action` and
`docs/reference/api-parity-ledger/action.json`.

## Direction

Not decided here. Problem 2 is a small, contained fix — thread the status
through `complete_goal` as every other surface already does. Problem 1 is a
design question: a bump allocator with no free needs either a reclamation pass
(rcl's shape: a result timeout plus an expiry sweep) or a different storage
shape (per-goal slots sized at declaration, which suits a static entity table
better and bounds the worst case instead of the total). Whichever is chosen, the
overflow path must stop being silent — `complete_goal_raw` returning `()` is how
this stayed invisible.
