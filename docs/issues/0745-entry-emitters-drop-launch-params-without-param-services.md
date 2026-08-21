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
- `nros_cpp_get_param_*` has no bool getter → bool launch params are not
  ctor-adoptable.
- `register_parameter_services` fails on cyclonedds (no service servers) —
  pre-existing, now visible; the param get/set RPC is silently absent
  there.
