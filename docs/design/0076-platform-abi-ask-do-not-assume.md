---
rfc: 0076
title: "The platform ABI: a caller asks, and never assumes"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0076 — The platform ABI: a caller asks, and never assumes

## Summary

`<nros/platform.h>` is the seam every RTOS reaches nano-ros through. It works
for the callers it grew up with — C code that knows its own platform's types at
compile time — and fails for the caller it now has: portable Rust that must
spawn a task, size a lock, or decide whether a capability exists at all.

Two work items hit this within one week and both worked *around* the ABI rather
than through it. phase-359 W7 needed a task with a chosen stack size on NuttX
and wrote a bespoke C shim (`nros_nuttx_spawn_tier`) because the ABI cannot
express one. phase-359 W10 needed the same thing generically and had to add
`nros_platform_task_storage_{size,align}` first, because `task_init`'s contract
says "size determined by the implementor" and offered no way to ask.

Each fix was local and each was correct. The defects behind them are general,
and this RFC proposes fixing them as a class.

**The principle:** *a caller asks; it never assumes.* Sizes, capabilities and
failures are all things the ABI must be able to state, because the alternative
— a caller guessing another platform's struct layout — is the exact shape of
issue 0570, where Rust's 20-byte `pthread_attr_t` met NuttX's 56-byte one and
`pthread_attr_init` wrote the difference into the caller's frame.

## The five defects

Each is stated with the evidence that establishes it, because several look like
style questions until you see what they already cost.

### D1 — Opaque storage has no portable sizing

The ABI hands out five opaque object types: `task`, `wake`, `mutex`,
`mutex_rec`, `condvar`. Exactly two can be sized by a caller — `wake` (phase
130) and `task` (phase-359 W10, and only because it was in the way).

For the other three, a caller that does not know the platform must guess. One
does, in the tree, today. `zpico-sys`'s generic adapter carries this:

```
 *   - `_z_task_t`: 256 B   (POSIX pthread_t ≤ 8;
 *                           FreeRTOS TCB pointer + attrs ≈ 32;
 *                           ThreadX TX_THREAD ≈ 232)
 *   - `_z_mutex_t`: 64 B   (POSIX pthread_mutex_t = 40;
 *                           Zephyr k_mutex ≈ 32;
 *                           FreeRTOS xSemaphoreHandle ≈ 8)
 *   - `_z_condvar_t`: 64 B (POSIX pthread_cond_t = 48;
 *                           Zephyr k_condvar ≈ 32;
 *                           FreeRTOS event group ≈ 32)
```

Hand-computed worst cases across platforms, with `≈`, and a stated "2× safety
margin". That margin is doing real work — and it is a comment, checked by
nobody, about structs whose size is Kconfig-dependent on at least two of those
platforms.

### D2 — `attr` is unspecified, and the ports disagree on whether it is optional

`nros_platform_task_init(void *task, void *attr, entry, arg)`. What `attr`
points at is not defined anywhere in the ABI. In practice:

| port | `attr` handling |
| --- | --- |
| posix | `(void) attr;` — ignored |
| zephyr | `(void) attr;` — ignored |
| freertos | reads a private `nros_freertos_task_attr_t` if non-NULL |
| esp-idf | reads a private `nros_esp_task_attr_t` if non-NULL |
| threadx | `attr == NULL` → **hard failure**; requires `stack_base` + `stack_depth` |

Three private structs, no shared definition, one port for which the parameter is
mandatory and two for which it is inert. The consequences are concrete:

* **A portable caller cannot set a stack size.** That is why W7 wrote a C shim:
  a Rust tier spawn needed 64 KiB and the ABI had no way to say so.
* **W10's `PlatformTask::spawn` passes `NULL`** — correct on four ports, and a
  guaranteed `-1` on ThreadX. Neither feature that uses it is enabled on ThreadX
  today, so this is latent rather than broken, but it is broken by construction.
