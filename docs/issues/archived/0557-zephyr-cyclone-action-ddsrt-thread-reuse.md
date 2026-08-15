---
id: 557
title: "Zephyr Cyclone action images fail at boot with `tid … is in use!` and rc=-100 — the readiness timeout hides an immediate failure"
status: resolved
type: bug
severity: high
area: rmw, zephyr, testing
related: [issue-0371, issue-0445, phase-350]
---

## Symptom

`zephyr::example_e2e::case_17_cyclonedds_c_action_e2e` and `case_18` (C++) fail
SOLO on a fully green `lane=all` fixture build — so this is not sweep
contention, which is how both were previously carried.

The verdict reads as a timeout:

```
[cyclonedds/c/Action] action-server didn't reach readiness
  (`Waiting for action goals`) within 60 s
```

The guest output says otherwise — it fails IMMEDIATELY and the 60 s is just the
harness waiting for a marker that will never arrive:

```
*** Booting Zephyr OS build v3.7.0 ***
<inf> cyclonedds: session_create: domain=29 entering
<inf> cyclonedds: cyclone: started application thread 3595786652
<err> os: tid 0x581fa0 is in use!      <- x6, consecutive tids
<inf> cyclonedds: session_create: calling dds_create_participant
<inf> cyclonedds: session_create: dds_create_participant returned 49379019
nros zephyr entry: run_components failed rc=-100
```

Issue 0445's shape at the harness level: a self-explaining terminal verdict
(`didn't reach readiness`) replaces the runtime result, and the real error is
four lines up.

## What the signals mean

`tid %p is in use!` is Zephyr's own `kernel/dynamic.c` — a dynamic thread stack
being reused while still live. Six of them, at consecutive tids, i.e. a pool of
threads, not one stray.

The cyclonedds submodule is pinned at

```
a09babf3 ddsrt: Zephyr-native sync backend — k_mutex/k_condvar instead of pooled pthreads
```

which is exactly the code that changed how ddsrt creates threads and
synchronisation primitives on Zephyr, and it landed today. Related: issue 0371
found the root cause of an earlier Zephyr Cyclone crash to be "the Zephyr
pthread mutex pool" — the same seam this commit rewrites.

`rc=-100` is the entry's own failure code from `run_components`.

## Not investigated further, deliberately

The suspect is a vendored FORK commit authored hours ago, in an active
migration. The last two times this session paused on that author's in-flight
work rather than patching it, they landed the fix themselves within the hour
(the RFC-0073 clock rename, then #548). Reporting beats a competing patch inside
their fork.

What a fix needs to establish first: whether the six `tid in use` errors are
fatal to participant creation or incidental, and whether `dds_create_participant`
returning `49379019` (a handle, not an error) means the failure is downstream of
it — the entry reports rc=-100 AFTER the participant is created.

## Reproduce

```
cargo nextest run -p nros-tests --test zephyr example_e2e::case_17_cyclonedds_c_action_e2e --no-capture
```

Both cases, C and C++, fail identically. `zephyr/rust` Cyclone action
(`case_16`) is worth checking as a control — if it passes, the fault is
language-path specific rather than in the backend.


## Phase-358 W5, 2026-08-15 — the `tid … is in use!` errors are INCIDENTAL, and they leak a stack each

The issue said a fix must first establish "whether the six `tid in use` errors
are fatal to participant creation or incidental". They are incidental, and the
whole chain reads out of source — no guessing, no guest needed:

1. **cyclonedds** (`src/ddsrt/src/threads/posix/threads.c`, `ddsrt_thread_create`)
   does the ordinary POSIX sequence:
   `pthread_attr_setstacksize(&attr, …)` → `pthread_create(…, &attr, …)` →
   `pthread_attr_destroy(&attr)`.
2. **Zephyr's POSIX layer** allocates the thread stack in
   `pthread_attr_setstacksize` (`k_thread_stack_alloc` into `attr->stack`,
   `lib/posix/options/pthread.c`).
3. `pthread_attr_destroy` then calls `k_thread_stack_free(attr->stack)` — on the
   stack the just-created, still-running thread is executing on. `dynamic.c`
   walks the thread list, finds a live owner that is neither `_THREAD_DUMMY` nor
   `_THREAD_DEAD`, logs **`tid %p is in use!`** and returns `-EBUSY` WITHOUT
   freeing.
4. `pthread_attr_destroy` **ignores that return**, zeroes the attr and returns
   `0`. So the caller sees success, the thread keeps its stack, and nothing about
   participant creation is harmed.

So the six lines are noise from the standard create-then-destroy-attr idiom, one
per ddsrt thread. `rc=-100` (`NROS_CPP_RET_TRANSPORT_ERROR`) is NOT caused by
them, which matches the other clue the issue already flagged: the participant
handle `49379019` is a handle, not an error, so the failure is downstream.

### The part worth fixing anyway: each one leaks 32 KB

`posix_thread_recycle` frees a dead thread's stack only when the CALLER did not
destroy the attr:

```c
if (t->attr.caller_destroys) {
        t->attr = (struct posix_thread_attr){0};   /* caller owns it — don't free */
} else {
        (void)pthread_attr_destroy((pthread_attr_t *)&t->attr);
}
```

The caller DID destroy it (step 3), and that destroy failed to free. So neither
path ever releases the stack: it leaks for the life of the image, at
`CONFIG_DYNAMIC_THREAD_STACK_SIZE` = **32768 bytes** per ddsrt thread. Six
threads at boot = ~192 KB out of the 4 MB `CONFIG_HEAP_MEM_POOL_SIZE`, so it is
survivable at boot and gets worse with every thread the backend creates.

This is a Zephyr-POSIX/ddsrt interaction, not a nano-ros defect, and it is
adjacent to the pool-exhaustion class in issues 0371/0496 — same seam, different
resource.

### Still open

What actually returns `-100`. It is downstream of `dds_create_participant` and
of these six lines.


## Phase-358 W5 — the hiding was two layers deep

**Layer 1 (test side) — fixed and verified on the guest.** The verdict now leads
with the guest's own error instead of the wait that observed it:

```
[cyclonedds/c/Action] action-server FAILED AT BOOT: nros zephyr entry: run_components failed rc=-100
  (readiness marker `Waiting for action goals` never arrived; the 60 s wait
   observed the failure, it did not cause it)
```

Getting there needed one correction. `first_guest_failure` scanned in LINE
order, so it led with `<err> os: tid … is in use!` — the benign line, four lines
above the real one. It is rank-major now: `GUEST_FAILURE_SIGNATURES` is a
precedence list, an entry's own error code outranks a kernel log line, and
`is in use!` is ranked deliberately low. Within one signature it still takes the
first match, so 0552's fault-then-register-dump still reads correctly.

**The control matrix says what this is not.** All three pass on the same
backend, same board, same fixture build:

| cell | result |
| --- | --- |
| `case_11` cyclonedds / **C** / pubsub | PASS |
| `case_14` cyclonedds / **C** / service | PASS |
| `case_16` cyclonedds / **Rust** / action | PASS |
| `case_17` cyclonedds / **C** / action | FAIL rc=-100 |

So not the backend, not the Zephyr sync port, not C, not actions. It is C/C++ ×
action specifically — and the Rust action passing means the RMW can create these
entities on this board.

**Layer 2 (guest side) — `-100` was itself a collapse.** The C example's
`server_configure` returns `nros_cpp_action_server_create`'s rc verbatim, and
that function ended with:

```rust
Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
```

`register_action_server_raw` returns a `NodeError`, and it was thrown away.
`-100` is documented as "the catch-all for unmapped variants", so the guest was
reporting *transport error* for a cause that need not be transport at all —
exactly the collapse issue 0436 fixed for `nros_cpp_init`, using a mapper
(`node_error_to_cpp_ret`) that even names the variant on the error path. Applied
there, not here. Now applied here.

### The class, not just the site

`grep -rn 'Err(_) => NROS_CPP_RET_TRANSPORT_ERROR' packages/api/nros-cpp/src/`
finds **16** such sites across `publisher.rs`, `subscription.rs`, `service.rs`
and `action.rs`. Only the action-server one is fixed here, deliberately: the
others' fallible calls return different error types (`create_publisher`,
`commit_slot`, `send_request_raw`, …) and rewriting them blind would be guessing
at mappings. Filed as a sweep rather than done wrong.


