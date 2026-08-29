---
id: 900
title: "Every executor arena slot is budgeted at the ActionClient worst case, so a pub/sub-only image carries ~56 KiB it cannot use"
status: open
area: core, memory
severity: medium
found: 2026-08-29
related: [0896, 0271, 0739, phase-392]
---

# The arena is sized for the entity an image does not have

## What was measured

`ARENA_SIZE` is **74,240 bytes on every generated config in the tree**, without
variation:

| image | `MAX_CBS` | `DEFAULT_RX_BUF_SIZE` | `ARENA_SIZE` |
| --- | ---: | ---: | ---: |
| `threadx-linux/rust/talker` | 4 | 1024 | 74,240 |
| `workspaces/c` (xrce) | 4 | 1024 | 74,240 |
| `workspaces/realtime-c` (nuttx riscv32imac) | 4 | 1024 | 74,240 |
| `workspaces/realtime-cpp` (nuttx riscv32imac) | 4 | 1024 | 74,240 |

The first row is a TALKER. It publishes on a timer and owns no action client,
no action server, and no service. It carries the same arena as everything else,
on a 32-bit embedded target as on a host.

## Why

`nros-node/build.rs:90-135` derives the arena by budgeting EVERY slot at the
largest entity that could occupy one:

```rust
const ACTION_CLIENT_PER_SERVICE:   usize = 4096 + 384;
const ACTION_CLIENT_SERVICES:      usize = 3;
const ACTION_CLIENT_FEEDBACK_SUBS: usize = 3;
const ACTION_CLIENT_SUB_OVERHEAD:  usize = 1536;
const ARENA_BASE_OVERHEAD:         usize = 2048;

let per_entry = ACTION_CLIENT_SERVICES * ACTION_CLIENT_PER_SERVICE
              + ACTION_CLIENT_FEEDBACK_SUBS * rx_buf_size
              + ACTION_CLIENT_SUB_OVERHEAD;
let derived_arena = (max_cbs * per_entry + ARENA_BASE_OVERHEAD).max(ARENA_FLOOR);
```

At the defaults: `per_entry` = 14,976 + 3·1024 = 18,048, and
`4 × 18,048 + 2,048` = **74,240**. The build script says so itself:

> Subscription / service entries are strictly smaller, so budget every slot at
> the action-client size.
>
> Embedded targets that never instantiate an `ActionClient` can override the
> derived size with `NROS_EXECUTOR_ARENA_SIZE`. A pub/sub-only workload only
> needs `3 × rx_buf + 512` per entry.

Taking that note at its word, a pub/sub-only image needs
`4 × (3·1024 + 512) + 2,048` = **16,384** bytes. The difference is
**57,856 B (~56.5 KiB)** carried by every image with no action client — which
is most of them.

## Two things make this worse than a loose default

**The rx buffer is amplified 12x.** `rx_buf_size` enters `per_entry` THREE
times (goal/result/feedback), and `per_entry` is multiplied by `MAX_CBS`. So
`NROS_SUBSCRIPTION_BUFFER_SIZE` is charged `3 × MAX_CBS` = 12x into the arena at
the defaults, on top of its per-subscription cost. Anyone raising that knob to
fit one large message type pays for it twelve times over here. That coupling is
also why issue 0896's per-type receive sizing cannot help this number: the arena
slot is sized from the GLOBAL knob regardless of what any individual
subscription needs.

**The escape hatch is a knob nobody can find.** `NROS_EXECUTOR_ARENA_SIZE`
exists, is honoured, and has a Kconfig sentinel — but nothing computes a right
value for an image, nothing warns when the derived one is 4x what the image
uses, and the correct replacement (`3 × rx_buf + 512` per entry) appears only in
a build-script comment. That is the shape of issues 0271 / 0739: "a knob nobody
can enumerate is a knob nobody sets", which cost ~145 KB in one image.

## Where the bytes land: THE STACK, not `.bss` (measured)

The first draft of this issue guessed `.bss` via the C API's
`nros_executor_t._opaque`. `just mem-report` on
`examples/workspaces/c/build/posix-zenoh-native/cmake/native_entry` refutes it —
there is **no arena symbol in the image at all**. Its RAM is the zenoh pools
(`SERVICE_BUFFERS` 144,128 / `LARGE_PAYLOADS` 131,072 / `SMALL_PAYLOADS` 32,768)
and nothing resembling 74,240 appears.

