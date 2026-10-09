---
id: 1766
title: "`nros::main!()` without `launch =` ignores the leaf's declared capability axes: `features = [\"param_services\"]` builds an image with no parameter services and no error"
status: resolved
type: bug
area: [api, macros, examples]
severity: low
found: 2026-10-09
resolved_in: "a Form-1 `nros::main!()` wires or refuses every axis its leaf declares (issue 1766)"
related: [1706, 0274, phase-314]
---

## What

A single-package Rust leaf states its capabilities in `system.toml`'s
`[system] features`, the same key a bringup uses. `nros::main!` reads that axis
only on its LAUNCH arm (`launch = "..."` / `model = "..."`): there it wires
`apply_param_services` and const-asserts that `nros` was built with the
`param-services` feature (phase-314). Form 1 -- `nros::main!()` or
`nros::main!(panic = ...)`, the shape every `examples/*/rust/<role>` leaf uses
-- never opens the model, so the axis is dropped in silence. No services, no
const-assert, no warning.

## Measured (2026-10-09)

`examples/mps2-an385-freertos/rust/talker` (`nros::main!(panic = "platform")`),
QEMU + `rmw_zenohd`:

| `[system] features` | `nros` cargo feature `param-services` | parameter store built | heap peak |
| --- | --- | --- | --- |
| `param_services` | no | no | 163,472 |
| `param_services` | yes | no | 163,472 |

163,472 is the parameter-less number issue 1706 recorded. The text grew by
53 KB with the feature, so the services were compiled and never registered.

## Why it matters beyond the missing services

Every derivation that reads the axis assumes the entry wires it: the
queryable pool counts six parameter servers per node for it (issue 1270), and
since issue 1706 the cargo-leaf sizing descriptor states `[params] store =
"param_services"` when the axis AND the cargo feature are both present, which
carves 280,832 B of `.bss` for the store. On a Form-1 leaf that store is never
used. (The axis alone, without the feature, carves nothing -- that was ruled in
issue 1706 from the first row above.)

## Direction

Form 1 should read the leaf's own resolved model (`nros sync` writes it under
`build/nros/models/`) for the capability axes, exactly as the launch arm does,
including the phase-314 const-assert. Then the axis means one thing on every
leaf, and the descriptor's reading of it is exact.

## Resolution (2026-10-10)

A self-bringup entry (Form 1 `nros::main!()` / `nros::main!(panic = ...)`, and
Form 2 `board = X`) now reads the leaf's `[system] features` and either wires
each declared axis or refuses the build naming it. Nothing is dropped.

It does not read the resolved model, as the direction above proposed: a leaf's
model is a launch-tree artifact, while the axis lives in `[system] features`,
which the macro already reads (through `leaf_system`) to find the board, and
which the sizing descriptor also reads on the leaf road (`InfraServices::from_features`).
Reading the same key keeps the entry and the descriptor in agreement.

### What each axis does now

The reading is `nros_orchestration_ir::leaf_capabilities` (new), shared by the
macro. Its axis table is pinned in both directions to the CLI's lowering
registry (`cargo_nano_ros::capability_resolver::CAPABILITIES`) by
`leaf_capability_axes_match_the_capability_registry` in `nros-cli-core`. That
test also checks that every `compiled_flag` is the `__macro_support` const for
exactly that feature.

| declared | Form 1 emits | refused when |
| --- | --- | --- |
| `param_services` | the phase-314 `PARAM_SERVICES_ENABLED` const-assert, plus `runtime.apply_param_services()` before the registers. The executor sizing counts the slots, as on the launch arm | `nros` lacks `param-services` (compile-time assert) |
| `lifecycle` | a `LIFECYCLE_SERVICES_ENABLED` const-assert, plus `runtime.apply_lifecycle(N)` after the registers. `N` comes from `[lifecycle] autostart` (`none`/`configure`/`active`), and is 0 when the table is absent | `nros` lacks `lifecycle-services`; `[lifecycle]` present without the axis in `features`; an unknown `autostart` word |
| `safety`, `rosout` | a `SAFETY_E2E_ENABLED` / `ROSOUT_ENABLED` const-assert. Both axes only pull code in (cargo-feature lowering), and no runtime call exists for either | `nros` lacks `safety-e2e` / `rosout` |
| anything else | nothing | always, naming the known axes |
| typed `[param_services]` / `[safety]` block in a leaf | nothing | always. It is the deprecated bringup spelling, and no leaf consumer reads it |

