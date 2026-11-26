---
id: 2982
title: "An S32Z Zephyr image's stack-floor walk refuses on five frameless functions and warns of a 194 KB lower bound against a 32 KiB stack"
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

## Is 194 KB a real path?

Open. The figure is the depth of ONE path, not a sum, and the frameless
functions count 0, so interrupt handlers and idle are not added into it.
But the three largest frames are executor callbacks, which run one after
another under the executor rather than nested, and one path holding all
three means the walk resolved an indirect call to more than the call site
can reach. The bind summary supports that reading: of the image's indirect
call sites, 119 have no typed target and 98 have no IR, so each of those 98
is bound to every function C code can hold, a callback dispatcher among
them. If so, 194 KB is an over-approximation and not a real overflow.
Settling it needs the OPEN role's chain from the build's
`nros/stack-floor/report.json` (requested).

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
2. Read the completed path. If it nests sibling callbacks through a bound
   indirect site, the binding is the defect; if it is real, raise
   `CONFIG_MAIN_STACK_SIZE` for the image.
3. Separately, share one splice buffer in `action_client_raw_try_process`.
