# Phase 502 — No unbounded condition wait in code we own

**Status (2026-10-15).** W0–W5 DONE; issue 1196 is resolved. Only W6 (the
non-blocking upstream-worthy zenoh-pico fixes) is open.

**Measured (2026-10-15).** `just ci gate` green (6/6).

- **FreeRTOS (QEMU MPS2-AN385):** `condvar-contract PASS zenoh-wait:
  elapsed_ms=1000 bounds=[999,3000)` in 4 of 4 runs. With the guard off,
  `FAIL … elapsed_ms=10006 … rescued=true`. The 9 `rtos_e2e` FreeRTOS zenoh
  cells pass.
- **Zephyr native_sim zenoh:** every cell whose images were built passes —
  boot, Rust and C++ pub/sub, the Rust service, Zephyr↔native pub/sub and
  service in both directions, and the action completion rate. The Cyclone,
  XRCE and C images were not built, so their cells were not run.
- **Native zenoh:** the `services`, `actions` and `action_multigoal` suites
  pass.
 The ABI half is already done: phase-436 D2
(`77e5ddd628`) removed `nros_platform_condvar_wait` from the platform ABI, and
issue 1906 fixed the timeout codes, broadcast and tick rounding that a bounded
wait relies on. What this phase owns is the rest of issue 1196's closing
condition: the two zenoh-pico paths that never went through that symbol,
`zpico_get`, and a gate for the whole class.

**Owns:** [issue 1196](../issues/archived/1196-platform-condvar-wait-is-unbounded.md) (resolved 2026-10-15 by W0–W4).

**Implements:** [RFC-0076](../design/0076-platform-abi-ask-do-not-assume.md)
(the contract between the platform ABI and its callers), with the amendment in
W0. [RFC-0034](../design/0034-platform-layer-split.md) D2 is why FreeRTOS is
patched rather than routed through the alias shim (W2).

**Continues:** phase-436 Path D2, which did the ABI half and handed this
remainder to issue 1196.

## Why

"No unbounded wait" is only a property of the system if it holds for every
path, not only for the platform ABI. After phase-436 D2, the alias shim's
`_z_condvar_wait` (`zpico-sys/c/zpico/platform_aliases.c`) waits on
`nros_platform_condvar_wait_until` with a deadline it owns (1000 ms) and
returns an expired slice as a spurious wakeup. That covers POSIX, NuttX and
ThreadX. Three sites remain, none of which the current gate
(`scripts/check-no-unbounded-condvar-wait.sh`) can see, because it looks for
one symbol name:

