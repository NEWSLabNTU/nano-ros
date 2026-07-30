# Phase 323 — one capability derivation, for every language

**Status (2026-07-31): Draft.** Implements the premise phase-314 and phase-315
established and did not finish: `system.toml` is the single source of truth for
the RMW, the ROS edition **and the capability list**, in every language.
**Closes:** issue 0351. **Informed by:** issues 0311, 0118 (archived, resolved),
phases 314 / 315.

## The state today

phase-315 gave Rust a real derivation: `nros sync` generates a facade crate
carrying the features derived from `system.toml`, and the entry names none of
them. C/C++ never got the equivalent, and the gap is invisible because three
different mechanisms hide it.

All three capability axes take the SAME path:

```
system.toml            NANO_ROS_FEATURES        _caps            cargo feature
[system].features  ->  (cmake cache var)   ->   in nros-c/  ->   param-services
[param_services]                                nros-cpp         lifecycle-services
[lifecycle]                                     CMakeLists       safety-e2e
[safety]
```

and on the workspace path **`NANO_ROS_FEATURES` is never set**, so none of them
arrives. Measured, not inferred — `ws-params-cpp` declares `[param_services]`,
the bake writes `set(NANO_ROS_FEATURES "param_services")` into
`<bake>/system_config.cmake`, nothing includes that file, and the workspace
cache reads:

```
NANO_ROS_FEATURES:STRING=
```

The three axes differ only in how the breakage is MASKED, which is why it reads
as three unrelated quirks rather than one bug:

| axis | mask | kind |
| --- | --- | --- |
| `param_services` | the `posix` always-on in nros-c / nros-cpp / the umbrella | implicit |
| `lifecycle` | same | implicit |
| `safety` | `cmake_defs = { NANO_ROS_SAFETY_E2E = "ON" }`, per fixture | explicit |

The safety workaround cites issue #118 for the missing wiring. **0118 is
`status: resolved`** (phase-269) and was about the executor-component integrity
READBACK API, not cmake lowering — so a temporary `-D` is now defended by a
closed ticket.

### Why the `posix` always-on cannot simply be deleted

It is not a convenience. On hosted it is the ONLY route those two axes take, so
removing it first breaks the workspaces that DO declare:

```
ParamTalker.cpp:(.text+0x44): undefined reference to `nros_cpp_get_param_integer'
```

That is `ws-params-cpp` — a workspace whose `system.toml` declares
`[param_services]`. The declaration path and the working path are disjoint.

## What it costs

* the same `system.toml` yields different capability sets per platform, which is
  the one thing phase-314 existed to end;
* hosted **cannot fail** when a declaration is missing, so "forgot to declare" is
  indistinguishable from "declared" on the platform most people develop on;
* the embedded side is stricter than the hosted side, i.e. the error surfaces
  furthest from where it was introduced.

## Design

**One derivation, two emitters, both generated.** The derivation already exists
and is already the SSoT — `SystemToml::capability_enabled()`, which honours both
the generic `[system].features` list and the deprecated typed blocks. Every
consumer must reach the feature list through it.

```
                    system.toml
                         |
          SystemToml::capability_enabled()      <- the ONE derivation (exists)
                    /            \
       Rust facade crate      NANO_ROS_FEATURES  <- two emitters
    (phase-315 W1, exists)    (bake writes it,
                               nobody reads it)  <- THE GAP
                    \            /
              nros_feature_set / cargo
              (phase-314, one computation)
```

Nothing new is invented: both emitters exist, `nros_feature_set` is already the
single feature computation, and `capability_enabled` is already the single
accessor. The only missing edge is that the cmake emitter is never consumed.

### The one open question — how `NANO_ROS_FEATURES` gets set

The bake already runs BEFORE cmake configure (`workspace-fixtures-build.sh:176`
vs `:221`), so `system_config.cmake` exists in time. What is undecided is who
reads it, and it must work for a hand-written workspace, not only for the
fixture script.

* **(A) `-D` from the derivation.** Whoever configures the workspace passes
  `-DNANO_ROS_FEATURES=…`, obtained from the CLI. Smallest change, matches how
  `safety` is already forced — except derived instead of hand-written. Weakness:
  a user running `cmake` by hand gets nothing, which is the same class of
  silent-miss this phase is closing.
* **(B) the entry's cmake includes the bake.** `nano_ros_add_executable` (or a
  small `nros_use_system()` before `find_package`) includes
  `<bake>/system_config.cmake`. Works for hand-written workspaces. Weakness:
  ordering — `NANO_ROS_FEATURES` must be set before nano-ros is pulled in, so it
  cannot be inside `nano_ros_add_executable`, which runs after.
* **(C) `find_package(nano_ros)` resolves the bringup itself.** Zero user
  ceremony. Weakness: cmake has to find the bringup, which means teaching it the
  workspace layout.

(B) is the current preference — it is the only option that is both explicit and
correct for a hand-written workspace, and standalone examples already do exactly
this by hand (`set(NANO_ROS_FEATURES "safety")` before pulling nano-ros in), so
it makes the workspace path match a shape that already works. **Decide before
W1.**

## Work items

### W1 — populate `NANO_ROS_FEATURES` on the workspace path

Per the decision above. **Done when:** `ws-params-cpp` compiles `nros-cpp` with
`param-services`, traceable to its declaration, with no `cmake_defs` and no
always-on.

### W2 — delete the `posix` always-on

The three sites in `nros-c/CMakeLists.txt`, `nros-cpp/CMakeLists.txt` and
`cmake/NanoRosRuntimeCrate.cmake`. Only after W1; verified by W4's gate rather
than by inspection.

**Done when:** a posix workspace that declares nothing compiles WITHOUT
`param-services` / `lifecycle-services`, and one that declares gets them.

### W3 — delete the safety `cmake_defs` masks

Remove `cmake_defs = { NANO_ROS_SAFETY_E2E = "ON" }` from the safety fixture rows
in `examples/fixtures.toml`, and the stale #118 citation with them. If the mask
survives W1, the bug survives with it — that is precisely how it outlived the
issue it cited.

**Done when:** the safety fixtures build and their CRC e2e tests pass with the
knob coming only from `[system].features = ["safety"]`.

### W4 — gate it

Extend `scripts/check-feature-set-ssot.sh`: no fixture row may force a
capability's cmake token, and no platform branch may append a capability. Both
are the "second source" shape that rule 1 and rule 2 already forbid for the
edition and the platform mapping.

**Done when:** re-adding either mask fails `just check`.

## Non-goals

* **Giving `param_services` / `lifecycle` cmake tokens.** Considered and
  rejected: a `cmake_token` exists to flip a cmake `option()`, which `safety`
  needs because it gates C/C++ CODE. These two need only a cargo feature, which
  `nros_feature_set` already maps. Tokens change nothing while
  `NANO_ROS_FEATURES` is empty and are redundant once it is not.
* **Changing what any capability means**, or the Rust facade, which already
  derives correctly.

## Acceptance

- [ ] A declared capability reaches the C/C++ cargo feature list on the
      workspace path, traceable to `system.toml`.
- [ ] An undeclared capability does NOT reach it, on posix as on embedded.
- [ ] No fixture row forces a capability knob; no platform branch appends one.
- [ ] `nros config show` and the built feature list agree for all three axes
      (the audit tool and the build stop disagreeing — cf. the phase-315 fix
      where they did).
- [ ] The gate rejects a reintroduced mask.