* **`stack_depth` is not one unit.** FreeRTOS's `xTaskCreate` takes *words*;
  ThreadX and the rest take *bytes*. The same field name means different things
  in two of the three private structs.

### D3 — Capability negotiation happens at link time, or not at all

The header defines a return vocabulary:

```c
#define NROS_PLATFORM_RET_OK          ((nros_platform_ret_t) 0)
#define NROS_PLATFORM_RET_ERROR       ((nros_platform_ret_t) -1)
#define NROS_PLATFORM_RET_UNSUPPORTED ((nros_platform_ret_t) -5)
```

**No port uses it.** Twenty-six entry points return a bare `int8_t`, 0 or -1.
`grep` finds `NROS_PLATFORM_RET_*` in the header that defines it and nowhere
else in the platform layer.

So "this platform cannot do this, ever" and "this failed just now" are the same
value. The bare-metal port says so in a comment it cannot say in its return:

```rust
-1 // Cannot create threads on single-threaded platform
```

That distinction is not academic. Issue 0246 is a **transient** `pthread_create`
failure on NuttX under load, which the tier spawn retries. W10's worker pool has
to decide whether a refusal is permanent, cannot ask, and therefore records
every refusal as permanent — disabling a priority level for the process on what
may have been a momentary ENOMEM.

Below that, capability is negotiated by *link error*: `nros-platform-mps2-an385`
and `-stm32f4` implement neither the task nor the wake family, so a caller that
references them fails at link with `undefined symbol`, on a platform where the
honest answer is `UNSUPPORTED`.

### D4 — Two provider kinds, and only one of them is checked — **WRONG, corrected 2026-08-16**

A platform provides the ABI in one of two ways:

* a hand-written `platform.c` (posix, freertos, threadx, zephyr, esp-idf, nuttx);
* a Rust `impl` of the `nros-platform-api` traits, with the symbols emitted by
  `nros_platform_cffi::nros_platform_export!` (mps2-an385, stm32f4, esp32-qemu).

Adding a symbol means touching both, and nothing enforces it. W10 added the task
storage probes to the header and to all five C ports **and not to the export
macro** — so the three Rust ports do not export them, and a caller linking one
gets undefined symbols. That gap is live in `main` as this is written.

> **This defect does not exist.** `scripts/check-platform-abi-mirror.sh` has
> gated exactly this since Phase 121.4.b, is wired into `just check`, and checks
> a superset (the `extern "C"` block as well as the export macro, across all
> three platform headers). The W10 gap was real, but it was a gate that had not
> been RUN — that session's tier 1 stopped at its preconditions — not a gate that
> was missing. Found when phase-364 W4 tried to build a second one; W4 is
> withdrawn and C5 below is unnecessary. Recorded rather than deleted, because
> "nothing enforces it" was an assumption this RFC made without checking, and
> the same assumption is cheap to make again.

### D5 — Priority is platform-native, and the natives disagree

`SchedPolicy::Fifo { os_pri: u8 }` documents `os_pri` as "the platform-native
numeric priority". The natives are not comparable:

| platform | direction | range |
| --- | --- | --- |
| ThreadX | **0 = highest** | `0..TX_MAX_PRIORITIES-1` |
| FreeRTOS / ESP-IDF | higher = more urgent | `0..configMAX_PRIORITIES-1` |
| POSIX / NuttX `SCHED_FIFO` | higher = more urgent | `1..99` (platform-dependent) |
| Zephyr | lower = more urgent; negative = cooperative | `-CONFIG_NUM_COOP..CONFIG_NUM_PREEMPT` |

A tier priority is authored once, in `system.toml`, and deployed to several of
these. The same number means "most urgent" on ThreadX and "least urgent" on
FreeRTOS. Nothing in the ABI records which convention a port uses.

## Proposal

Four changes, ordered so each is independently landable and each leaves the tree
working.