**RTIC and Embassy, on either arm.** These framework emits register neither
service family, so a declared `param_services` or `lifecycle` is now a
compile error naming the framework. The launch arm used to drop it in silence
too, so that branch was closed for both arms.

The new `__macro_support` consts are `LIFECYCLE_SERVICES_ENABLED`,
`SAFETY_E2E_ENABLED` and `ROSOUT_ENABLED`, beside `PARAM_SERVICES_ENABLED`.

### Measured: `examples/mps2-an385-freertos/rust/talker`

A copy of the leaf was built with `[system] features = ["param_services"]` and
`nros/param-services` (`nros sync` + `nros build`, `dev` profile). It ran under
QEMU mps2-an385 with `-icount shift=auto`, against a live `rmw_zenohd` on
`tcp/0.0.0.0:7800`, for 45 s per run. The descriptor states
`[params] store = "param_services"` in both builds (#1830's leaf rule: axis AND
feature). The "before" build is this branch with `wire_leaf_axes` forced to
`Ok(None)`, which is the pre-fix emit.

| build | text / data / bss | `EXECUTOR_BACKING` | console | heap peak | `ros2 service list --no-daemon` | delivered |
| --- | --- | --- | --- | --- | --- | --- |
| before | 542,208 / 4,520 / 1,222,508 | 297,352 B | no `parameter store` line | 163,472 of 720,896 | (empty) | 42 `Publishing:` |
| after | 542,256 / 4,520 / 1,222,508 | 297,352 B | `parameter store: 32 slots (280832 B) carved from the executor backing` | 192,432 of 720,896 | the six `/talker/*parameter*` services | 42 `Publishing:`; `ros2 topic echo --once /chatter` received `Hello World: 14` |

So the "before" image already reserved the carved store in `.bss`, because the
descriptor stated it, and then never built it. That was the cost this issue
predicted. The "after" image builds the store in that region, so the store does
not reach heap_4. The +28,960 B heap peak is the six services' endpoints, not
the store; the store alone would be 280,832 B. `ros2 param list --no-daemon
/talker` printed nothing in 20 s. It was not investigated: the talker declares
no parameter, and the service listing above is the evidence that the services
were registered.

### Tests (fail-before / pass-after, mutation-checked)

- `nros-macros` `main_macro::leaf_axis_tests` expands the macro over a
  throwaway leaf:
  - `form1_wires_a_declared_param_services_axis` covers Form 1 and Form 2;
  - `form1_wires_lifecycle_with_its_autostart_and_asserts_every_axis_feature`;
  - `an_axis_that_cannot_be_honoured_is_refused_by_name`, for a typo and for RTIC;
  - `a_leaf_with_no_features_wires_nothing`, the negative control.

  Forcing `wire_leaf_axes` to return `Ok(None)` (the pre-fix behaviour) turns
  three of them red and leaves the negative control green. Dropping only the
  `param_services_enabled |= caps.param_services` line turns two of them red.
  These tests set process environment variables, so the crate's one existing
  env-setting test (`an_unknown_framework_is_an_error_not_owned_spin`, which
  claimed a "single-threaded test process") now shares one `TEST_ENV_LOCK`
  with them.
- `nros-orchestration-ir` `leaf_capabilities::tests` (5): the reading itself.
- `nros-cli-core` `leaf_capability_axes_match_the_capability_registry`.
  Renaming `rosout`'s `nros_feature` in the leaf table turns it red.

### Not changed

The launch arm still emits no `LIFECYCLE_SERVICES_ENABLED` assert when the
model gives a node `lifecycle_autostart`. Generated workspace entries get the
feature from the capability registry. A hand-written `launch =` entry without
it would still register nothing. That is a sibling of this issue, and it was
left as is because this change did not survey which in-tree launch entries
would start refusing.