(That run also printed a STALE-IMAGE banner, so its byte counts describe an
older tree and are quoted here only as shape. The ABSENCE of an arena symbol is
structural, not a staleness artifact.)

The reason is written down one lane over, in
`docs/reference/platform-implementation-notes.md:143`:

> Stack overflow -> "Invalid mbox": `Executor` has an inline
> `arena: [MaybeUninit<u8>; ARENA_SIZE]` **on the task stack**. Action examples
> use `NROS_EXECUTOR_ARENA_SIZE=8192`; `APP_TASK_STACK` must be 16384 words
> (64 KB) for headroom.

The board path — which is what every example uses — builds the executor through
`Executor::open_sized` (`nros-board-linux/src/lib.rs:325`) with the arena inline
in the value, so it lives wherever that value lives: a task stack. The C API's
`_opaque` route is the L1 polling path, a different and less-travelled one.

This makes the defect worse than a static-RAM overshoot in two ways:

* **Stack is the scarcest resource on an RTOS target**, and a 74,240-byte frame
  is not something a per-task stack absorbs quietly. It is charged to whichever
  task calls spin, and it interacts with issue 0667's finding that a task's
  `stack_bytes` is a floor the port raises rather than a number the caller
  controls.
* **The workaround is already load-bearing in the tree.** FreeRTOS action
  examples pin `NROS_EXECUTOR_ARENA_SIZE=8192` — a 9x reduction from the
  derived value — and STILL need a 64 KB app task stack. That override is
  evidence the derivation does not describe real images: someone already had to
  discover the right number by hitting "Invalid mbox" and working backwards.

So the saving is not (only) ~56 KiB of static RAM; it is stack headroom on every
image that spins, and the difference between an image that boots and one that
dies in an allocator with an unrelated-looking error.

**Still measure rather than derive.** `mem-report` reads `.bss`/`.data` and
therefore cannot see this at all — sizing the change needs a stack-usage probe
(worst-case frame at the spin call, or a high-water mark on a running RTOS
image), not a symbol table.

## Measured on a live executor: 32 bytes of 74,240

The derivation's error is not the ~4.5x this issue first estimated from the
pub/sub-only formula. Opening a real `Executor` and registering one timer
(`nros-tests/tests/component_dispatch.rs::executor_arena_is_over_provisioned_for_a_timer_only_image`)
claims **32 bytes of 74,240** — a factor of **2,320**. The arena is a BUMP
allocator (`Executor::arena_alloc` charges `size_of::<T>()` and reserves
nothing per slot), so that number is exact, not a lower bound.

That reframes the fix: the allocator has always known the right answer and had
no way to say it.

## Landed: the executor can now say what it uses (W1)

* `Executor::arena_used()` / `Executor::arena_capacity()` — public accessors,
  ledgered as extensions (upstream has no arena to ask about; rclcpp
  heap-allocates each entity).
* A one-shot advisory on the first `spin_once` when the arena is grossly
  over-provisioned, naming the `NROS_EXECUTOR_ARENA_SIZE` value to set. Emitted
  through `nros_log`, never stdio (issue 0589), from a process-scoped static
  rather than an `Executor` field — the same reason `DROPPED_TAKES` is a static:
  a field would move `EXECUTOR_OPAQUE_U64S` and every image's executor footprint
  to buy a diagnostic. Process scope is also correct here rather than merely
  cheap, since the value it names is a build-time constant identical for every
  executor in the image.
* The threshold is a separate `const fn arena_is_over_provisioned` so it is
  testable — the reporter is one-shot, so a test calling it twice would silently
  check nothing the second time. Four unit tests cover the boundary, a
  well-sized arena staying quiet, overflow, and the issue-0460 zero-capacity
  sentinel (a fault, NOT headroom — calling it over-provisioned would bury a
  fatal misconfiguration under an advisory about wasted space).

This does not shrink anything by itself. It converts the folklore into a number
a user can read, which is the precondition for the rest.

**Not yet verified:** that the advisory line actually reaches a console on a
real image. It did not appear in the `component_dispatch` run, most likely
because a bare `cargo test` installs no `nros_log` sink — unconfirmed, and worth
one check before relying on it in the field.

## Direction for the rest

## Not to be confused with

Issue 0896, which is about a SUBSCRIPTION's receive buffer taking the small size
class because nothing states a per-type bound. This is one level up: even with a
perfect per-type hint, the arena slot holding that subscription is still
budgeted as though it were an action client. The two share the
`NROS_SUBSCRIPTION_BUFFER_SIZE` knob and nothing else.
