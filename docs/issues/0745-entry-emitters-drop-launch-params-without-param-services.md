---
id: 745
title: "Launch params never reached a component ctor: emitters gated seeding on param_services and emitted it post-construction, the store needed pre-node init, and ComponentNode read a different store"
status: resolved
type: bug
area: codegen
related: [issue-0255, issue-0744]
---

# 0745 — the launch-param seeding chain was broken in four places

Found by the ASI reference consumer measuring its control loop: the bringup
declared `ctrl_period = 0.03` in the launch XML, the resolved model carried
it, and the node ran at the compiled default (0.15 s) anyway — 160 ms timer
ticks measured standalone against a 30 ms intent. `control_output` was
equally dead. Four independent defects, each sufficient:

1. **Emitters dropped params without `param_services`.** The
   `nros_cpp_declare_param` block only emitted inside
   `if plan.param_services` — a plan without the capability silently
   discarded every launch param. Seeding is initial VALUES; only the
   runtime get/set RPC is the services surface.
2. **Seeding emitted AFTER construction.** An rclcpp-shape ctor reads
   `declare_parameter` initials immediately (the 0255 remap rule, one row
   over) — post-configure seeding is too late by design.
3. **The executor store did not exist pre-node.** `Executor::params` was
   created only by `register_parameter_services`, which builds six service
   servers and FAILS before a node exists (and fails outright on RMWs
   without service-server support — cyclonedds today), so pre-construction
   seeds had nowhere to land (`declare` returned false, `-1` through the
   FFI).
4. **ComponentNode read a different store.** Its `declare_parameter`
   facade is backed by the node-owned `params_` ParameterServer; the
   executor-store seeds were invisible to it even when they existed. (The
   facade's own comment claimed launch-seeded values win — aspirational.)

## Fix (landed together; the chain only works whole)

- `emit_declare_params` (shared C/C++ emitter helper, sibling of
  `emit_declare_remaps`): rc-checked `nros_cpp_declare_param` per launch
  param, per node, BEFORE construction — unconditional.
  `register_parameter_services` stays gated on `param_services` and moves
  to a NON-FATAL post-construction call: on RMWs without service servers
  the get/set RPC is unavailable, but the seeded store stands.
- `Executor::ensure_parameter_store` — `declare_parameter{,_with_descriptor}`
  lazily create the STORE (`ParamState.services` is now
  `Option<Box<dyn ParamServiceProcessor>>`, `None` until registration);
  `register_parameter_services` PRESERVES an existing store instead of
  overwriting the seeds.
- `ComponentNode::declare_parameter` adopts the executor-store seed as its
  effective default (`adopt_launch_seed_`, gated on
  `NROS_SYSTEM_PARAM_SERVICES`; `bool` not adoptable yet — the shim has no
  bool getter).
- `nros_lower_system_features` was DEFINED but called from no path; the
  workspace configure now calls it after capability resolution, and the
  `param_services` arm lowers to a directory-wide
  `NROS_SYSTEM_PARAM_SERVICES` compile definition (component TUs never see
  the entry's system_config.h).

## Verified

ASI freertos-posix (`param_services` declared in the bringup): standalone
control timer 158.8 ms → **31.6 ms mean (min 31.4 / max 32.5, n=1265)**
against the seeded 30 ms period; string seed (`control_output`) adopted
through the same path. Emitter tests updated (seeding precedes
construction; the disabled-case guard still holds — empty-params plans emit
nothing).

## Left open (follow-up candidates)

- **Timer over-credit under load**: with real subscription traffic the same
  timer publishes at ~1.5× its period (bursts, min interval ≪ period; max
  interval == period). Standalone it is exact. Separate defect in the
  executor's timer crediting under load; measured at ~50 Hz for a 33 Hz
  intent on the ASI loop.
- ~~`nros_cpp_get_param_*` has no bool getter → bool launch params are not
  ctor-adoptable.~~ Done — `6fb8579dd` added `nros_cpp_get_param_bool` and the
  `is_same<T, bool>` arm of `adopt_launch_seed_` (which must stay ahead of the
  `is_integral` arm, since `bool` satisfies both).
- `register_parameter_services` fails on cyclonedds (no service servers) —
  pre-existing, now visible; the param get/set RPC is silently absent
  there.

## The C emitter kept the pre-0745 block — fixed 2026-08-21

The "moves to a NON-FATAL post-construction call" above was true of the C++
emitter only. Both C sites (`emit_c.rs`, the flat path and the tiered
`ti == 0` path) still emitted the phase-269 block verbatim:

```c
nros_cpp_ret_t ps_ret = nros_cpp_register_parameter_services(executor);
if (ps_ret != NROS_CPP_RET_OK) return (int32_t)ps_ret;
```

So on an RMW without service servers a **C** entry declaring `param_services`
aborted boot with the registration rc, where the C++ entry next to it degraded
to "no get/set RPC, seeds intact" — the exact asymmetry the third follow-up
above predicts, one language over. The block also re-declared every launch
param AFTER construction, which is defect 2 of this issue surviving in the arm
nobody re-read; it was invisible only because `emit_declare_params` had already
seeded the same values pre-construction, so the late duplicate re-wrote what
was already correct.

Visible without a build, in the untracked generated corpus: every
`build-workspace-fixtures/src/native_c_*_entry/*_generated.c` carried `ps_ret`,
every `native_cpp_*`/`native_mixed_*` `.cpp` carried `(void)`.

Fixed by giving both C sites the C++ shape (non-fatal `(void)` call, no
duplicate seeding). Pinned by
`typed_emit_param_services_registration_is_non_fatal_both_paths`, which asserts
non-fatality, seed-before-registration, and exactly ONE seed per param, across
BOTH the flat and tiered emitters — the tiered site is a separate copy of the
block, which is why the first sweep missed it.
