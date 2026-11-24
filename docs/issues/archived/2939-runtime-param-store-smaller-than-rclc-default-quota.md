---
id: 2939
title: "rclc's default parameter server is refused by a store sized from the image's declarations, and a test still expects one callback slot where the runtime reserves two"
status: resolved
type: bug
area: [c-api, params, testing]
severity: high
found: 2026-11-24
resolved: 2026-11-24
resolved_in: "nros-c: `rclc_parameter_server_init_default` caps rclc's default quota at the store"
related: [2768, 2829]
---

## What happened

The nros12 release `platform-matrix` run 45351389552 (tier 2, `main` at
0d9fa3e470) failed three tests:

1. `cpp_c_param_live_read_e2e::c_param_live_read_publishes_baked_initial`:
   "C component never published live-read baked param (250)".
2. `rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`: the
   `workspace-features-c-params` and `workspace-features-mixed-params` entries
   stopped before registering, with
   "node /param_talker: parameter server not started: it asks for
   max_params = 4, and the image's parameter store holds 2 (all nodes)"
   (3 for the mixed image), then "component setup failed with code -6".
3. `image_config_workspace::each_workspace_image_builds_against_its_own_configuration`:
   "`plain` ... a configured sibling's value leaked", `max_handles=2`
   against an expected 1.

All three looked like one value leaking between images. They are two causes,
and neither is the phase-581 W6 renames.

## Cause 1: the default server asks for more than the store holds (1 and 2)

The C talker (`examples/workspaces/features/src/c_param_talker_pkg`, and the
C half of the mixed image) starts its server with
`rclc_parameter_server_init_default`, whose options are rclc's: a quota of 4
parameters. Since issue 2768 the build sizes the parameter store from what
the image declares: each node's own parameters, its launch seeds and the
seeded `use_sim_time`. The C image declares 2, the mixed image 3. The init
check refuses any `max_params` over the store's capacity, so a server whose
parameters all fit was refused because of a quota the caller never stated.

These images could not build at all between issue 2768 (2026-11-20) and
issue 2829's fix (2026-11-22), so the runtime check had not run since the
store began being sized from declarations.

**Fix:** `rclc_parameter_server_init_default` takes rclc's quota capped at the
store's capacity. A quota the caller states through
`rclc_parameter_server_init_with_option` is still refused when it is over the
store, as issue 2768 designed.

**Test:** `packages/api/nros-c/tests/run/rclc_parameter_default_quota.c`, in
`just check c`, against an nros-c whose store holds 2: the stated quota of 4
is refused, the default server starts with a quota of 2, and the node's
parameter fits. On the unfixed code it fails with the same refusal the
release run printed.

## Cause 2: the test expected one callback slot, the runtime reserves two (3)

There was no leak: the configured sibling `warn` states `max_cbs = 13`.
`plain` declares one timer, and since phase-556 W11 the runtime reserves one
more callback slot for its `/clock` subscription in every image whose nodes
carry `use_sim_time` (the sizing descriptor's `infra_families` names
`clock`). `plain` derives 2 correctly. The test's constant still said 1; the
test had been out of the tier-2 lane's runs until now, so nothing compared
them.

**Fix:** `PLAIN_DERIVED_MAX_CBS` is 2, with the reason.

## Verified

- `just check c`: `rclc_parameter_default_quota: OK`; the negative control
  (unfixed `parameter.rs`) fails 5 checks with the release run's message.
- `nros build` of `demo_bringup:linux_c_params` and
  `demo_bringup:linux_mixed_params`, run against a router: the C image
  publishes 250 and the mixed image 750 (the values tests 1 and 2 assert).
- `image_config_ws` `plain` prints `max_handles=2`.

## Not fixed here

The mixed image in the release run printed the store-size advice with the
pre-W6 knob spelling (`NROS_MAX_PARAMETERS`), which no source on `main`
contains, while the C image printed the current one. That part of the mixed
binary was built from older sources than the run's commit: a fixture
staleness question for the tier-2 runner's persistent fixture tree, not a
runtime defect.
