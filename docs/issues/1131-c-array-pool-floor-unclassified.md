---
id: 1131
title: "One knob-sized C array has no ruling on whether zero is a legal size"
status: open
area: rmw, memory
severity: low
related: [1015, 1033, 1167, 0815, 0196, phase-392, phase-403, phase-412]
---

# The wider question issue 1015 left open, now down to one

Was fifteen. Fourteen are ruled; the one below is left UNRULED ON PURPOSE,
because it has a real argument that zero is the right answer and that argument
needs a measurement nobody has taken. A guard on it would foreclose a saving
exactly the way issue 1015's first fix foreclosed issue 1033's.

## Where the class stands

```
$ python3 scripts/check-c-array-pool-floors.py | tail -1
check-c-array-pool-floors: OK (23 knob-sized C arrays: 19 guarded, 3 zero-legal, 1 unclassified)
```

23 (not 21 — see "what the gate could not see" below): **19 guarded, 3
zero-legal, 1 unclassified**, against 8 / 3 / 10 before.

## What is left, and what it needs

| knob | file | why zero is arguable | what would settle it |
| --- | --- | --- | --- |
| `NROS_ZEPHYR_MAX_TIERS` | `zephyr/nros_platform_zephyr_shims.c` | `K_THREAD_STACK_ARRAY_DEFINE(nros_tier_stacks, N, 16384)` — **64 KiB** of stacks in every Zephyr image, declared tiers or not, because nothing produces this knob and 4 is what everything compiles. A tierless image setting 0 is `XRCE_MAX_SUBSCRIBERS=0`'s shape, and the refusal is loud AND FATAL, which is more than the table first claimed: `entry_tiers.rs` logs `failed to spawn tier … pool exhausted?` and then returns `Err(RuntimeError::Spin)`, so a tiered image at 0 fails rather than running short-handed. | a Zephyr build at `-DNROS_ZEPHYR_MAX_TIERS=0` proving `K_THREAD_STACK_ARRAY_DEFINE` accepts a count of 0, plus a `just mem-report <elf> --json --baseline` delta for the 64 KiB. **Not attemptable on a host with no Zephyr:** the macro lives in `zephyr/kernel.h`, which comes from the west workspace, so even reading its expansion needs a checkout this repo does not carry. |

It is in the gate's `UNCLASSIFIED` table with that reasoning inline, and the
`UNCLASSIFIED_CEILING` is now 1, so adding a second is a visible edit.

## Second pass: `NROS_RMW_UORB_PX4_MAX_CALLBACKS`, ruled GUARDED

The first pass left this one needing two things. One is still unavailable and
turned out not to matter; the other is answerable from the tree and answers the
question the other way round.

**The runtime half of the "zero is fine" argument is CORRECT, and it is not
enough.** The first pass suspected the polling fallback might be "a degradation
nobody reports". It is the opposite — it is the DEFAULT everywhere else:
`callback_default.cpp` supplies `__attribute__((weak))`
`nros_orb_register_callback` returning `-1` unconditionally, and that is what
every non-PX4 build links. `subscriber.cpp` handles the `-1` by pinning `ready`
true and falling through to `orb_check` on every poll — its own comment calls it
"same behaviour the pre-push-wake K.4.2 build had". So at 0 both range-`for`s
over `g_pool[0]` do not execute, every registration returns `-1`, and data still
flows. **Zero silences nothing here**, which is the discriminator issues 1015 and
1033 turn on.

**Zero still loses, on the language.** `Slot g_pool[0]` is a zero-size array,
which ISO C++ forbids — a GNU extension, and this is where the C case issue 1033
measured for the XRCE pools does not carry over. This TU is compiled
`-Wall -Wextra -Wpedantic` by `packages/rmw/uorb/nros-rmw-uorb/CMakeLists.txt`,
and PX4 builds every module `-Werror`; PX4 is the only build that compiles the
file at all, since it joins the sources only under
`NROS_RMW_UORB_BUILD_PX4_GLUE`, which requires `NROS_RMW_UORB_LINK_PX4`.
Measured on a reduction with those exact flags:

```
$ g++ -std=gnu++14 -Wall -Wextra -Wpedantic -Werror -fno-exceptions -fno-rtti -c probe.cpp
error: ISO C++ forbids zero-size array ‘g_pool’ [-Werror=pedantic]
$ g++ … -DPOOL_N=64 -c probe.cpp     # clean
```

**And the guard forecloses nothing, which is why it is safe to write.** The
saving zero was meant to buy — a polling-only image that does not pay for the
pool — already has a better spelling: `NROS_RMW_UORB_BUILD_PX4_GLUE=OFF` drops
the whole TU and links the weak stubs, so the pool is not empty but ABSENT.
Zero was a strictly worse way to ask for something the build system already
offers. That is the test issue 1015's first fix failed against issue 1033's
pools, and this one passes it.