| Site | Platform | Forever spelling |
| --- | --- | --- |
| `zephyr/nros_zenoh_zephyr_system.c` `_z_condvar_wait` (the shim's system section is skipped on Zephyr) | Zephyr | `pthread_cond_wait` |
| vendored `zenoh-pico/src/system/freertos/system.c` `_z_condvar_wait` (the full alias TU is excluded on FreeRTOS) | FreeRTOS | `xSemaphoreTake(cv->sem, portMAX_DELAY)` |
| `zpico-sys/c/zpico/zpico.c` `zpico_get` | all | a direct `_z_condvar_wait` call |

A bounded wait reported as a spurious wake changes no caller's behaviour,
because every caller loops on its predicate. Re-read in the zenoh-pico 1.8.0
submodule:

| Call site | Loops on its predicate? |
| --- | --- |
| `src/collections/fifo_mt.c:81` (push, not-full) | yes |
| `src/collections/fifo_mt.c:116` (pull, not-empty) | yes, and checks `is_closed` |
| `src/collections/ring_mt.c:106` | yes, and checks `is_closed` |
| `src/collections/sync_group.c:51` | yes |
| `src/utils/scheduler.c:98` | yes (`do … while`) |
| ours, `zpico.c` `zpico_get` | yes (`while (!ctx.done)`); deleted in W3 |

What the period buys is recovery: a lost signal costs one period instead of
the thread. Issue 1750's ThreadX rule refuses a signal from a foreign host
thread on the grounds that "a lost signal costs latency and nothing else";
that is true only once no waiter can wait forever.

## Work items

### W0 — RFC-0076: no unbounded condition wait

- [x] Add a contract clause to RFC-0076 (C6). The ABI has no condition wait without
      a deadline (phase-436 D2 made that true). Code we own never waits on a
      condition or a signalling semaphore without a deadline; a caller whose
      own contract is unbounded loops over bounded waits. Mutex acquisition
      stays unbounded-but-inheriting, an exemption valid only because issue
      1907 made priority inheritance real on every port. Thread join is a
      separate contract, also exempt.
- [x] Give issue 1196 a `**Home:** phase-502` line.

*Acceptance:* `check-roadmap-status` passes and the RFC records the clause.

### W1 — Zephyr's zenoh system file

- [x] `zephyr/nros_zenoh_zephyr_system.c` `_z_condvar_wait`: replace
      `pthread_cond_wait` with one `pthread_cond_timedwait` whose deadline is
      the monotonic clock plus the same 1000 ms slice the alias shim uses
      (the condvar is already created monotonic in the same file), and return
      a timeout as 0, a spurious wake.
- [x] The slice has one spelling: the shim's constant moves to a header both
      files include, rather than a second literal
      (`zpico-sys/c/zpico/nros_zp_condvar_slice.h`, reached from the Zephyr
      library through a private include directory).

*Acceptance:* the Zephyr `rtos_e2e` zenoh cells stay green, and W4's gate
fails on the old line (negative control) and passes on the new one.

### W2 — FreeRTOS: patch the vendored wait on the fork

**Decision: patch, not route through the shim.** On FreeRTOS `_z_condvar_t`
is zenoh-pico's own struct (`{mutex, sem, waiters}`) and `z_clock_t` is
`TickType_t`. The shim's condvar section needs the generic platform header's
layouts, and RFC-0034 D2 classifies condvar as an opaque-struct service where
a generic alias layout breaks the by-value ABI; the runner's own comment
(`nros-zpico-build/src/runner.rs`, the FreeRTOS scalar-only alias arm) is why
FreeRTOS was excluded. Moving FreeRTOS's sync types onto the platform ABI is a
far larger change than bounding one wait.

- [x] On the zenoh-pico fork's patch branch `nano-ros` (never `main`), guard
      `_z_condvar_wait` in `src/system/freertos/system.c` with a new
      `Z_FEATURE_NROS_BOUNDED_CONDVAR_WAIT`. Under the guard the take waits a
      finite tick count for the slice, rounded UP and clamped below
      `portMAX_DELAY` (issue 1906's rule). On timeout it deregisters itself
      (`waiters--` under `cv->mutex`) and returns `_Z_RES_OK`.
- [x] `runner.rs` defines the guard beside
      `Z_FEATURE_NROS_PLATFORM_{ALLOC,SLEEP,RANDOM}`.
- [x] Fork workflow: commit in the submodule, rebase on `origin/nano-ros`,
      push the fork branch first (linear, fast-forward), then move the pin
      forward (`check-submodule-pins`).

*Acceptance:* `nros-condvar-contract` (issue 1906's test crate) gains a check
that calls the zenoh layer's `_z_condvar_wait` with no signaller and asserts it
returns 0 after at least one slice and well under ten. It passes in the
`condvar-contract-freertos-mps2` image under QEMU, and fails (times out) with
the guard off. The `rtos_e2e` FreeRTOS zenoh cells stay green.

### W3 — Delete `zpico_get`

`zpico_get` has no caller: `git grep -nw zpico_get` finds its definition, its
Rust declaration (`zpico-sys/src/lib.rs`), its cbindgen stub
(`zpico-sys/src/ffi.rs`), the generated `c/include/zpico.h`, and prose in
nros-c and nros-cpp saying they never call it. Its wait cannot simply take a
deadline, because its reply context lives on the stack and returning early
would be a use-after-free. That is an ownership problem; deletion cures it.

- [x] Delete the function, the declaration and the stub, regenerate
      `zpico.h`, and delete the reply dropper and the `mutex`/`cond` fields of
      `get_reply_ctx_t` that only `zpico_get` used. `get_reply_handler` stays:
      the pending-get path reuses it.

*Acceptance:* the zenoh service and action e2e cells pass, and `just
mem-report` on one native and one embedded image shows the session shrinking
by `ZPICO_MAX_PENDING_GETS` × (mutex + condvar storage), quoted in the commit.

*Measured:* `g_sessions` shrank in both images (the before image was rebuilt from
`HEAD`'s `zpico.c`):

| Image | Before | After | Change |
| --- | --- | --- | --- |
| Zephyr native_sim `rust/talker` | 83,000 B | 82,968 B | −32 B |
| FreeRTOS mps2 `service-server` | 83,792 B | 82,768 B | −1,024 B |

Zephyr's pthread mutex and condvar are 4-byte handles, while FreeRTOS's are
static semaphore objects, which is why the saving differs so much. No native
image was measured.

The Zephyr figure matches 4 pending-get slots × (4 + 4) B; that breakdown
is inferred, not read from the build.

### W4 — A class gate for forever-waits

- [x] `check-no-unbounded-condvar-wait.sh` widens from one symbol to the
      class, over every tracked Rust and C/C++ file we own (`third-party/` and
      the zenoh-pico submodule excluded): `pthread_cond_wait(`,
      `k_condvar_wait(…K_FOREVER)`, `k_sem_take(…K_FOREVER)`,
      `tx_semaphore_get(…TX_WAIT_FOREVER)`,
      `tx_event_flags_get(…TX_WAIT_FOREVER)`, `xSemaphoreTake(<non-mutex>,
      portMAX_DELAY)` and a call of `_z_condvar_wait(`, reading the timeout
      argument across line breaks. It keeps refusing the retired
      `nros_platform_condvar_wait`.
- [x] Mutex acquisition and thread join are exempt by W0's rule, as named
      categories with a reason each. Anything else sits in an exemption table
      whose stale entries fail. **Correction:** the table is NOT empty. The
      widened reach found two sites this doc's survey missed, and the W2 test
      adds a third:
      - ThreadX zpico `_z_task_join` waits on `tx_event_flags_get(…,
        TX_WAIT_FOREVER)`. It is a join, so the join rule covers it (a hit
        inside a function whose name ends in `join`) and it needs no row.
      - Three rows remain, each a test whose call IS its subject: the shim
        slice test `condvar_shim_codes.rs`, the W2 check in
        `nros-condvar-contract`, and the host harness gate in
        `tests/zephyr/thread_slot_release_test.c`.
- [x] Self-test: plant each pattern, including the `zpico_get` shape and a
      multi-line `xSemaphoreTake`, and check each is rejected while a mutex
      `portMAX_DELAY` and a definition are accepted.
- [x] Confirm it runs on a merge-gating lane (`check-default-gates-run-somewhere`):
      the fast line, on `pull_request`, `merge_group` and `push`.

*Acceptance:* reverting W1's line makes the gate fail naming that file and
line; with W1 in place it passes with an empty exemption table.

### W5 — Issue 1750's comment

- [x] `nros-platform-threadx/src/platform.c`'s note on refusing a foreign
      thread's signal says every wait "takes a timeout or is woken by a ThreadX
      peer". After W1–W3 that is true of every condition wait; reword it to
      cite this phase and the slice that makes it true.

*Acceptance:* none beyond review; it is a comment.

### W6 — Upstream-worthy zenoh-pico fixes (non-blocking)

On a wait error four sites return with the mutex HELD:
`_Z_RETURN_IF_ERR(_z_condvar_wait(...))` in `fifo_mt.c:81,116` and
`ring_mt.c:106`, and the early `return ret` in `sync_group.c:103`. Issue
1907's refusal of a wait under a mutex held twice (-1 on ThreadX and Zephyr's
native arm) is a real way to reach them.

- [ ] A fix on the fork's `nano-ros` branch that unlocks before returning, as
      a commit of its own, offered upstream separately. Our branch does not
      wait on the upstream merge.

*Acceptance:* a host test drives `_z_fifo_mt_pull` into a wait error and
asserts the mutex is free afterwards. This item does not gate closing 1196.

## Test strategy by platform

| Platform | Harness | Items |
| --- | --- | --- |
| POSIX, NuttX | `condvar_shim_codes.rs` (already covers the shim's slice); `rtos_e2e` NuttX zenoh cells | regression only |
| ThreadX | `condvar-contract-threadx-linux` (issue 1906) | regression only |
| FreeRTOS | `condvar-contract-freertos-mps2` (issue 1906), extended in W2 | W2 |
| Zephyr | `rtos_e2e` Zephyr zenoh cells; W4's negative control | W1 |

Every slice check asserts a lower bound as well as an upper one: a wait that
returned at once would pass an upper-bound-only test while busy-spinning.

## Risks

- **Power.** A blocked waiter wakes once per slice. A signal still wakes it
  at once, so no latency is added. No in-tree path parks indefinitely on these
  waits in normal operation (fifo and ring channels and periodic tasks are
  unused, and `zpico_get` is deleted), which is inferred from the call-site
  table above, not measured.
- **The vendored FreeRTOS timeout path** used to be reached only by
  `_z_condvar_wait_until`; under W2 it runs once per slice, so its
  waiter-count bookkeeping is exercised far more often. W2's deregistration
  and its test are the evidence.
- **Fork push.** W2 needs the zenoh-pico fork branch pushed before the pin
  moves. Nothing else depends on it, but 1196 does not close without it.

## Non-goals

- Bounding mutex acquisition, which rests on priority inheritance (issue 1907).
- Routing FreeRTOS sync through the platform ABI (see W2's decision).
- Making the slice a knob.
- The vendored FreeRTOS `_z_condvar_wait_until`'s `target_time > now`
  comparison on `TickType_t`, which looks wrap-unsafe (inferred). A separate
  issue if confirmed.

## What closes issue 1196

W0–W4 landed: no code we own makes an unbounded condition wait outside the
named mutex and join categories, and the widened gate enforces it with an
empty exemption table. W5 and W6 may follow.