### C1 — Uniform storage sizing, in both the forms callers need

Add `nros_platform_<obj>_storage_size()` / `_align()` for `mutex`, `mutex_rec`
and `condvar`, matching `wake` and `task`.

Runtime probes alone are **not sufficient**, and this is the part worth getting
right: zenoh-pico embeds `_z_mutex_t` *by value* in its own structs, so it needs
the size at **compile** time. A function call cannot size an array.

So each port additionally publishes compile-time macros in its own header:

```c
#define NROS_PLATFORM_MUTEX_STORAGE_SIZE   40
#define NROS_PLATFORM_MUTEX_STORAGE_ALIGN  8
```

and asserts the two agree, once, in the port:

```c
_Static_assert(NROS_PLATFORM_MUTEX_STORAGE_SIZE >= sizeof(nros_mutex_t),
               "macro and type disagree");
```

C consumers that embed by value use the macro; Rust and any dynamic allocator
uses the probe; the static assert makes a divergence a compile error in the port
that owns both. The zenoh adapter's hand-computed table is then deleted rather
than maintained.

### C2 — A public task attribute struct, with `NULL` meaning "defaults"

Define it in the ABI header, once:

```c
typedef struct {
    const char *name;        /* NULL = port default                       */
    size_t      stack_bytes; /* 0 = port default. ALWAYS bytes.           */
    void       *stack_mem;   /* NULL = port allocates                     */
    int32_t     priority;    /* normalised; see C4. INT32_MIN = inherit   */
    int8_t      core;        /* -1 = unpinned                             */
    uint8_t     flags;       /* NROS_PLATFORM_TASK_DETACHED, …            */
} nros_platform_task_attr_t;

void nros_platform_task_attr_init(nros_platform_task_attr_t *attr);
```

Rules that make it portable rather than merely shared:

* **`attr == NULL` means "every default", on every port.** ThreadX's mandatory
  `attr` becomes a port-side default (allocate a default stack, or return
  `UNSUPPORTED` if it genuinely cannot), so the same call works everywhere.
* **`stack_bytes` is bytes.** FreeRTOS divides by `sizeof(StackType_t)`; the
  unit conversion belongs in the one port that needs it, not in every caller.
* **`stack_mem` exists because two ports need it.** ThreadX requires caller
  memory; Zephyr's native `k_thread` needs a `K_THREAD_STACK_DEFINE` region
  (today's Zephyr port sidesteps this by going through its POSIX layer, which is
  worth keeping visible rather than accidental).
* **`attr_init` rather than designated initialisers**, so adding a field later
  does not break source compatibility for out-of-tree ports.

The three private structs are deleted. W7's `nros_nuttx_spawn_tier` shim
collapses into a plain `task_init` call with an attr, and W10's `PlatformTask`
stops passing `NULL` — which is what makes it correct on ThreadX.

### C3 — One failure vocabulary, actually used

Keep the existing `int8_t` width (so this is source- and binary-compatible for
every caller that tests `!= 0`) and define the codes inside it:

