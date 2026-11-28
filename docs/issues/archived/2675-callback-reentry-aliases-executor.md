---
id: 2675
title: "A C or C++ callback that calls back into its executor (a graph query, a timer cancel, a parameter call) makes a second `&mut` to the executor while `spin_once` holds the first"
status: resolved
type: bug
severity: high
area: [api, executor, safety]
related: [issue-0290, issue-0278, issue-2674, rfc-0108, rfc-0021, phase-572, phase-573]
found: 2026-11-17
resolved: 2026-11-25
resolved_in: "test(nros-node, phase-573 W4): a callback re-enters its executor while another band preempts it"
---

**Home: [phase-573](../../roadmap/phase-573-executor-reentry-soundness.md).** Found while
scoping [phase-572](../../roadmap/archived/phase-572-rust-component-graph-query.md), which
keeps the graph queries out of a Rust callback for this reason and waits on it.

## What

On the owned pass, a C or C++ callback runs while Rust holds a live `&mut` to
the executor, and many FFI entries the callback may call derive a second one.

- **C.** `rclc_executor_spin_some` (`nros-c/src/executor.rs`, around line 4845)
  takes `rust_exec = get_executor(executor)` (a `CExecRef` whose `DerefMut`
  yields `&mut CExecutor`). It calls `rust_exec.spin_once(..)`, which is
  `pub fn spin_once(&mut self, ..)` in `nros-node`, and that runs the user's C
  callbacks. A callback that calls, for example,
  `rclc_executor_get_topic_names_and_types(executor, ..)` reaches
  `nros_executor_names_and_types_impl`. That calls `get_executor(exec_t)` again
  and takes `&mut CExecutor` while the outer `&mut self` is still live inside
  `spin_once`.
- **C++.** `nros_cpp_executor_get_topic_names_and_types` goes through
  `cpp_names_and_types` → `cpp_ctx_checked(handle)`, which reads the executor
  pointer, takes the lock, and makes "the only reference". The same holds when
  it is called from a callback the C++ spin is dispatching.

The executor lock does not prevent this. It is RECURSIVE by design
(`nros-node/src/executor/exec_lock.rs`: "it is recursive because a callback
that calls back into the executor (`timer_cancel`, `*_release`, a parameter
call) re-enters it on the same thread"). It serialises threads, not reborrows
on one thread. `in_dispatch` (RFC-0021's guard) refuses only the BLOCKING
helpers (issue 0290 fixed C++'s; issue 0278 the futures). The non-blocking
entries are deliberately allowed from a callback: the graph queries, timer
cancel and reset, releases, parameter calls.

## Why it is a bug

Under Rust's aliasing model, `spin_once(&mut self)` holds a unique borrow
that is protected for the whole call. Any access to the executor through
another reference made during that call is undefined behaviour, whether it
reads or writes. A graph query also writes: it takes `&mut Session` and may
touch the graph cache. `&mut self` is emitted as `noalias`, so the compiler
may keep executor fields in registers across the callback, which it assumes
cannot reach `*self`.

No miscompile has been observed. This is a soundness defect found by reading
the code, not a measured failure. RFC-0108 records the re-entry as the reason
the lock is recursive and never discusses the aliasing.

## Scope

This is a class, not one entry: every C and C++ FFI entry that derives the
executor (`get_executor`, `get_executor_from_ptr`, `cpp_ctx_checked`,
`cpp_ctx_locked`, and the `CLocked`/`Locked` lookups) is unsound when called
from a callback on the owned pass, unless it is refused there. The shared and
band passes (RFC-0108 item 2) give the lock up around each callback and
re-derive their reference after it, so they are sound when the lock opens a
window (it has a thread id). Without one, a shared pass behaves like the owned
pass.

Not affected: the Rust component surface. Its callbacks get no executor
access, which is why phase-572 W1 keeps the graph queries out of
`CallbackCtx`.

## Fix direction (ruled 2026-11-17)

Keep the behaviour, make it sound. The owned pass holds no reference into
the executor across a user callback: `Executor::pass` re-derives from `this`
after every callback in every mode, and every spin entry reaches it through a
raw pointer. Refusing every executor-deriving entry while dispatching was
rejected, because it would take timer cancel and reset, releases, parameter
calls and graph queries out of C and C++ callbacks.

Reading `pass` (2026-11-17) narrowed the core half. The windowed shared and
band passes already re-derive after each callback, so they are not affected.
The owned pass, and a shared pass whose lock has no thread id, keep `s` live
across `try_process`. Separately, `spin_once(&mut self)` and the FFI spin
entries hold a protected `&mut` for the whole pass.

## Acceptance

- A Miri test (or the nearest the FFI allows) that runs a C callback calling
  back into its executor shows no aliasing violation, or the call is refused.
- Every executor-deriving FFI entry is either safe from a callback or refuses
  there with `RCLC_RET_REENTRANT` / `Reentrant`. The list of entries is in this
  issue.

## Resolution

Fixed by phase-573 W1 to W4, as RFC-0108 D9 describes:

- **The dispatch core (W2, W2b).** `pass` derives its reference again after
  every callback in every mode. Every spin entry reaches it through
  `spin_once_raw` and a raw pointer. Each entry kind's dispatch either
  derives its entry again after the callback or has no FFI path that writes
  the running entry.
- **The FFI entries (W3).** Every executor-deriving entry in nros-c and
  nros-cpp goes through its crate's one locked derive. A call that would
  block, spin again or release an entry the running pass may touch returns
  `RCLC_RET_REENTRANT` / `Reentrant` from a callback. The gate
  `check-ffi-executor-derive` holds the list.
- **Preemption (W4).** Two band workers over one executor: an urgent band
  resets the timer a less urgent band is still dispatching, and the preempted
  callback then resets itself.

Acceptance, item by item:

- A Miri test runs callbacks that call back into their executor, and shows no
  aliasing violation: `just check miri-executor-reentry`, which runs
  `executor::tests::reentry*` under Tree Borrows. Each fixed site's case
  failed on the code before its fix, or was shown sensitive by a measured and
  reverted mutation.
- Every executor-deriving FFI entry is safe from a callback or refuses there:
  `check-ffi-executor-derive` (W3).

One residue is tracked separately. Without `alloc`, an owned session is still
shared from inside its owner
([issue 2845](../2845-owned-session-from-executor-backing.md), phase-573 W6).
