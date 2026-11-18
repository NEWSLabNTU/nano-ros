---
id: 2704
title: "A Zephyr image on `rmw = \"cyclonedds\"` with `param_services` boots without its parameter services: the Zephyr road generates no `rcl_interfaces` descriptors"
status: open
type: bug
area: [zephyr, rmw, params, cmake]
severity: high
found: 2026-11-18
related: [2304, 1268, 1271, 2705, phase-556]
---

## Measured

A downstream C++ image on Zephyr (FVP Base_RevC AEMv8-R, user-mode network),
`rmw = "cyclonedds"`, `[system] features = ["param_services"]`, one node
`/controller`, nano-ros at 9afb5c8a19. It boots, its own subscriptions and
publishers work, and then prints once:

```
<err> nros: parameter services could not be created for node /controller at
service get_parameters: Transport(Unsupported). `ros2 param` cannot reach 1 of
this image's 1 node(s). Not retrying: this failure cannot change without a
rebuild.
<err> nros: NodeError::Transport(Unsupported): transport error: operation not
supported by this backend
```

Boot continues. The image runs without any of the six parameter services, and
`ros2 param` against it times out. The build is green.

The image's link carried fifteen Cyclone typesupport archives
(`lib<pkg>__cyclonedds_ts.a` for `builtin_interfaces`, `std_msgs`,
`geometry_msgs`, the application's own packages, ...). None was
`rcl_interfaces`.

## Where the line comes from

- The line: `Executor::report_parameter_service_reconcile_failure`,
  `packages/core/nros-node/src/executor/spin.rs` (issues 1271, 1268; added in
  8604a25ac7, 2026-09-10).
- The error: `create_param_srv` -> `session.create_service` -> the Cyclone
  backend's `service_create`
  (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp`), which
  returns `NROS_RMW_RET_UNSUPPORTED` when `descriptors_for_service` finds no
  registered descriptor for the request/reply types
  (`rcl_interfaces::srv::dds_::GetParameters_Request_`/`_Response_`).
  `get_parameters` is simply the first of the six to be created.

## Can Cyclone on Zephyr serve a service at all?

Yes. `service_create` has no platform gate; the image's own services, and the
Zephyr C/C++ Cyclone service e2e (issue 0157), work. The backend's
`nros-rmw.toml` declares no "no services" capability, and it should not: the
capability is there. What is missing is the TYPE: Cyclone creates an entity
only for a type whose descriptor some linked translation unit registered
(`[rmw.capabilities] type-descriptors = "needs-type-descriptors"` says exactly
this).

## Cause

The runtime's own entity types reach a descriptor-registry backend on two
roads only:

1. The cargo road: `nros-node` registers each type's descriptor from its schema
   at create time (`rmw_type_registry::register_type`, issue 1268), through the
   registrar the Rust `nros-rmw-cyclonedds` crate installs. On the Zephyr C/C++
   road nros-node is built without `needs-type-descriptors` (nothing under
   `zephyr/` lowers it) and the Rust registrar crate is not linked, so this is
   a no-op there.
2. The top-level cmake road: `nros_runtime_interface_types()`
   (`cmake/NanoRosRuntimeInterfaceTypes.cmake`, issue 2304, b5fb685377,
   2026-11-10) runs the backend's per-message hook over `builtin_interfaces` +
   `rcl_interfaces` when `param_services` is declared, and over
   `lifecycle_msgs` for `lifecycle`. It is called from the top-level
   `CMakeLists.txt` only. `zephyr/CMakeLists.txt` resolves the same
   `NROS_FEATURES` (and turns on `param-services` in nros-cpp/nros-c) but never
   calls it.

So the Zephyr road is the one road with neither, and every Zephyr C/C++ image
on Cyclone that declares `param_services` (or `lifecycle`) builds without the
descriptors its runtime needs. Issue 2304 fixed the cmake road and named the
cargo road as fine; the Zephyr road was not in its scope.

Two adjacent gaps of the same shape:

- `rosout` (phase-556 W13 made it a capability axis, `CONFIG_NROS_CAPABILITY_ROSOUT`)
  publishes `rcl_interfaces/msg/Log`, but `nros_runtime_interface_types()`
  generates `rcl_interfaces` only for `param_services`. A cmake-road Cyclone
  image declaring `rosout` without `param_services` has the same hole.
- `infra_service_descriptors.rs` (issue 1268) checks the twelve types build a
  descriptor HOST-side, and row `c-parameters-cyclonedds` (issue 2304) covers
  the cmake road. No row runs parameter services on a Zephyr Cyclone image, so
  nothing caught this.

## When it started

Not a regression of a recent pin. Reasoned from history rather than a rebuild:

- The Zephyr road has never called `nros_runtime_interface_types` (`git log -S`
  on the name: only b5fb685377 and its docs commit, both cmake-road), and has
  never lowered `needs-type-descriptors`.
- The line itself exists since 8604a25ac7 (2026-09-10). Before it, the same
  failure was discarded silently on every spin (issue 1271).
- Every pin the downstream image used (nros11 release 09219edc75, ea94566e87,
  5871cd6e45, a49a8dacaf, 9afb5c8a19) contains both 8604a25ac7 and
  b5fb685377, so each of them builds this image without the descriptors and
  would print the same line. The downstream image never booted on FVP between
  a49a8dacaf (configure broke, fixed by #4190) and 9afb5c8a19, which is why it
  is first seen now.

## Why it matters more now

phase-556 W13 makes `param_services` and `rosout` the default for every image
(`node_interfaces::DEFAULT_NODE_INTERFACES` flips to `Stock`). Once it does,
every Zephyr C/C++ image on Cyclone gets this line, and a node that `ros2 param`
cannot reach, without having asked for anything.

## Direction

Design rule (manager, 2026-11-18): a node with parameter services ON whose
backend cannot serve them is refused at BUILD/configure time, or does not
request them; never a runtime line with services silently missing.

Proposed: **provide, and refuse at configure when providing is impossible.**
Not "do not request": the backend CAN serve them, so dropping the services
would make Zephyr Cyclone the one combination where W13's stock set quietly
does not exist.

1. `zephyr/CMakeLists.txt`: after `nros_resolve_image_features()` and the
   Kconfig-axes fallback, call `nros_runtime_interface_types("${NROS_FEATURES}"
   TARGETS <the Zephyr app / nros-cpp / nros-c interface targets>)`, with the
   Zephyr per-message hook (`_nros_zephyr_cyclonedds_typesupport`, the one
   `nros_generate_interfaces` already uses there). Its existing FATAL_ERROR
   when `rcl_interfaces` cannot be found is the configure-time refusal. The
   function may need a road argument (or the hook resolved by the caller) since
   `NROS_RMW_PER_MESSAGE_HOOK` names the cmake-road hook.
2. `cmake/NanoRosRuntimeInterfaceTypes.cmake`: generate
   `builtin_interfaces` + `rcl_interfaces` for `rosout` too, not only
   `param_services`.
3. A guard that the capability and its types agree, decided from facts known
   at configure: for a backend whose descriptor states
   `[rmw.capabilities] type-descriptors`, a road that lowers
   `param_services`/`lifecycle`/`rosout` into the runtime and provisions no
   descriptors for their packages is a configure error naming the axis and the
   package (the 2304 message, raised on every road). A structural check
   (`check-runtime-interface-types-roads`, or an extension of an existing
   road-parity check) that every road which lowers `NROS_FEATURES` into
   nros-cpp/nros-c also calls `nros_runtime_interface_types` would have caught
   this one at review.
4. Test: a Zephyr Cyclone fixture row with `param_services` (native_sim or
   qemu, cheapest Cyclone Zephyr target) whose verdict includes "no `parameter
   services could not be created` line" and a `get_parameters` round trip,
   alongside `c-parameters-cyclonedds`. A cheaper, link-level variant:
   assert the image's ELF holds the twelve `rcl_interfaces::srv::dds_::*`
   descriptor registrations when `param_services` is on.

Separately, the runtime line can stay as the backstop, but with a
configure-time guard in place it should never be reachable on a supported
combination.

Not investigated: whether a Zephyr RUST image on Cyclone (cargo road inside
Zephyr) installs the registrar; the downstream image is C++.

## Fix (in progress, PR 4223)

Roads that can build a Cyclone image, swept 2026-11-18:

- top-level cmake (`CMakeLists.txt`): native/posix, FreeRTOS (with the
  `integrations/freertos` and `integrations/s32ds` shells), ThreadX, PX4
  (`find_package(nano_ros)` bootstraps it), and the NuttX workspace road. All
  of them `add_subdirectory(<nano-ros>)`. This road already called the helper.
- Zephyr module (`zephyr/CMakeLists.txt`): C, C++ and generated Rust
  applications all link the C++ Cyclone backend. This road never called the
  helper, and now does: called directly in the module, which Zephyr
  adds from its own DEFER after the application's directory, through the same
  helper with `RMW` and `HOOK` arguments, and the
  `_nros_zephyr_runtime_typesupport_hook` adapter over the Zephyr per-message
  body.
- cargo: `nros-node` registers each type at create time through the
  backend's Rust registrar. This road was covered already, by construction.
- NuttX guest-make (`integrations/nuttx`) builds no Cyclone image at all, so
  it has nothing to hold. Its Kconfig offering Cyclone anyway is issue 2705.

The helper also generates `rcl_interfaces` for `rosout`.
`backend_link::runtime_interface_types_refusal`, beside the issue 2487
predicate, refuses an image at `nros build` when an enabled axis needs a
runtime package that is not on the interface index. The cmake helper's
FATAL_ERROR is the configure-time refusal for a direct cmake user. Gate
`check-runtime-interface-types-roads` holds every per-message generator's road
to a call of the helper.

One table for both roads: the axis -> packages -> registered types live in
`packages/core/nros-orchestration-ir/data/runtime-interface-types.json` only.
The cmake helper reads its `packages` (`string(JSON)`) and generates each
package whole, so the descriptor table records every type of it. The cargo
road reads its `registers` (`runtime_interface_types`, `include_str!`), the
types the executor creates, and sizes the Cyclone tables from that count
(`type_descriptor_sizing::infra_types`); the CLI refusal reads `packages`. The
gate refuses a package list spelled in the helper or in `backend_link.rs`, and
holds `registers` equal to the types at nros-node's own creation sites
(`create_param_srv`, `create_param_event_pub`, `create_rosout_pub`,
`create_lc_srv`, `create_lc_pub`), so a fourth runtime-owned type cannot land
on one road only.

Measured on the downstream FVP image (Zephyr C++, Cyclone, user-mode network):

- Run 1, with the call DEFERRED: the 12 `rcl_interfaces` service descriptors
  were linked, but the image printed `descriptor registry OVERFLOWED: 23 type
  registration(s) dropped at cap 259 (first dropped:
  'rcl_interfaces::msg::dds_::FloatingPointRange_')` followed by the same
  `Transport(Unsupported)`. The Cyclone table cap
  (`NrosRmwCycloneddsDescriptorCap.cmake`) is a DEFER that ran before the
  deferred call, so the table was sized without these types.
- Run 2, with the call made directly: the table was sized to 282 rows (259 +
  23), boot reached "Actuation Safety Island is Live" with no overflow line
  and no `parameter services could not be created` line, and the control loop
  kept running.

The cap now also refuses, at configure, a registration recorded after the
table was sized once it no longer fits, and names the type (case F in
`tests/cmake-cyclonedds-descriptor-cap-tests.sh`). Before, that case only
showed up at boot as the overflow line.

The cargo road registers at run time, with no configure step. There, a
registration that does not fit the C++ descriptor table is now REFUSED, not
dropped: `TypeRegistry` mirrors through
`nros_rmw_cyclonedds_try_register_descriptor`, which holds the type under its
one key (the descriptor's `m_typename`, the DDS form; see below) or not at all,
and returns `-2` when the table is full. The registry then forgets the type,
frees the descriptor it built (a refused registration may be retried every
spin), and returns `BuildError::DescriptorTableFull`; the seam fails the
entity create, and `rmw_type_registry::register_type` names the type. The
void entry stays for the static-init constructors, which cannot fail. Tests:
`descriptor_table_capacity` (C++: over the cap, refused, no slot taken, not
counted as a drop), `type_registry::tests::
descriptor_table_full_refuses_and_caches_nothing` and `descriptor_seam` (Rust,
bridge stub).

### Follow-up: one table row per type

Found reviewing the above: a cargo-road registration took TWO rows of the C++
table, one for the schema `TYPE_NAME` (`pkg/msg/Name`) and one for the built
descriptor's `m_typename` (`pkg::msg::dds_::Name_`), while
`derive_max_descriptor_types(counted)` sizes the table at `counted` rows. So
an image of more than half the table's types met the new refusal on a table
its own count had sized (at the 256 fallback: 129 types; above it: every
image the derivation sized). No in-tree image is that large (the largest
cargo-road Cyclone fixture is about 38 types).

Fixed by holding a registration under ONE key, `descriptor_key()`: the
descriptor's `m_typename`, or the caller's name when it has none. Every
production lookup already asks by the DDS form (`publisher.cpp`,
`subscriber.cpp` via `action_topic_type`, `service.cpp` via
`descriptors_for_service`, `graph.cpp`), and `find_descriptor` now normalises
a ROS-form ask with `ros_form_to_dds`, so both forms still resolve. The table
is a count of types on every road, with no keys-per-type factor anywhere to
drift. The backend's own graph type
(`rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_`, registered on every
road but in no model count) gets its own row on top
(`kBackendOwnDescriptorTypes`, which `graph.cpp` looks its type up by).
Test: `descriptor_table_cargo_road` (C++: half the image rows plus one, in
the cargo-road shape, all held, one row each, both forms resolve; refused
against the two-key table).