The PX4 SDK is still absent (`third-party/px4/PX4-Autopilot` is an empty
submodule dir here), so `sizeof(CallbackAdapter)` is still unmeasured. It does
not change the ruling: it would quantify a saving that is unreachable at 0 for a
reason that has nothing to do with its size.

### The probe context this needed

`check-c-array-guard-probe` gained a `preprocess` entry for the file — PX4's
uORB and work-queue headers are an SDK checkout no host lane has. Its declared
enclosing condition is **`none`**, and that was MEASURED rather than assumed:
`NROS_RMW_UORB_USE_PX4_HEADER` is supplied for fidelity with the real TU, but
dropping it leaves the guard firing anyway, because the file's own
`#ifndef … #error` does not stop gcc preprocessing. Written down that way so a
reviewer is not misled into thinking the probe proves more than it does — the
opposite of the `-DZ_FEATURE_MULTI_THREAD=1` case, where dropping the
declaration does make the guard unreachable.

## What was ruled, and on what

Nine knobs got a `#if <KNOB> < N` / `#error` **below** their `#ifndef` fallback
(issue 1167's ordering rule — an undefined identifier reads as 0 in `#if`, so a
guard above its own default fires on every build):

* `XRCE_BUFFER_SIZE`, `XRCE_SUBSCRIBER_RING_DEPTH`,
  `XRCE_SERVICE_REQUEST_RING_DEPTH`, `XRCE_MAX_PENDING_REPLIES` — these four are
  **capacities INSIDE a slot**, which is what separates them from the three slot
  **counts** issue 1033 ruled zero-legal in the same header. An image that wants
  none of an entity sets its COUNT to 0 and the whole slot array goes away —
  that is the 33,296-bytes-a-subscriber saving. Setting a capacity to 0 instead
  keeps every slot and makes each one unable to do its job: at
  `XRCE_SUBSCRIBER_RING_DEPTH=0` and `XRCE_SERVICE_REQUEST_RING_DEPTH=0` the
  `count >= depth` ring-full arm is true for every arrival, so the callback
  drops the lot in silence — issue 1015 exactly; at `XRCE_MAX_PENDING_REPLIES=0`
  `take_request` can never allocate a token and returns `WOULD_BLOCK` forever;
  at `XRCE_BUFFER_SIZE=0` `xrce_stage_inbound`'s `len + 4 <= cap` fails for every
  payload. Two already had a floor above zero in a producer
  (`nros-rmw-xrce-cffi/build.rs` refuses `NROS_XRCE_BUFFER_SIZE < 64`; Kconfig
  says `range 1 1024` for the ring depth), so the guards agree with them.
* `Z_TASK_STACK_SIZE` — the actual stack of every zenoh-pico ThreadX task.
  `tx_thread_create` refuses anything under `TX_MINIMUM_STACK`; a build that
  wants no zenoh tasks sets `Z_FEATURE_MULTI_THREAD=0`, which removes the struct.
* `NROS_ZEPHYR_MAX_THREADS` — the platform's ENTIRE task pool. At 0 the image can
  create no task at all, zenoh-pico's read task included: issue 0839's failure
  with the wall at zero. (The producer cannot reach 0 anyway —
  `zephyr/CMakeLists.txt` emits the define under `if(CONFIG_…)`, false at 0 — so
  this is the backstop for a bare `-D`.)
* `NROS_THREADX_MAX_TIMERS` — the registry is the only way a `ULONG`
  `expiration_input` finds its wrapper, so at 0 `registry_claim` never succeeds
  and the whole timer ABI returns NULL forever, to save 32 pointers.
* `NROS_COMPONENT_MAX_TIMERS` — `nros::Timer` is `void* + size_t + bool` (plus
  one `unique_ptr` under `NROS_CPP_STD`), so 1 slot versus 0 is 12–32 bytes per
  component, against `Timer timers_[0]`, which ISO C++ does not have at all.
* `STRESS_SIZE` — guarded at **16**, not 1: `build_payload()` writes indices
  0..11 with no bounds test, so anything under 12 is an out-of-bounds write on a
  static object. 16 is the floor the CMake cache entry
  (`payload bytes (>=16)`) has always documented.

## Both gates in this family had a reach narrower than their rule (0196's shape)

Ruling nine knobs hit both at once, and both failed CLOSED, which is the good
direction and still wrong.

### `check-c-array-guard-probe` could only reach `zpico.c`

