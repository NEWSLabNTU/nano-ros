---
id: 1743
title: "On the cargo road an XRCE image's service-server cap ignores the parameter and lifecycle servers, so a param + lifecycle image cannot boot"
status: open
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
