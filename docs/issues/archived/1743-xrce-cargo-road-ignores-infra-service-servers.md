---
id: 1743
title: "On the cargo road an XRCE image's service-server cap ignores the parameter and lifecycle servers, so a param + lifecycle image cannot boot"
status: resolved
type: bug
area: rmw, xrce, sizing, cli
severity: medium
found: 2026-10-07
related: [issue-1722, issue-1270, issue-1033, phase-480]
---

## What happens (measured, 2026-10-07)

`examples/workspaces/features`' param_talker built for XRCE
(`[image.native_rust_params_xrce]`, row `workspace-features-rust-params-xrce`,
native, cargo driver). The bringup enables `[param_services]` and `[lifecycle]`
(the build's own `[env]` says `NROS_DECLARED_INFRA_QUERYABLES =
"param+lifecycle"`), so the runtime creates 6 + 5 = 11 service servers.
Against the pinned Agent:

    [INFO]  nros: session open (rmw=xrce)
    [ERROR] nros_rmw_xrce: no free service server slot for
            '/param_talker/get_transition_graph': this image was BUILT with
            NROS_XRCE_MAX_SERVICE_SERVERS=4. ...
    nros: application error: Capability { name: "lifecycle", reason: "Transport::InvalidConfig" }

The image exits at boot. The diagnostic is loud and right (issue 1033), but
the number it names was never derived.

## Why (read from the source)

- `nros-cli-core::entity_inventory` derives the demand correctly: it counts
  the infra servers into its queryable total (issue 1270).
- The cargo road's `nros-cargo.toml` `[env]` (`leaf_entity_env.rs`,
  `DERIVED_ENV_KEYS`) emits `ZPICO_MAX_PUBLISHERS` / `ZPICO_MAX_SUBSCRIBERS`
  and the executor tables. It emits no `NROS_XRCE_MAX_*`, even for an image
  whose `rmw = "xrce"`, and passes the infra servers only as the FACT
  `NROS_DECLARED_INFRA_QUERYABLES`.
- That fact has exactly one consumer, zenoh's `nros-zpico-build/src/runner.rs`.
  `nros-rmw-xrce-cffi/build.rs` never reads it.

So `XRCE_MAX_SERVICE_SERVERS` falls to `internal.h`'s default of 4, and
`XRCE_MAX_SUBSCRIBERS` / `XRCE_MAX_SERVICE_CLIENTS` to theirs.

Workaround in tree: the issue-1722 row states
`NROS_XRCE_MAX_SERVICE_SERVERS = "11"`, with a comment pointing here. Delete it
with the fix.

## What a fix needs

- The XRCE build completes the count from `NROS_DECLARED_INFRA_QUERYABLES` +
  `NROS_DECLARED_NODES`, as zenoh's does, or the cargo road emits the three
  `NROS_XRCE_MAX_*` counts for an XRCE image. Zero stays a legal answer
  (issue 1033), so this is unfloored.
- Check the Zephyr / CMake road gives the same answer for the same image.

## Not measured

The C/C++ native XRCE entries and Zephyr XRCE images with params or lifecycle.
Whether the subscriber and client caps are short in the same way on a real
image.

## Resolution

Fixed on `fix/1743-xrce-infra-service-cap` (commit "XRCE service-server slots
count the parameter and lifecycle servers").

**Cause, confirmed.** The image's two halves of the count already reached the
XRCE build: the application's servers as the sizing descriptor's `[image]
service_server_queryables`, the runtime's as `NROS_DECLARED_INFRA_QUERYABLES`
+ `NROS_DECLARED_NODES`. Only `nros-zpico-build` summed them.
`nros-rmw-xrce-cffi/build.rs` read neither fact, so
`NROS_XRCE_MAX_SERVICE_SERVERS` fell to `internal.h`'s 4.

**Fix.**

- The parameter/lifecycle server counts and their fact parsers moved to
  `nros_sizing_descriptor::infra`, so zenoh and XRCE price the same servers
  with one formula (issue 1025's rule).
- `nros-rmw-xrce-cffi/build.rs` derives `NROS_XRCE_MAX_SERVICE_SERVERS` =
  application + runtime servers, only beside a stated infra fact (issue 1655's
  pairing). It is unfloored (issue 1033), and a stated knob still wins.
- `leaf_entity_env::with_fact_infra` completes the cargo-leaf inventory with
  the families its own facts declare.
- The hand-stated `NROS_XRCE_MAX_SERVICE_SERVERS = "11"` on row
  `workspace-features-rust-params-xrce` is gone, so the live cell proves the
  derivation.

**Every road.** Cargo: the build script above. CMake (C/C++ native):
`nros_entity_facts_env` already carries both facts and the descriptor path on
the Corrosion command, so the same build-script rung applies. Zephyr west:
`_nros_resolve_derivable_knob(NROS_XRCE_MAX_SERVICE_SERVERS …
NROS_DERIVED_MAX_QUERYABLES)` already states the knob from the inventory,
which counts both families when the model declares them (issue 1270). XRCE
subscriber and client caps carry no infrastructure term (the families create
servers and one publisher, and XRCE has no publisher pool), so they are not
part of this class.

**Measured (2026-10-09, pinned Agent `2.4.3-nros1`, row rebuilt with no
stated knob).**

- With the fix: the image opens its session and spins.
  `xrce_set_parameters_naming_25_declared_lands` PASS (61.4 s, solo).
- Negative control, with the derivation call disabled and the row rebuilt:
  the image exits at boot with `no free service server slot for
  '/param_talker/get_transition_graph': this image was BUILT with
  NROS_XRCE_MAX_SERVICE_SERVERS=4` and `application error: Capability { name:
  "lifecycle", … }`. The same test FAILS (`left: 0, right: 25`).
- The build-script selftest (case 13, on the normal build path) fails with the
  runtime term removed. The leaf test fails with the fact read disabled.
  `check-infra-queryable-counts` fails with `infra.rs`'s lifecycle mirror set
  to 4.

Sweep: `git grep -n "NROS_DECLARED_INFRA_QUERYABLES\|XRCE_MAX_SERVICE_SERVERS\|infra_queryables\|InfraServices" -- packages cmake zephyr scripts`.

## Not measured (at resolution)

- A C/C++ native XRCE image with params or lifecycle. It takes the same build
  script, but no in-tree row combines them.
- A Zephyr XRCE image with params or lifecycle. Zephyr's derived queryable
  count covers the families only when a model declares them, which zenoh shares
  on the same road.
- Whether a standalone (model-less) Zephyr leaf can enable the families at all.