| code | value | meaning |
| --- | --- | --- |
| `OK` | 0 | success |
| `ERROR` | -1 | unspecified failure |
| `UNSUPPORTED` | -5 | this platform never provides this |
| `NOMEM` | -6 | resource exhausted **now**; a retry may succeed |
| `INVALID` | -7 | caller passed something impossible |
| `TIMEOUT` | -8 | deadline expired (already `wake_wait_ms`'s semantics) |

`nros_platform_ret_t` is currently `int32_t` and has no users, so narrowing it to
`int8_t` costs nothing and makes the declared type match the 26 functions that
already return that width.

The distinction that matters is `UNSUPPORTED` vs `NOMEM`: a caller may cache the
first forever and must retry the second. That is precisely the decision W10's
pool gets wrong today, and issue 0246 is the case that punishes it.

### C4 — Normalised priority, with an escape hatch

Define one portable band — `0` = least urgent, larger = more urgent, `INT32_MIN`
= inherit the creator's — and require each port to document and implement the
map onto its native range. ThreadX inverts; FreeRTOS is identity-ish; Zephyr
maps onto its preemptive band and reserves negatives for cooperative.

Keep a raw escape (`NROS_PLATFORM_TASK_PRIORITY_RAW(n)`) for expert use, because
tuning a specific RTOS against its own documentation is legitimate and the
normalised band should not make it impossible. The tier vocabulary in
`system.toml` targets the normalised band; a per-platform override stays
per-platform, which is what `[tiers.<name>.nuttx] priority` already does.

### C5 — Generate the export macro's symbol list from the header — **UNNECESSARY (see D4)**

The header is already the ABI SSoT for the Rust *consumer* side (RFC-0054:
`generated.rs` is committed bindgen output, gated by `check-abi-bindings`). The
*provider* side has no such rule, which is why D4 happened.

Extend the same treatment: generate the `nros_platform_export!` symbol list from
the header, and gate that the exported set equals the declared set. Then a symbol
added to the header cannot be missing from the Rust ports — the W10 gap becomes
unrepresentable rather than merely fixed.

## What this RFC deliberately does not propose

* **Not a trait-object platform.** `Executor` is deliberately non-generic over
  the platform (`open_threaded`'s `fn(SchedPolicy)` pointer exists to keep it
  that way). The C ABI stays the seam; this RFC makes it expressive, not
  object-oriented.
* **Not a string-keyed capability query.** `UNSUPPORTED` plus per-symbol presence
  already answers "can this platform do X", and a query API would add a second
  way to ask the same question.
* **Not a new threading model.** Nothing here changes what a task *is*; it
  changes how a caller describes the one it wants.

## Costs and risks

* **Every port is touched** — five C ports, three Rust ports, the C test stub,
  and the zenoh adapter. The changes are individually small and mechanical;
  landing them as one commit would not be reviewable.
* **ThreadX changes behaviour**: `attr == NULL` stops being a failure. That is
  the point, but it is a semantic change to a shipped port and wants a runtime
  check on hardware, not just a compile.
* **No out-of-tree ports are known**, so the `attr` break has no external cost
  today. If that stops being true, `attr_init` is what keeps later additions
  source-compatible.
* **Priority normalisation is the one change with a behavioural blast radius**
  beyond the ABI: it changes what an authored number means on ThreadX. It is
  ordered last for that reason, and should land with the tier tests that can
  observe it.

## Migration

Sequenced so nothing is half-migrated at any commit; the phase doc carries the
work items.

| step | change | verification |
| --- | --- | --- |
| M1 | C3 — return vocabulary; narrow `nros_platform_ret_t`; ports return the specific codes | compile-only; no caller reads them yet |
| M2 | C1 — probes + compile-time macros + static asserts for `mutex`/`mutex_rec`/`condvar`; delete the zenoh adapter's hand-computed table | the adapter compiles on every RTOS lane; a deliberately-wrong macro must fail the static assert |
| M3 | C2 — public `nros_platform_task_attr_t`; `NULL` = defaults on every port; delete the three private structs; collapse `nros_nuttx_spawn_tier`; W10's `PlatformTask` passes a real attr | NuttX + FreeRTOS + ThreadX QEMU cells; the ThreadX `NULL` path is the one that must be run, not just built |
| M4 | C5 — generated export list + symbol-parity gate | the gate must fail on the current tree before it passes (the W10 task probes are the fixture) |
| M5 | C4 — normalised priority + raw escape | `realtime_tiers_e2e` on NuttX and ThreadX; the inversion is only observable at runtime |

M1 and M2 are independent of the rest and of each other. M3 depends on M1 (it
returns `UNSUPPORTED`). M4 should land before M5 so the priority change reaches
the Rust ports through the generator rather than by hand.
