# Thin-Wrapper Compliance Audit

This document captures the methodology and findings from the Phase 83
audit of `nros-c` and `nros-cpp` against the thin-wrapper principle
declared in `CLAUDE.md`:

> `nros-c` must be a thin FFI wrapper over `nros-node` — delegate to
> Rust types, don't reimplement logic. New C API features must first be
> implemented in `nros-node`, then wrapped.
>
> `nros-cpp` is a freestanding C++14 library... wrapping `nros-node`
> directly via typed `extern "C"` FFI.

Keep this doc in sync with the current compliance state. Future
reviewers should re-run the methodology (below) and update the findings
table whenever public surface in `nros-c` / `nros-cpp` grows.

## Methodology

1. **Scope**: audit `packages/core/nros-c/src/` and
   `packages/core/nros-cpp/src/` (and their headers) against the five
   compliance classes listed in the checklist.
2. **Dispatch an Explore agent** with the prompt in
   `.claude/agents/explore.md` (or a direct invocation) asking for
   citations in `path:line` form for each class. Keep the prompt under
   ~500 words to constrain the scan; include the full compliance-rule
   text as context. Example prompt skeleton:

   > Audit `packages/core/nros-c/src/` and
   > `packages/core/nros-cpp/src/` to verify they are thin FFI wrappers
   > over `nros-node`. For each of these five violation classes, list
   > every offending site:
   >
   > 1. Reimplemented state machines — action goal lifecycle, service
   >    request/reply tracking, etc.
   > 2. Retry / timeout / polling loops that spin the executor
   >    internally instead of the user-supplied executor.
   > 3. CDR serialization/deserialization in the C/C++ layer (should go
   >    through `nros-serdes` or `nros-rmw` types).
   > 4. Key-expression / topic construction (should be in
   >    `TopicInfo`/`ServiceInfo`/`ActionInfo`).
   > 5. Direct zenoh-pico / XRCE-DDS / transport calls (should go
   >    through `Session` / `Publisher` / etc.).
   >
   > Output: `path:line — one-sentence description` per finding, with
   > severity (blocker / warn / nit). Group by crate. No suggestions —
   > just the audit.

3. **Triage** the findings: **blocker** items (duplicated state machines
   or reimplemented protocol logic) get phase work items; **warn** and
   **nit** items are tracked but may land in later passes.
4. **Land fixes** incrementally, one finding per commit where possible,
   so each closure can be reverted independently.
5. **Record** each fix in the table below with the commit hash that
   closed it.

The whole audit takes ~15 minutes of agent time + a couple of hours of
manual triage. Re-run after any significant API addition to `nros-c` or
`nros-cpp`.

## Phase 83 findings

The first run found five issues spanning both FFI crates. All five are
now resolved — three fixed by structural refactors in commits
`220ea8fa` and `c1c6b2be`, one (CDR centralization) in commit
`d24a28ef`, and two (static-mut blocking flags in the action *client*
path) closed by existing Phase 77 work on async action clients.

| #  | Site                                                          | Severity   | Fixed in | Resolution |
|----|---------------------------------------------------------------|------------|----------|------------|
| 1  | `nros-cpp/src/action.rs` — `PendingGoal[]` + auto-accept       | blocker    | `c1c6b2be` | Deleted the pending-goal array and the auto-accept goal trampoline. The action server now exposes `set_goal_callback<F>(f)`; the callback owns the accept/reject decision. The goal queue that duplicated `ActionServerArenaEntry::active_goals` is gone. |
| 2  | `nros-c/src/action/client.rs` — `static mut BLOCKING_ACCEPTED` | blocker    | Phase 77 | Tracked separately as part of the async action-client work. Not re-delivered in Phase 83. |
| 3  | `nros-c/src/action/server.rs` — `goals[N]` + `active_goal_count` mirror of arena | blocker | `220ea8fa`, `c1c6b2be` | Step 1 (`220ea8fa`) stopped mutating the stale fields from trampolines and rewrote `nros_action_server_get_active_goal_count` on top of `ActionServerRawHandle::active_goal_count`. Step 2 (`c1c6b2be`) removed the `goals[N]` array and `active_goal_count` field entirely, collapsed `nros_goal_handle_t` to a pure `{uuid}` identity card, and added `nros_action_get_goal_status(server, goal, &status)` that reads the arena. |
| 4  | Scattered CDR header bytes (`0x00 0x01 0x00 0x00`, offsets 4/20/24) | warn       | `d24a28ef` | Hoisted `CDR_HEADER_LEN`, `CDR_LE_HEADER`, `write_cdr_le_header`, and `strip_cdr_header` into `nros-serdes`; re-exported via `nros::cdr`. `nros_core::GoalId::UUID_LEN` covers the recurring 16-byte UUID. Every call site in `nros-c/src/action/` and `nros-cpp/src/action.rs` switched to the named constants. |
| 5  | `nros-cpp/src/action.rs` — `static mut BLOCKING_ACCEPTED` in the client path | nit/warn | Phase 77 | Same pattern as finding #2; deferred to the Phase 77 async action-client closure. |

## Compliance checklist for future reviewers

A `nros-c` or `nros-cpp` PR is thin-wrapper-compliant if **none** of the
following appear in the added code. Apply this as a pre-merge review
gate.

- [ ] No `[u8; N]` arrays or `heapless::Vec` fields in the C/C++ FFI
      struct that mirror data the arena already owns (`active_goals`,
      `pending_*`, `completed_results`, etc.). Reserve such fields for
      pure identity or inline storage of the RMW handle — never
      lifecycle state.
- [ ] No counters that duplicate arena accessors (e.g.
      `active_goal_count`, `pending_count`). Always forward the getter
      to an arena call like `handle.active_goal_count(executor)`.
- [ ] Callbacks are trampolines that translate ABI + call through to the
      user's callback. No FSM transitions inside trampolines; no slot
      reservation beyond what the arena requires.
- [ ] Lifecycle ops (`succeed` / `abort` / `cancel` / `complete` /
      `publish_feedback`) contain zero C-side state mutation. They
      extract the goal identity, look up the arena handle, and delegate.
- [ ] No `static mut BLOCKING_*` flags and no `zpico_get`-style condvar
      waits. Blocking convenience wrappers must take a user-supplied
      executor and drive it via `spin_some` — see Phase 77 and Phase 82
      for the canonical pattern.
- [ ] No magic bytes / offsets for CDR framing. Import from `nros::cdr`
      (or `nros_serdes::...` inside `nros-node` internals).
- [ ] No key-expression / topic-string construction. Use
      `TopicInfo` / `ServiceInfo` / `ActionInfo` builders from
      `nros-rmw`.
- [ ] No direct calls to `zpico_*` / `uxr_*` / other transport SDKs.
      Go through `Session` / `Publisher` / `Subscriber` /
      `ActionServerRawHandle`.

If a review needs to break any of these rules, the PR description must
justify the exception with a link to the governing phase doc (e.g.
Phase 77 for the action-client blocking pattern). Otherwise the PR
should be bounced and the relevant logic moved into `nros-node` first.

## Re-running the audit

Approximately once per release:

```
# From the repo root — dispatch the audit prompt above. The full
# prompt lives in the Phase 83 commit history if you want to copy it.
# Manually triage each finding; blockers become phase work items,
# warns/nits get tracked in this doc's findings table.
```

Update this file's table whenever a new finding lands or an old one is
resolved. The goal is that running the audit on `main` should find
zero new blockers, and the findings table stays an accurate record of
known exceptions.
