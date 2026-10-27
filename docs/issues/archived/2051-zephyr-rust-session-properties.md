---
id: 2051
title: "A Zephyr Rust west application cannot bake its bringup's lowered RMW file yet"
status: resolved
type: limitation
area: [zephyr, config, rmw]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109, issue-1288]
resolved: 2026-10-27
---

## What

RFC-0109 D8 (phase-514 W3a): a bringup whose backend reads a
`session-json5` file (zenoh's `rmw/zenoh.json5`) has it lowered on the host
and baked into each image. The carriers are:

- cargo roads (single-package leaf, workspace cargo image): `nros sync` /
  `nros build` write the lowered list into the image's cargo settings as
  `NROS_SESSION_PROPERTIES`, which `nros::main!` reads at expansion;
- cmake roads, including a C/C++ Zephyr west application: `nano_ros_entry()`
  passes the file to `nros codegen entry --rmw-config`.

A **Rust** Zephyr west application (`rust_cargo_application()`) is on
neither: the west lane has no `--config` seam of its own (issue 1288), so
nothing puts `NROS_SESSION_PROPERTIES` in front of the cargo build Zephyr's
Rust support runs. Baking nothing would boot the image on the board's default
router while the bringup says otherwise, so `nros build` REFUSES such an
image, naming this issue.

## Fix shape

Forward `NROS_SESSION_PROPERTIES` on the Zephyr module's cargo command, the
way the module's knob resolver forwards `NROS_SIZING_DESCRIPTOR` (issue
1407), then drop the refusal in `nros-cli-core`'s west branch
(`cmd/build.rs`, "issue 2051").

## Resolution

Three defects stood between the bringup's file and the router, and each hid
the next:

1. **No carrier.** `nros build` now writes the lowered table as Rust
   (`session_properties::rust_table`) to `<resolve>/session-properties.rs`,
   always (empty for none), and `zephyr/CMakeLists.txt` puts its PATH on the
   Rust lane's cargo command as `NROS_SESSION_PROPERTIES_RS`. The `nros`
   crate's build copies it in (no new dependency, so no lock moves) and
   `zephyr_component_main!` installs it before the session opens. The text
   itself cannot ride that command: it is an unquoted shell line inside a
   CMake list, which stripped the JSON's `"` (measured — the build script got
   `[[connect,tcp/...]]`) and would split an ISO-TP endpoint's `;`.
2. **The Kconfig locator was dialed first.** A Zephyr image always bakes
   `CONFIG_NROS_ZENOH_LOCATOR` (default `tcp/127.0.0.1:7447`), and zenoh-pico
   tries endpoints in order, so a router there would win over the file's.
   The entry now drops the baked locator when the image has session
   properties (`Context::yield_locator_to_session_properties`) — RFC-0109 D9,
   as `nros::main!` and the C/C++ boot blob already do. A run-time
   `--nros-locator` still wins.
3. **`ZenohRmw::open` dropped every property.** It built the shim's config
   with `properties: &[]`, so a session opened through `nros-rmw-cffi`'s Rust
   adapter (the path a Zephyr image takes) reached zenoh-pico with none —
   measured in gdb: two properties at `open_marshalling_properties`, zero at
   `zpico_init_with_config`. It forwards them now
   (`the_rmw_configs_session_properties_reach_the_shim`).

Measured on `examples/zephyr/rust/talker` (native_sim) with an
`rmw/zenoh.json5` naming `tcp/127.0.0.1:7499`: the image dials only 7499 and
publishes through a router there; removing the file re-renders an empty table
on the next `nros build` and the image dials the Kconfig 7447 again.
