# phase-364 — the platform ABI states its own sizes, tasks and refusals

**Status (2026-08-16). W1 IN PROGRESS. Implements [RFC-0076](../design/0076-platform-abi-ask-do-not-assume.md).**
The ABI cannot say how big its own objects are, what a task should look like, or
that a platform simply does not do something — so callers guess, and nothing
checks the guesses. This phase makes each of those things sayable.

## Why this phase exists

Two phase-359 work items hit the same wall in one week and both worked AROUND
the ABI rather than through it:

* **W7** needed a NuttX tier task with a chosen stack size and wrote a bespoke C
  shim (`nros_nuttx_spawn_tier`), because the ABI has no portable way to ask for
  one.
* **W10** needed the same thing generically and had to add
  `nros_platform_task_storage_{size,align}` first, because `task_init`'s
  contract says "size determined by the implementor" and offered no way to ask.

Both fixes were local and correct. RFC-0076 records the five general defects
behind them; this phase migrates them.

## Work items

Ordered so each is landable alone and nothing is half-migrated at any commit.
M-numbers match RFC-0076's migration table.

### W1 (M1) — one failure vocabulary, actually used

`NROS_PLATFORM_RET_{OK,ERROR,UNSUPPORTED}` is declared in the header and used by
**no port**; 26 entry points return a bare `int8_t` 0/-1. So "this platform never
does this" and "this failed just now" are the same value.

* Narrow `nros_platform_ret_t` from `int32_t` to `int8_t`. It has no users, so
  this costs nothing and makes the declared type match the width the functions
  already return.
* Define the codes inside that width: `OK 0`, `ERROR -1`, `UNSUPPORTED -5`,
  `NOMEM -6`, `INVALID -7`, `TIMEOUT -8`.
* Ports return the specific code where they know it. The bare-metal ports'
  `-1 // Cannot create threads on single-threaded platform` becomes
  `UNSUPPORTED`, which is what it always meant.

**Compatible by construction**: every existing caller tests `!= 0`, and every
new code is non-zero. No caller reads the distinction yet — W3 is the first.

**Acceptance**: no port returns a bare `-1` where it knows better; a caller can
distinguish `UNSUPPORTED` from `NOMEM`; `check-abi-bindings` regenerated.

### W2 (M2) — storage sizing, in both the forms callers need

Add `storage_size`/`storage_align` for `mutex`, `mutex_rec` and `condvar`,
matching `wake` and `task`.

**Runtime probes are not sufficient, and that is the design point.** zenoh-pico
embeds `_z_mutex_t` *by value*, so it needs the size at COMPILE time; a function
call cannot size an array. So each port also publishes
`NROS_PLATFORM_<OBJ>_STORAGE_{SIZE,ALIGN}` macros and asserts, once, that the
macro and the type agree:

```c
_Static_assert(NROS_PLATFORM_MUTEX_STORAGE_SIZE >= sizeof(nros_mutex_t),
               "macro and type disagree");
```

Then delete `zpico-sys`'s hand-computed table — `TX_THREAD ≈ 232`,
`pthread_mutex_t = 40`, "2× safety margin" — which is a comment about other
platforms' Kconfig-dependent structs, checked by nobody.

**Acceptance**: the zenoh adapter compiles on every RTOS lane with no
hand-written size; a deliberately-wrong macro fails the static assert.

### W3 (M3) — a public task attribute, with `NULL` meaning defaults

Define `nros_platform_task_attr_t` in the ABI header (name, `stack_bytes`,
`stack_mem`, `priority`, `core`, `flags`) plus `nros_platform_task_attr_init`,
and delete the three private structs.

Rules that make it portable rather than merely shared:

* **`attr == NULL` means every default, on every port.** ThreadX's mandatory
  `attr` becomes a port-side default (allocate a default stack, or return
  `UNSUPPORTED`). This is a behaviour change to a shipped port.
* **`stack_bytes` is always bytes.** FreeRTOS divides by `sizeof(StackType_t)`;
  the unit conversion belongs in the one port that needs it.
* **`attr_init` rather than designated initialisers**, so a later field is
  source-compatible for out-of-tree ports.

Then: W7's `nros_nuttx_spawn_tier` collapses into a plain `task_init`, and
phase-359 W10's `PlatformTask::spawn` stops passing `NULL` — which is what makes
it correct on ThreadX, where it is a guaranteed `-1` today.

**Acceptance**: NuttX + FreeRTOS + ThreadX QEMU cells; the ThreadX `NULL` path
must be RUN, not just built.

### W4 (M4) — generate the export list, so the two provider kinds cannot drift

A platform provides the ABI as a hand-written `platform.c` or as Rust trait
impls with symbols emitted by `nros_platform_export!`. Nothing keeps them in
step: phase-359 W10 added the task probes to the header and all five C ports and
NOT to the macro, so the three Rust ports do not export them **on `main`
today**.

Generate the macro's symbol list from the header (which RFC-0054 already makes
the SSoT for the consumer side) and gate that the exported set equals the
declared set.

**Acceptance**: the gate must FAIL on the pre-W4 tree — the W10 probes are its
fixture — and pass after. A symbol declared and not exported becomes
unrepresentable, not merely fixed.

### W5 (M5) — normalised priority, with a raw escape

`os_pri` is documented as "platform-native", and the natives disagree: `0` is the
HIGHEST priority on ThreadX and the LOWEST on FreeRTOS, for a number authored
once in `system.toml` and deployed to both.

Define one band (`0` least urgent, larger more urgent, `INT32_MIN` inherit),
require each port to document and implement its map, and keep
`NROS_PLATFORM_TASK_PRIORITY_RAW(n)` for tuning a specific RTOS against its own
documentation.

**Last on purpose**: it is the only change here whose blast radius is
behavioural rather than structural — it changes what an authored number means on
ThreadX.

**Acceptance**: `realtime_tiers_e2e` on NuttX and ThreadX. The inversion is only
observable at runtime.

## Dependencies

W1 and W2 are independent of everything and of each other. W3 depends on W1 (it
returns `UNSUPPORTED`). W4 should land before W5, so the priority change reaches
the Rust ports through the generator rather than by hand.

## Risks

* **Every port is touched** — five C ports, three Rust ports, the C test stub,
  the zenoh adapter. Individually small and mechanical; one commit would not be
  reviewable.
* **ThreadX changes behaviour in W3.** That is the point, and it wants a runtime
  check on QEMU rather than a compile.
* **No out-of-tree ports are known**, so the `attr` break has no external cost
  today; `attr_init` is what keeps later additions source-compatible if that
  stops being true.
