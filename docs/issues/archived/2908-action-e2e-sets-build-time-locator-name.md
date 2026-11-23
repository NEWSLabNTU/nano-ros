---
id: 2908
title: "Live-peer tests set NROS_* names the runtime no longer reads: the zenoh action cells passed the router as NROS_ENTRY_LOCATOR and dialled the default port, and the peer-mode pair lost its domain"
status: resolved
type: bug
area: [testing, interop]
severity: high
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "nros-tests: the sites set the names the runtime reads, and a lib test refuses a runtime `.env(\"NROS_*\")` name nothing reads at run time"
related: [2907]
---

## What failed

Live-peer dispatch 45148744207 failed both zenoh action cells in
`ros2_action_e2e`:

- `a_stock_ros2_client_drives_the_nano_ros_action_server_over_zenoh`
- `the_nano_ros_action_client_drives_a_stock_ros2_server_over_zenoh`

Both tests start a router on a random port and hand it to the nano-ros binary
as `.env("NROS_ENTRY_LOCATOR", &locator)`. A hosted image reads its locator at
run time through `nros::env`, which reads `NROS_LOCATOR`. `NROS_ENTRY_LOCATOR` is
the build-time carrier (`option_env!` in `nros/src/init.rs`, forwarded by
`nros/build.rs`). The knob rename moved the runtime name and left these two
sites on the build-time one. So the node booted with the backend's default
locator, dialled port 7447, and failed with `ConnectionFailed`.

## The same class, found by the check

The same rename stopped the runtime reading `NROS_DOMAIN_ID`; a hosted image
reads `ROS_DOMAIN_ID`. Nine more test sites set it:

- `nano2nano::test_peer_mode_communication` set ONLY `NROS_DOMAIN_ID` on its
  peer-mode listener and talker. Both ran on domain 0, so the test lost the
  per-test domain isolation `unique_ros_domain_id()` exists for, and could
  cross-talk with a parallel test. Live.
- `graph_interop` (3 sites), `params_per_node_interop` (2) and
  `rust_multi_node_per_node_graph` (3) set both names. The `NROS_DOMAIN_ID`
  line did nothing there. Dead.

## Fix

- The action cells set `NROS_LOCATOR`, and the peer pair sets `ROS_DOMAIN_ID`.
  The dead `NROS_DOMAIN_ID` lines are gone.
- `nros_tests::runtime_env_names` (a lib test, so it runs wherever the crate's
  unit tests run): every `.env("NROS_…"` call in `nros-tests` must name a
  variable some tracked source reads at run time. A read is a shell expansion
  (`$NAME`, `${NAME`; in CMake only `$ENV{NAME}`), or the quoted name beside a
  reader call (`getenv(`, `var(`, `var_os(`, a typed `env_<type>(` helper,
  Python's `environ`). A line that only bakes or writes the name is not a read.
  The first version also counted CMake's `${NAME}`, a CMake variable, and an
  `env_` prefix that matched an unrelated function name, so it passed both
  mutations. Both are now negative controls among its unit tests.
- Mutation-checked against the live tree: putting back
  `NROS_ENTRY_LOCATOR` at `ros2_action_e2e.rs:450`, or `NROS_DOMAIN_ID` at
  `nano2nano.rs:187`, turns it red and names the site.

Dynamic names (`cmd.env(name, …)` with `name` a variable) are not checked.
