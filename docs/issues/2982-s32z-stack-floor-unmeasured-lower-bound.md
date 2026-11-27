---
id: 2982
title: "A refused stack-floor walk warns of a certain overflow along a path that crosses type-bound indirect calls (194 KB on an S32Z image with a 32 KiB stack)"
status: open
type: bug
severity: medium
area: [tooling, stack-floor, zephyr]
related: [issue-2979, issue-2818, issue-2550]
found: 2026-11-26
---

Reported by the ASI migration session, on `controller_bringup@s32z270-rtu0-zephyr`.

## What happens

The post-link stack-floor walk REFUSES, and because the image is
UNMEASURED (no row in the stack-floor table) the refusal is a warning and
the build passes:

```
OPEN: stated stack 32768 B; the walk reached at least 194260 B from `main`
before it refused (every missing frame counted as 0).
OPEN: that is already MORE than the stack: this image WILL overflow it.
```

The deepest frames on that path are an rclcpp subscription lambda
(`register_subscription_capturing<Trajectory>`, 22096 B),
`MpcLateralController::run` (17432 B) and
`nros_node::executor::arena::action_client_raw_try_process` (16488 B).

It is not new. The same warning, at 194620 B, was already in the passing
serial build of nano-ros `1a638e9958`, before issue 2979's iterative walk,
and the refused block is identical at `ebbb3a570d`.

## Why the walk refuses

Five functions on the walk have no frame in any input:

- `__aeabi_uidiv`, `__aeabi_uidivmod`: libgcc from the Zephyr SDK 0.16.9
  `arm-zephyr-eabi` (thumb/v7+fp/hard) toolchain, which has no row in
  `config/stack-builtins.toml` (the walker suggests
  `just zephyr stack-builtins`);
- `arch_cpu_idle`, `z_arm_cortex_r_svc`, `z_arm_int_exit`: Zephyr
  architecture code with no stack-usage record.

## Is 194 KB a real path? No

Settled by re-running the walk on a fresh build with the five frameless
functions accepted as 0-byte leaves (`--external`): it completes, at
`floor 215072 B (path 195520 B + 10%)`, and the walker itself labels the
path an UPPER bound, through an indirect call bound by type rather than by
a vtable slot. The path is 1008 frames. Checked against the linked image's
disassembly, about 310 steps are indirect calls and about 280 could not be
classified.

The three large frames are on one chain only through steps that are not
real calls:

- the subscription lambda (22096 B) is reached from a publisher's
  `publish_with_buffer`, but a publish never calls a subscriber's lambda;
- `MpcLateralController::run` (17432 B) is reached through a work-queue
  dispatch and a service-server trampoline; it actually runs from the
  controller's timer;
- `action_client_raw_try_process` (16488 B) is reached through an
  interrupt entry and a socket thread, in an image whose shape states no
  action client (`NROS_EXECUTOR_ACTION_CLIENTS=0`), so it may not run at
  all.

In the real image each of these runs directly under the executor, one at a
time. The real depth is roughly the largest of them plus the init and
executor frames, not their sum. The image's 32 KiB main stack may still be
tight for the 22 KiB lambda; that is for the image to measure.

## The defect: the warning calls an upper bound a lower bound

The warning an UNMEASURED image gets when its walk refuses says the walk
"reached at least N B ... (every missing frame counted as 0)" and, when
N exceeds the stack, that the image "WILL overflow it". Counting missing
frames as 0 makes N a lower bound only along the path it reports. The path
itself crosses indirect calls bound by type, which a completed walk reports
as an upper bound. Here the "certain overflow" is a path that cannot run.
So the warning must not claim a lower bound or a certain overflow when its
path crosses a bound indirect edge (`upper_bound_edge`, which a completed
walk already asks), and should say "an upper-bound path" instead.

A second gap: a refused walk records only its three largest frames.
`report.json` keeps no path for a refused role, so reading it took a
re-run with `--external`. The refused role's path belongs in the report.

The set of frameless functions also moved between two builds at the same
pin. One rebuild's refusal also named Eigen kernels (`gemv_dense_selector`,
`general_matrix_matrix_product`) and `__gnu_cxx::__to_xstring`, which the
next did not.

## A real cost found on the way

`action_client_raw_try_process`'s 16488 B frame is real: it holds two
stack buffers, the spliced feedback (`FEEDBACK_BUF`) and the spliced result
(`RESULT_BUF`), 8192 B each here, in disjoint scopes that the compiler does
not overlap because each pointer is passed to a C callback. Every image
with an action client pays `FEEDBACK_BUF + RESULT_BUF` of stack during
dispatch. It dates from the Cyclone encapsulation splice and is not a
regression. One shared buffer, or splicing in place in the core's own
buffers, would halve or remove it.

## How to pick it up

1. Give the five functions frames: a builtins row for this libgcc (`just
   zephyr stack-builtins`), and `[[policy]]` rows (or measured frames) for
   the three arch functions. The walk then completes.
2. Fix the warning (above): no "at least" and no "WILL overflow" for a
   path that crosses a bound indirect edge; and keep a refused role's path
   in `report.json`.
3. Separately, share one splice buffer in `action_client_raw_try_process`.

## Progress (2026-11-27): the S32Z refusal is still step 1

NXP's ASI run (main 0e4afce2fa) reports the S32Z Zephyr main walk still
refusing: UNMEASURED, at least 193,996 B, so its 32 KB main stack cannot
be settled from the walk. That is this issue's step 1, unchanged: the
Cortex-R image calls frameless code directly, the toolchain's
thumb/v7+fp/hard libgcc (`__aeabi_uidiv*`, no row in
`config/stack-builtins.toml`) and Zephyr's arm assembly (`arch_cpu_idle`,
`z_arm_cortex_r_svc`, `z_arm_int_exit`).

The aarch64 half of the same class closed with issue 2897: frameless
assembled code is decoded off the linked objects (`apply_assembled_objects`,
`stack_builtins.ASSEMBLED_ARCHES = ("aarch64",)`). Step 1 here is that
step's arm extension (`ASSEMBLED_ARCHES` gains `arm`, re-measuring the
Cortex-M images it also moves) plus the measured Cortex-R libgcc row. It
lands in this issue's own change, with step 2 (the warning's wording).

**Held until nros12 is published (2026-11-27, 243, release owner).**
Both steps are post-release: the user ruled that the S32Z stack-walk gap
is not a release blocker. Work may be written as an unarmed draft, and
nothing is armed until the publish is announced.

The other edge the same run reports, `Logger::dispatch` bound to an
escaped callback "by type", is not this issue's: it is the `dyn`
over-binding phase-592 W2 removes (RFC-0143 D1).