## RESOLVED 2026-08-15 — one doubled underscore, defeating the guard written to prevent it

Nothing to do with ddsrt thread reuse, the Zephyr sync backend, or the `tid … is
in use!` lines. `ros_form_to_dds` appended a trailing `_` unconditionally, to a
name that already had one:

```
type = 'example_interfaces/action/Fibonacci_SendGoal_'   <- already ends in '_'
dds  = 'example_interfaces::action::dds_::Fibonacci_SendGoal__'
base = 'example_interfaces::action::dds_::Fibonacci_SendGoal__SendGoal_'
req  = '..._SendGoal__SendGoal_Request_'   MISS   (registry has 51 entries)
```

The second underscore is what did the damage. `action_effective_base` carries an
idempotence guard from issue #234 — "detect an already-suffixed form and pass it
through unchanged" — implemented as a `_SendGoal_` suffix test. With the tail
mangled to `endGoal__` that test no longer matched, so the infix was appended a
SECOND time, producing exactly the doubled `<A>_SendGoal_SendGoal_` shape #234
existed to prevent. A guard walked past by a change one layer up.

**Why only C/C++.** `ros_form_to_dds` returns early, unchanged, when the type has
no `/`. Rust advertises the DDS form (`example_interfaces::action::dds_::…`) and
never reaches the append; the C/C++ path advertises the ROS form and does. That
is the entire C-vs-Rust divergence, and it is why cyclonedds/C/pubsub,
cyclonedds/C/service and cyclonedds/Rust/action all passed while
cyclonedds/{C,C++}/action failed.

**Fix.** Append the trailing `_` only when it is not already present — the
convention is EXACTLY one, which is what `service_type_name` assumes when it
strips one before adding `_Request_`.

**Verified**, after a clean rebuild of every Cyclone leaf:

```
PASS  case_14_cyclonedds_c_service_e2e
PASS  case_18_cyclonedds_cpp_action_e2e     <- was failing
PASS  case_17_cyclonedds_c_action_e2e       <- was failing
PASS  case_16_cyclonedds_rust_action_e2e    <- control, still green
```

~8 s each, against a 60 s readiness timeout. The guest reaches
`Waiting for action goals`, and the trace shows both descriptors HIT, the topics
and the reader/writer created, then the same for `cancel_goal` and `get_result`.

### What the six `tid … is in use!` lines were

A red herring, established earlier in this issue and left standing: benign, one
per ddsrt thread, leaking 32 KB each. They print before the real failure, which
is why the verdict's line-order ranking had to become precedence ranking.

### Debt this exposed, filed separately

* **#586** — 15 more `Err(_) => TRANSPORT_ERROR` sites in the C++ FFI.
* **#589** — on `native_sim`, any Rust `println!`/`eprintln!` recurses forever in
  `zvfs_write` and SIGSEGVs the image. Found by adding a diagnostic here.
