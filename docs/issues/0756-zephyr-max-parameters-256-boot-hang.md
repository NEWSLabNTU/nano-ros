---
id: 756
title: "`NROS_MAX_PARAMETERS=256` hangs Zephyr boot right after
  `dds_create_participant` (bisected; 32 boots clean)"
status: resolved
type: bug
area: zephyr, memory
related: [issue-0749]
---

## Symptom

On the Zephyr FVP lane (autoware-safety-island controller image,
cyclonedds RMW), building with `NROS_MAX_PARAMETERS=256` — the value the
FreeRTOS lanes run, and the natural setting for a controller declaring
150+ parameters — produces an image that boots to
`dds_create_participant` and then hangs: no fault, no panic, no further
output. Bisected on the knob alone (2026-08-22, consumer side):
`NROS_MAX_PARAMETERS=32` with everything else identical boots and runs
the full autonomous-driving loop.

The knob only ACTS on the Zephyr Rust lane since `d1c5b3b3b` (issue 0749
made the sizing knobs reach cargo at all), so this is the first time any
Zephyr image actually built a 256-slot param store — the hang was
unreachable before.

## Consumer state

autoware-safety-island `build.sh` pins the Zephyr lane back to the old
effective value (`NROS_MAX_PARAMETERS=32`; params past the 32nd fall back
to compiled defaults — the behaviour every Zephyr image has always had)
and documents this issue as the unpin condition. FreeRTOS lanes run 256
without trouble.

## Suspicion

A large-parameter-store stack temporary: the param store scales with
`MAX_PARAMETERS` and something on the boot path (store init, or the
first param-service registration inside participant/node bring-up)
plausibly constructs it — or an array of it — on a Zephyr thread stack
sized for the 32-slot layout. A hang rather than a MPU fault is
consistent with a clobbered adjacent stack. Not yet confirmed upstream;
the bisect evidence is knob-level, not frame-level.

## Direction

1. Reproduce on a stock Zephyr cell (native_sim or FVP) with
   `NROS_MAX_PARAMETERS=256` — no consumer code needed.
2. Find the frame that scales with the knob (param store init path);
   move it to static/arena storage or size the owning stack from the
   knob.
3. Whatever the fix, boot should FAIL LOUD when a sizing knob makes a
   stack unviable — a silent hang after `dds_create_participant` took a
   consumer-side bisect to attribute.


---

## Root cause (measured 2026-08-22)

Confirmed at frame level; the suspicion in "Direction" was right about the
mechanism and the frame is `Box::new`'s.

`ParameterValue` is an enum sized by its largest variant,
`StringArray(Vec<String<MAX_STRING_VALUE_LEN>, MAX_ARRAY_LEN>)` — 32 x 264 B.
So **every parameter slot costs ~8.5 KiB regardless of what it holds**, and the
store scales from there (`size_of`, host):

| | bytes |
| --- | ---: |
| `ParameterValue` | 8,464 |
| `Parameter` | 8,536 |
| `ParameterServer` @ 32 | 285,192 |
| `ParameterServer` @ 256 | 2,281,480 |

`Executor::ensure_parameter_store` and the `register_parameter_services`
fallback both did `Box::new(ParamState { server: ParameterServer::new(), .. })`.
Rust has no placement-new: `Box::new(expr)` materialises `expr` on the CALLER'S
STACK and then copies it into the allocation. Measured on
`thumbv7em-none-eabihf`, opt-level 2, by the prologue's `sub sp`:

| `MAX_PARAMETERS` | `Box::new` | in-place |
| ---: | ---: | ---: |
| 32 | 280,596 B | 68 B |
| 256 | 2,244,628 B | 68 B |

The cyclonedds snippet sets `CONFIG_MAIN_STACK_SIZE=524288`. 280,596 fits it
with 46% to spare; 2,244,628 overruns it 4.3x. That is the whole bisect: 32
boots, 256 does not, and the overrun is a silent clobber rather than an MPU
fault because it walks off the end of a stack that has no guard below it.

FreeRTOS lanes were unaffected because they run much larger task stacks.

## Fix

`ParameterServer::init_in_place(dst: *mut Self)` writes each slot's `None`
through the destination pointer, so no `ParameterServer` is ever materialised by
value. `Executor::new_param_state()` allocates with `Box::new_uninit()` and
initialises both fields through the allocation; both former call sites route
through it. Largest remaining temporary is one `Option<ParameterEntry>`.

The knob no longer decides whether boot survives.

## Residual — the knob is still not free

This fixes the stack. It does not shrink the store: 256 slots is still a
**2.2 MB heap allocation**, which most embedded consumers cannot satisfy. The
failure mode moves from silent stack corruption to a failed allocation, which is
better but only as loud as the consumer's `handle_alloc_error`.

The durable fix is the 8.5 KiB-per-slot enum itself — a `StringArray` that every
`bool` parameter pays for. Boxing that variant (or giving it a separate pool)
would cut the store by ~30x and make 256 slots cost ~70 KiB. That is a
representation change to a public type and is left open deliberately.