The build-tier twin exists because a guard that EXISTS is not a guard that FIRES
(issue 1167). It carried ONE hardcoded include set — zenoh-pico's — because every
guarded array had lived in `zpico.c`. The first guard outside that file made it
report `9 problem(s)`, every one of them "does NOT fire at `-D<KNOB>=0`", none of
them about a guard: the probe simply cannot compile a ThreadX, Zephyr or XRCE
translation unit with zenoh-pico's flags, and the four other files need headers
no host lane has at all (`tx_api.h`, `zephyr/kernel.h`, the cmake-generated
`uxr/client/config.h` and `nros_cpp_config_generated.h`).

It now carries a per-file `PROBE_CONTEXT` with two declared modes:

* **`compile`** — the real TU, real headers, `-fsyntax-only`. Still `zpico.c`.
* **`preprocess`** — `#include` lines stripped, and every enclosing condition the
  guard sits under supplied BY NAME (`-DZ_FEATURE_MULTI_THREAD=1`,
  `-DCONFIG_PTHREAD=1`), with the reason the headers are absent recorded beside
  it. Weaker in exactly one dimension — it cannot notice an enclosing condition
  that is false in every real build — so that condition is written where a
  reviewer can disagree with it, rather than assumed.

A guarded file in NEITHER mode is a hard failure, so the gate cannot grow a guard
it silently does not probe. It reports **18 guards across 7 files** now, against
9 across 1.

It also matched the literal `"must be >= 1"`, so a guard whose floor is higher
read as absent; it matches `must be >= \d+`.

## What the source gate could not see

`check-c-array-pool-floors` required the `#ifndef` and its `#define` to be
ADJACENT LINES. Two knobs document themselves between the two, so the gate had
never counted them — it reported `21 knob-sized arrays` over a tree that has 23,
and both invisible knobs live in files whose SIBLING knobs it already rules on:

* `XRCE_SUBSCRIBER_RING_DEPTH` — unruled, now guarded (above).
* `ZPICO_GRAPH_CACHE_SIZE` — **already carried a correct guard**
  (`zpico.c:388`), written with the other zpico guards and given no credit for
  five weeks, because the knob was not in the gate's set at all.

The scan now walks comments and blank lines (`next_code_line`), with a control in
the selftest. The `GUARD` pattern also accepts `#if K < N` for any positive
integer literal rather than only `< 1`, so a knob whose smallest legal value is
above one (`STRESS_SIZE`) can state it; `< 0` is still not a guard.

## Verified how

* `python3 scripts/check-c-array-pool-floors.py` — green, selftest on the normal
  path, `23 knob-sized C arrays: 18 guarded, 3 zero-legal, 2 unclassified`.
* `python3 scripts/check-c-array-guard-probe.py` — green,
  `18 guard(s) across 7 file(s) fire at 0 and are silent at defaults`. This is
  the proof that each new guard FIRES rather than merely existing (issue 1167's
  lesson), and it now covers every guarded file rather than one.
* `just ci gate` (compile + unit, NO fixtures) — the tier this change earns.
* Mutations, each red naming the exact site, then restored green:
  * source gate — a deleted guard; a guard moved above its `#ifndef` (the 1167
    shape); the scan reverted to adjacency; the `GUARD` pattern reverted to
    `< 1`; a guard disarmed to `< 0`.
  * probe gate — a guarded file with no `PROBE_CONTEXT` entry; `GUARD_TEXT`
    reverted to the literal `must be >= 1`; the declared enclosing condition
    `-DZ_FEATURE_MULTI_THREAD=1` dropped (which must, and does, make
    `Z_TASK_STACK_SIZE` read as unreachable — so the declaration is
    load-bearing, not decoration).

### Second pass (the uORB ruling)

* `python3 scripts/check-c-array-pool-floors.py` — green,
  `23 knob-sized C arrays: 19 guarded, 3 zero-legal, 1 unclassified`.
* `check-c-array-guard-probe` SKIPS on a bare agent worktree (it needs the
  zenoh-pico submodule, which is not checked out there), so its
  `compile_probe` was driven directly against the new `PROBE_CONTEXT` entry:
  **silent at defaults, and at `-DNROS_RMW_UORB_PX4_MAX_CALLBACKS=0` it emits
  the `#error` naming the knob** — the two halves that gate asks for. Its
  `--self-test` is green.
* Mutations, each red naming the site, then restored green:
  * the guard disarmed to `< 0` — the source gate reports the knob UNCLASSIFIED
    again and prints the exact `#if … < 1` to write.
  * the declared enclosing define dropped — guard still fires, which is the
    finding that made the `enclosing` note say `none` rather than claim a
    condition the probe does not actually depend on.
* NOT run: any Zephyr, PX4 or fixture build (memory-constrained host). Nothing
  in this pass needed one — the ruling rests on the reduction's compile result
  and on in-tree source.
